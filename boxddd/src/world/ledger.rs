use super::ShapeResource;
use crate::core::provenance::{ContactEpoch, OwnerToken, ResourceToken, allocate_resource_token};
use crate::error::{Error, HandleKind, Result};
use crate::types::{BodyId, BodyKey, ContactId, ContactKey, JointId, JointKey, ShapeId, ShapeKey};
use boxddd_sys::ffi;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
struct CallbackIndexState {
    owner: OwnerToken,
    shapes: HashMap<ShapeKey, ResourceToken>,
}

#[derive(Clone, Debug)]
pub(crate) struct CallbackProvenanceIndex {
    inner: Arc<RwLock<CallbackIndexState>>,
}

impl CallbackProvenanceIndex {
    fn new(owner: OwnerToken) -> Self {
        Self {
            inner: Arc::new(RwLock::new(CallbackIndexState {
                owner,
                shapes: HashMap::new(),
            })),
        }
    }

    fn reserve_shape(&self) -> Result<()> {
        self.write()
            .shapes
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)
    }

    fn publish_shape(&self, raw: ffi::b3ShapeId, token: ResourceToken) {
        self.write().shapes.insert(ShapeKey::from_raw(raw), token);
    }

    fn retire_shape(&self, key: ShapeKey) {
        self.write().shapes.remove(&key);
    }

    fn clear(&self) {
        self.write().shapes.clear();
    }

    pub(crate) fn resolve_shape(&self, raw: ffi::b3ShapeId) -> Option<ShapeId> {
        let state = self.read();
        let token = state.shapes.get(&ShapeKey::from_raw(raw)).copied()?;
        Some(ShapeId::from_parts(raw, state.owner, token))
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, CallbackIndexState> {
        self.inner.read().unwrap_or_else(|error| error.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, CallbackIndexState> {
        self.inner
            .write()
            .unwrap_or_else(|error| error.into_inner())
    }
}

#[derive(Debug)]
struct ResourceRegistry<K> {
    active: HashMap<K, ResourceToken>,
    pending: HashMap<K, Observation<ResourceToken>>,
    visible: HashMap<K, Observation<ResourceToken>>,
}

impl<K> Default for ResourceRegistry<K> {
    fn default() -> Self {
        Self {
            active: HashMap::new(),
            pending: HashMap::new(),
            visible: HashMap::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Observation<T> {
    Unique(T),
    Ambiguous,
}

fn merge_observation<K, T>(map: &mut HashMap<K, Observation<T>>, key: K, value: T)
where
    K: Copy + Eq + Hash,
    T: Copy + Eq,
{
    match map.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(Observation::Unique(value));
        }
        Entry::Occupied(mut entry) => match entry.get() {
            Observation::Unique(current) if *current == value => {}
            Observation::Unique(_) => {
                entry.insert(Observation::Ambiguous);
            }
            Observation::Ambiguous => {}
        },
    }
}

impl<K: Copy + Eq + Hash> ResourceRegistry<K> {
    fn reserve<T>(&mut self) -> Result<PendingResource<T>> {
        self.active
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.pending
            .try_reserve(self.active.len() + 1)
            .map_err(|_| Error::AllocationFailed)?;
        self.visible
            .try_reserve(self.active.len() + 1)
            .map_err(|_| Error::AllocationFailed)?;
        Ok(PendingResource {
            token: allocate_resource_token()?,
            marker: PhantomData,
        })
    }

    fn publish<T>(&mut self, key: K, pending: PendingResource<T>) -> ResourceToken {
        let previous = self.active.insert(key, pending.token);
        debug_assert!(
            previous.is_none(),
            "native resource key reused while active"
        );
        pending.token
    }

    fn authorize(&self, key: K, token: ResourceToken, kind: HandleKind) -> Result<()> {
        match self.active.get(&key) {
            Some(current) if *current == token => Ok(()),
            _ => Err(Error::StaleHandle { kind }),
        }
    }

    fn resolve_active(&self, key: K, kind: HandleKind) -> Result<ResourceToken> {
        self.active
            .get(&key)
            .copied()
            .ok_or(Error::StaleHandle { kind })
    }

    fn resolve_observed(&self, key: K, kind: HandleKind) -> Result<ResourceToken> {
        let mut candidate = self.active.get(&key).copied();
        for observations in [&self.pending, &self.visible] {
            match observations.get(&key) {
                Some(Observation::Unique(token)) if candidate.is_none() => {
                    candidate = Some(*token);
                }
                Some(Observation::Unique(token)) if candidate == Some(*token) => {}
                Some(Observation::Unique(_) | Observation::Ambiguous) => {
                    return Err(Error::StaleHandle { kind });
                }
                None => {}
            }
        }
        candidate.ok_or(Error::StaleHandle { kind })
    }

    fn retire(&mut self, key: K) -> Option<ResourceToken> {
        let token = self.active.remove(&key)?;
        debug_assert!(
            self.pending.contains_key(&key) || self.pending.len() < self.pending.capacity()
        );
        merge_observation(&mut self.pending, key, token);
        Some(token)
    }

    fn rotate_event_window(&mut self) {
        self.visible.clear();
        std::mem::swap(&mut self.visible, &mut self.pending);
    }

    fn clear(&mut self) {
        self.active.clear();
        self.pending.clear();
        self.visible.clear();
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ContactObservation {
    token: ResourceToken,
    epoch: ContactEpoch,
}

#[derive(Debug, Default)]
struct ContactRegistry {
    active: HashMap<ContactKey, ResourceToken>,
    pending: HashMap<ContactKey, Observation<ContactObservation>>,
    visible: HashMap<ContactKey, Observation<ContactObservation>>,
}

impl ContactRegistry {
    fn authorize(&self, key: ContactKey, token: ResourceToken) -> bool {
        self.active
            .get(&key)
            .is_some_and(|current| *current == token)
    }

    fn resolve_current(&mut self, key: ContactKey) -> Result<ResourceToken> {
        if let Some(token) = self.active.get(&key) {
            return Ok(*token);
        }
        self.active
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.pending
            .try_reserve(self.active.len() + 1)
            .map_err(|_| Error::AllocationFailed)?;
        self.visible
            .try_reserve(self.active.len() + 1)
            .map_err(|_| Error::AllocationFailed)?;
        let token = allocate_resource_token()?;
        let previous = self.active.insert(key, token);
        debug_assert!(previous.is_none(), "contact published twice");
        Ok(token)
    }

    fn resolve_end(
        &mut self,
        key: ContactKey,
        fallback_epoch: ContactEpoch,
    ) -> Result<ContactObservation> {
        match self.visible.get(&key) {
            Some(Observation::Unique(observation)) => return Ok(*observation),
            Some(Observation::Ambiguous) => {
                return Err(Error::StaleHandle {
                    kind: HandleKind::Contact,
                });
            }
            None => {}
        }

        self.visible
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let observation = ContactObservation {
            token: allocate_resource_token()?,
            epoch: fallback_epoch,
        };
        merge_observation(&mut self.visible, key, observation);
        Ok(observation)
    }

    fn retire_active(&mut self, epoch: ContactEpoch) {
        for (key, token) in self.active.drain() {
            debug_assert!(
                self.pending.contains_key(&key) || self.pending.len() < self.pending.capacity()
            );
            merge_observation(&mut self.pending, key, ContactObservation { token, epoch });
        }
    }

    fn rotate_event_window(&mut self) {
        self.visible.clear();
        std::mem::swap(&mut self.visible, &mut self.pending);
    }

    fn clear(&mut self) {
        self.active.clear();
        self.pending.clear();
        self.visible.clear();
    }
}

#[derive(Debug)]
pub(crate) struct PendingResource<T> {
    token: ResourceToken,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug)]
pub(crate) enum BodyResource {}
#[derive(Debug)]
pub(crate) enum ShapeResourceKind {}
#[derive(Debug)]
pub(crate) enum JointResource {}

#[derive(Debug, Default)]
struct BodyRelations {
    shapes: HashSet<ShapeId>,
    joints: HashSet<JointId>,
}

#[derive(Debug)]
struct ShapeState {
    body: BodyId,
    resource: Option<ShapeResource>,
}

#[derive(Debug)]
struct JointState {
    body_a: BodyId,
    body_b: BodyId,
}

#[derive(Debug)]
pub(crate) struct PendingShape {
    resource: PendingResource<ShapeResourceKind>,
    body: BodyId,
    backing: Option<ShapeResource>,
}

#[derive(Debug)]
pub(crate) struct PendingJoint {
    resource: PendingResource<JointResource>,
    body_a: BodyId,
    body_b: BodyId,
}

#[derive(Debug)]
pub(crate) struct BodyCascade {
    shapes: Vec<ShapeId>,
    joints: Vec<JointId>,
}

#[derive(Debug)]
pub(crate) struct WorldLedger {
    owner: OwnerToken,
    bodies: ResourceRegistry<BodyKey>,
    shapes: ResourceRegistry<ShapeKey>,
    joints: ResourceRegistry<JointKey>,
    contacts: RefCell<ContactRegistry>,
    body_relations: HashMap<BodyId, BodyRelations>,
    shape_states: HashMap<ShapeId, ShapeState>,
    joint_states: HashMap<JointId, JointState>,
    contact_epoch: ContactEpoch,
    visible_contact_epoch: ContactEpoch,
    callback_index: CallbackProvenanceIndex,
}

impl WorldLedger {
    pub(crate) fn new(owner: OwnerToken) -> Self {
        Self {
            owner,
            bodies: ResourceRegistry::default(),
            shapes: ResourceRegistry::default(),
            joints: ResourceRegistry::default(),
            contacts: RefCell::new(ContactRegistry::default()),
            body_relations: HashMap::new(),
            shape_states: HashMap::new(),
            joint_states: HashMap::new(),
            contact_epoch: ContactEpoch::INITIAL,
            visible_contact_epoch: ContactEpoch::INITIAL,
            callback_index: CallbackProvenanceIndex::new(owner),
        }
    }

    pub(crate) fn callback_index(&self) -> CallbackProvenanceIndex {
        self.callback_index.clone()
    }

    pub(crate) fn reserve_body(&mut self) -> Result<PendingResource<BodyResource>> {
        self.body_relations
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.bodies.reserve()
    }

    pub(crate) fn reserve_shape(
        &mut self,
        body: BodyId,
        backing: Option<ShapeResource>,
    ) -> Result<PendingShape> {
        self.authorize_body(body)?;
        self.callback_index.reserve_shape()?;
        self.shape_states
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.body_relations
            .get_mut(&body)
            .ok_or(Error::StaleHandle {
                kind: HandleKind::Body,
            })?
            .shapes
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        Ok(PendingShape {
            resource: self.shapes.reserve()?,
            body,
            backing,
        })
    }

    pub(crate) fn reserve_joint(&mut self, body_a: BodyId, body_b: BodyId) -> Result<PendingJoint> {
        self.authorize_body(body_a)?;
        self.authorize_body(body_b)?;
        self.joint_states
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.body_relations
            .get_mut(&body_a)
            .ok_or(Error::StaleHandle {
                kind: HandleKind::Body,
            })?
            .joints
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        if body_a != body_b {
            self.body_relations
                .get_mut(&body_b)
                .ok_or(Error::StaleHandle {
                    kind: HandleKind::Body,
                })?
                .joints
                .try_reserve(1)
                .map_err(|_| Error::AllocationFailed)?;
        }
        Ok(PendingJoint {
            resource: self.joints.reserve()?,
            body_a,
            body_b,
        })
    }

    pub(crate) fn publish_body(
        &mut self,
        raw: ffi::b3BodyId,
        pending: PendingResource<BodyResource>,
    ) -> BodyId {
        let token = self.bodies.publish(BodyKey::from_raw(raw), pending);
        let id = BodyId::from_parts(raw, self.owner, token);
        let previous = self.body_relations.insert(id, BodyRelations::default());
        debug_assert!(previous.is_none(), "body relation published twice");
        id
    }

    pub(crate) fn publish_shape(&mut self, raw: ffi::b3ShapeId, pending: PendingShape) -> ShapeId {
        let PendingShape {
            resource,
            body,
            backing,
        } = pending;
        let token = self.shapes.publish(ShapeKey::from_raw(raw), resource);
        self.callback_index.publish_shape(raw, token);
        let id = ShapeId::from_parts(raw, self.owner, token);
        let previous = self.shape_states.insert(
            id,
            ShapeState {
                body,
                resource: backing,
            },
        );
        debug_assert!(previous.is_none(), "shape state published twice");
        let inserted = self
            .body_relations
            .get_mut(&body)
            .expect("shape parent body was reserved")
            .shapes
            .insert(id);
        debug_assert!(inserted, "shape relation published twice");
        id
    }

    pub(crate) fn publish_joint(&mut self, raw: ffi::b3JointId, pending: PendingJoint) -> JointId {
        let PendingJoint {
            resource,
            body_a,
            body_b,
        } = pending;
        let token = self.joints.publish(JointKey::from_raw(raw), resource);
        let id = JointId::from_parts(raw, self.owner, token);
        let previous = self.joint_states.insert(id, JointState { body_a, body_b });
        debug_assert!(previous.is_none(), "joint state published twice");
        let inserted_a = self
            .body_relations
            .get_mut(&body_a)
            .expect("joint body A was reserved")
            .joints
            .insert(id);
        debug_assert!(inserted_a, "joint relation published twice");
        if body_a != body_b {
            let inserted_b = self
                .body_relations
                .get_mut(&body_b)
                .expect("joint body B was reserved")
                .joints
                .insert(id);
            debug_assert!(inserted_b, "joint relation published twice");
        }
        id
    }

    pub(crate) fn authorize_body(&self, id: BodyId) -> Result<ffi::b3BodyId> {
        self.check_owner(id.owner_token(), HandleKind::Body)?;
        self.bodies
            .authorize(id.key(), id.resource_token(), HandleKind::Body)?;
        Ok(id.into_raw())
    }

    pub(crate) fn authorize_shape(&self, id: ShapeId) -> Result<ffi::b3ShapeId> {
        self.check_owner(id.owner_token(), HandleKind::Shape)?;
        self.shapes
            .authorize(id.key(), id.resource_token(), HandleKind::Shape)?;
        Ok(id.into_raw())
    }

    pub(crate) fn authorize_joint(&self, id: JointId) -> Result<ffi::b3JointId> {
        self.check_owner(id.owner_token(), HandleKind::Joint)?;
        self.joints
            .authorize(id.key(), id.resource_token(), HandleKind::Joint)?;
        Ok(id.into_raw())
    }

    pub(crate) fn authorize_contact(&self, id: ContactId) -> Result<ffi::b3ContactId> {
        self.check_owner(id.owner_token(), HandleKind::Contact)?;
        if id.epoch() != self.contact_epoch {
            return Err(Error::StaleHandle {
                kind: HandleKind::Contact,
            });
        }
        if !self
            .contacts
            .borrow()
            .authorize(ContactKey::from_raw(id.into_raw()), id.resource_token())
        {
            return Err(Error::StaleHandle {
                kind: HandleKind::Contact,
            });
        }
        Ok(id.into_raw())
    }

    pub(crate) fn resolve_body(&self, raw: ffi::b3BodyId) -> Result<BodyId> {
        let token = self
            .bodies
            .resolve_active(BodyKey::from_raw(raw), HandleKind::Body)?;
        Ok(BodyId::from_parts(raw, self.owner, token))
    }

    pub(crate) fn resolve_observed_body(&self, raw: ffi::b3BodyId) -> Result<BodyId> {
        let token = self
            .bodies
            .resolve_observed(BodyKey::from_raw(raw), HandleKind::Body)?;
        Ok(BodyId::from_parts(raw, self.owner, token))
    }

    pub(crate) fn resolve_shape(&self, raw: ffi::b3ShapeId) -> Result<ShapeId> {
        let token = self
            .shapes
            .resolve_active(ShapeKey::from_raw(raw), HandleKind::Shape)?;
        Ok(ShapeId::from_parts(raw, self.owner, token))
    }

    pub(crate) fn resolve_observed_shape(&self, raw: ffi::b3ShapeId) -> Result<ShapeId> {
        let token = self
            .shapes
            .resolve_observed(ShapeKey::from_raw(raw), HandleKind::Shape)?;
        Ok(ShapeId::from_parts(raw, self.owner, token))
    }

    pub(crate) fn resolve_joint(&self, raw: ffi::b3JointId) -> Result<JointId> {
        let token = self
            .joints
            .resolve_active(JointKey::from_raw(raw), HandleKind::Joint)?;
        Ok(JointId::from_parts(raw, self.owner, token))
    }

    pub(crate) fn resolve_observed_joint(&self, raw: ffi::b3JointId) -> Result<JointId> {
        let token = self
            .joints
            .resolve_observed(JointKey::from_raw(raw), HandleKind::Joint)?;
        Ok(JointId::from_parts(raw, self.owner, token))
    }

    pub(crate) fn resolve_contact(&self, raw: ffi::b3ContactId) -> Result<ContactId> {
        let token = self
            .contacts
            .borrow_mut()
            .resolve_current(ContactKey::from_raw(raw))?;
        Ok(ContactId::from_parts(
            raw,
            self.owner,
            token,
            self.contact_epoch,
        ))
    }

    pub(crate) fn resolve_contact_end(&self, raw: ffi::b3ContactId) -> Result<ContactId> {
        let observation = self
            .contacts
            .borrow_mut()
            .resolve_end(ContactKey::from_raw(raw), self.visible_contact_epoch)?;
        Ok(ContactId::from_parts(
            raw,
            self.owner,
            observation.token,
            observation.epoch,
        ))
    }

    pub(crate) fn prepare_body_cascade(&self, id: BodyId) -> Result<BodyCascade> {
        self.authorize_body(id)?;
        let relations = self.body_relations.get(&id).ok_or(Error::StaleHandle {
            kind: HandleKind::Body,
        })?;
        let mut shapes = Vec::new();
        shapes
            .try_reserve(relations.shapes.len())
            .map_err(|_| Error::AllocationFailed)?;
        shapes.extend(relations.shapes.iter().copied());
        let mut joints = Vec::new();
        joints
            .try_reserve(relations.joints.len())
            .map_err(|_| Error::AllocationFailed)?;
        joints.extend(relations.joints.iter().copied());
        Ok(BodyCascade { shapes, joints })
    }

    pub(crate) fn body_shapes(&self, id: BodyId) -> Result<Vec<ShapeId>> {
        Ok(self.prepare_body_cascade(id)?.shapes)
    }

    pub(crate) fn finish_body_cascade(&mut self, id: BodyId, cascade: BodyCascade) {
        for shape in cascade.shapes {
            self.retire_shape(shape);
        }
        for joint in cascade.joints {
            self.retire_joint(joint);
        }
        self.body_relations.remove(&id);
        self.bodies.retire(id.key());
    }

    pub(crate) fn retire_shape(&mut self, id: ShapeId) -> Option<ShapeResource> {
        let state = self.shape_states.remove(&id);
        if let Some(state) = state.as_ref() {
            if let Some(body) = self.body_relations.get_mut(&state.body) {
                body.shapes.remove(&id);
            }
        }
        self.shapes.retire(id.key());
        self.callback_index.retire_shape(id.key());
        state.and_then(|state| state.resource)
    }

    pub(crate) fn retire_joint(&mut self, id: JointId) {
        let state = self.joint_states.remove(&id);
        if let Some(state) = state.as_ref() {
            if let Some(body) = self.body_relations.get_mut(&state.body_a) {
                body.joints.remove(&id);
            }
            if state.body_a != state.body_b {
                if let Some(body) = self.body_relations.get_mut(&state.body_b) {
                    body.joints.remove(&id);
                }
            }
        }
        self.joints.retire(id.key());
    }

    pub(crate) fn shape_resource(&self, id: ShapeId) -> Option<&ShapeResource> {
        self.shape_states.get(&id)?.resource.as_ref()
    }

    pub(crate) fn replace_shape_resource(
        &mut self,
        id: ShapeId,
        resource: ShapeResource,
    ) -> Option<ShapeResource> {
        self.shape_states
            .get_mut(&id)
            .expect("shape was authorized before resource replacement")
            .resource
            .replace(resource)
    }

    pub(crate) fn clear_shape_resource(&mut self, id: ShapeId) -> Option<ShapeResource> {
        self.shape_states
            .get_mut(&id)
            .expect("shape was authorized before resource replacement")
            .resource
            .take()
    }

    #[cfg(test)]
    pub(crate) fn shape_resource_count(&self) -> usize {
        self.shape_states
            .values()
            .filter(|state| state.resource.is_some())
            .count()
    }

    pub(crate) fn finish_drop(&mut self) {
        self.callback_index.clear();
        self.joint_states.clear();
        self.shape_states.clear();
        self.body_relations.clear();
        self.joints.clear();
        self.shapes.clear();
        self.bodies.clear();
        self.contacts.get_mut().clear();
    }

    pub(crate) fn prepare_contact_turnover(&self) -> Result<ContactEpoch> {
        self.contact_epoch.next()
    }

    pub(crate) fn finish_contact_turnover(&mut self, next: ContactEpoch) {
        self.contacts.get_mut().retire_active(self.contact_epoch);
        self.contact_epoch = next;
    }

    pub(crate) fn finish_step(&mut self, next: ContactEpoch) {
        let visible_contact_epoch = self.contact_epoch;
        let contacts = self.contacts.get_mut();
        contacts.retire_active(visible_contact_epoch);
        contacts.rotate_event_window();
        self.bodies.rotate_event_window();
        self.shapes.rotate_event_window();
        self.joints.rotate_event_window();
        self.visible_contact_epoch = visible_contact_epoch;
        self.contact_epoch = next;
    }

    fn check_owner(&self, owner: OwnerToken, kind: HandleKind) -> Result<()> {
        if owner == self.owner {
            Ok(())
        } else {
            Err(Error::ForeignHandle { kind })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::provenance::allocate_owner_token;

    fn body_raw() -> ffi::b3BodyId {
        ffi::b3BodyId {
            index1: 1,
            world0: 0,
            generation: 7,
        }
    }

    fn contact_raw() -> ffi::b3ContactId {
        ffi::b3ContactId {
            index1: 1,
            world0: 0,
            padding: 0,
            generation: 11,
        }
    }

    fn shape_raw() -> ffi::b3ShapeId {
        ffi::b3ShapeId {
            index1: 2,
            world0: 0,
            generation: 9,
        }
    }

    fn joint_raw() -> ffi::b3JointId {
        ffi::b3JointId {
            index1: 3,
            world0: 0,
            generation: 13,
        }
    }

    #[test]
    fn identical_native_bits_do_not_revive_retired_resource_tokens() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let pending = ledger.reserve_body().unwrap();
        let old = ledger.publish_body(body_raw(), pending);
        let cascade = ledger.prepare_body_cascade(old).unwrap();
        ledger.finish_body_cascade(old, cascade);
        let pending = ledger.reserve_body().unwrap();
        let replacement = ledger.publish_body(body_raw(), pending);

        assert_ne!(old, replacement);
        assert!(matches!(
            ledger.authorize_body(old),
            Err(Error::StaleHandle {
                kind: HandleKind::Body
            })
        ));
        let replacement_raw = ledger.authorize_body(replacement).unwrap();
        assert_eq!(replacement_raw.index1, body_raw().index1);
        assert_eq!(replacement_raw.world0, body_raw().world0);
        assert_eq!(replacement_raw.generation, body_raw().generation);
    }

    #[test]
    fn retired_resource_provenance_is_visible_for_one_completed_step() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let pending = ledger.reserve_body().unwrap();
        let body = ledger.publish_body(body_raw(), pending);
        let cascade = ledger.prepare_body_cascade(body).unwrap();
        ledger.finish_body_cascade(body, cascade);

        assert_eq!(ledger.resolve_observed_body(body_raw()).unwrap(), body);

        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);
        assert_eq!(ledger.resolve_observed_body(body_raw()).unwrap(), body);

        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);
        assert!(matches!(
            ledger.resolve_observed_body(body_raw()),
            Err(Error::StaleHandle {
                kind: HandleKind::Body
            })
        ));
    }

    #[test]
    fn resource_key_reuse_is_ambiguous_until_the_event_window_expires() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let pending = ledger.reserve_body().unwrap();
        let old = ledger.publish_body(body_raw(), pending);
        let cascade = ledger.prepare_body_cascade(old).unwrap();
        ledger.finish_body_cascade(old, cascade);

        let pending = ledger.reserve_body().unwrap();
        let replacement = ledger.publish_body(body_raw(), pending);
        assert_ne!(old, replacement);
        assert!(matches!(
            ledger.resolve_observed_body(body_raw()),
            Err(Error::StaleHandle {
                kind: HandleKind::Body
            })
        ));

        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);
        assert!(matches!(
            ledger.resolve_observed_body(body_raw()),
            Err(Error::StaleHandle {
                kind: HandleKind::Body
            })
        ));

        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);
        assert_eq!(
            ledger.resolve_observed_body(body_raw()).unwrap(),
            replacement
        );
    }

    #[test]
    fn identical_native_bits_from_another_owner_are_foreign() {
        let mut first = WorldLedger::new(allocate_owner_token().unwrap());
        let mut second = WorldLedger::new(allocate_owner_token().unwrap());
        let pending = first.reserve_body().unwrap();
        let body = first.publish_body(body_raw(), pending);
        let pending = second.reserve_body().unwrap();
        let _other = second.publish_body(body_raw(), pending);

        assert!(matches!(
            second.authorize_body(body),
            Err(Error::ForeignHandle {
                kind: HandleKind::Body
            })
        ));
    }

    #[test]
    fn contact_epoch_turnover_rejects_identical_native_bits() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let old = ledger.resolve_contact(contact_raw()).unwrap();
        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_contact_turnover(next);
        let replacement = ledger.resolve_contact(contact_raw()).unwrap();

        assert_ne!(old, replacement);
        assert!(matches!(
            ledger.authorize_contact(old),
            Err(Error::StaleHandle {
                kind: HandleKind::Contact
            })
        ));
        let replacement_raw = ledger.authorize_contact(replacement).unwrap();
        assert_eq!(replacement_raw.index1, contact_raw().index1);
        assert_eq!(replacement_raw.world0, contact_raw().world0);
        assert_eq!(replacement_raw.generation, contact_raw().generation);
    }

    #[test]
    fn contact_end_resolution_preserves_the_retired_identity() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let old = ledger.resolve_contact(contact_raw()).unwrap();
        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_contact_turnover(next);
        assert!(matches!(
            ledger.authorize_contact(old),
            Err(Error::StaleHandle {
                kind: HandleKind::Contact
            })
        ));

        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);
        let end = ledger.resolve_contact_end(contact_raw()).unwrap();
        let repeated = ledger.resolve_contact_end(contact_raw()).unwrap();

        assert_eq!(end, old);
        assert_eq!(repeated, old);
        assert!(matches!(
            ledger.authorize_contact(end),
            Err(Error::StaleHandle {
                kind: HandleKind::Contact
            })
        ));

        let current = ledger.resolve_contact(contact_raw()).unwrap();
        assert_ne!(current, end);
        assert!(ledger.authorize_contact(current).is_ok());
    }

    #[test]
    fn unobserved_contact_end_is_retired_without_becoming_active() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);

        let end = ledger.resolve_contact_end(contact_raw()).unwrap();
        assert_eq!(ledger.resolve_contact_end(contact_raw()).unwrap(), end);
        assert!(matches!(
            ledger.authorize_contact(end),
            Err(Error::StaleHandle {
                kind: HandleKind::Contact
            })
        ));

        let current = ledger.resolve_contact(contact_raw()).unwrap();
        assert_ne!(current, end);
        assert!(ledger.authorize_contact(current).is_ok());
    }

    #[test]
    fn ambiguous_contact_turnovers_fail_closed_for_end_events() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let first = ledger.resolve_contact(contact_raw()).unwrap();
        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_contact_turnover(next);
        let second = ledger.resolve_contact(contact_raw()).unwrap();
        assert_ne!(first, second);
        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_contact_turnover(next);

        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_step(next);
        assert!(matches!(
            ledger.resolve_contact_end(contact_raw()),
            Err(Error::StaleHandle {
                kind: HandleKind::Contact
            })
        ));
    }

    #[test]
    fn body_cascade_closes_shape_and_both_joint_edges() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let pending_body_a = ledger.reserve_body().unwrap();
        let body_a = ledger.publish_body(body_raw(), pending_body_a);
        let mut second_raw = body_raw();
        second_raw.index1 = 2;
        let pending_body_b = ledger.reserve_body().unwrap();
        let body_b = ledger.publish_body(second_raw, pending_body_b);
        let pending_shape = ledger.reserve_shape(body_a, None).unwrap();
        let shape = ledger.publish_shape(shape_raw(), pending_shape);
        let pending_joint = ledger.reserve_joint(body_a, body_b).unwrap();
        let joint = ledger.publish_joint(joint_raw(), pending_joint);

        let body_b_before = ledger.prepare_body_cascade(body_b).unwrap();
        assert_eq!(body_b_before.joints, vec![joint]);
        let cascade = ledger.prepare_body_cascade(body_a).unwrap();
        assert_eq!(cascade.shapes, vec![shape]);
        assert_eq!(cascade.joints, vec![joint]);
        ledger.finish_body_cascade(body_a, cascade);

        assert!(matches!(
            ledger.authorize_body(body_a),
            Err(Error::StaleHandle {
                kind: HandleKind::Body
            })
        ));
        assert!(matches!(
            ledger.authorize_shape(shape),
            Err(Error::StaleHandle {
                kind: HandleKind::Shape
            })
        ));
        assert!(matches!(
            ledger.authorize_joint(joint),
            Err(Error::StaleHandle {
                kind: HandleKind::Joint
            })
        ));
        let body_b_after = ledger.prepare_body_cascade(body_b).unwrap();
        assert!(body_b_after.joints.is_empty());
    }

    #[test]
    fn discarded_staging_does_not_publish_graph_membership() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let pending_body = ledger.reserve_body().unwrap();
        let body = ledger.publish_body(body_raw(), pending_body);

        drop(ledger.reserve_shape(body, None).unwrap());
        let mut second_raw = body_raw();
        second_raw.index1 = 2;
        let pending_body_b = ledger.reserve_body().unwrap();
        let body_b = ledger.publish_body(second_raw, pending_body_b);
        drop(ledger.reserve_joint(body, body_b).unwrap());

        let body_relations = ledger.prepare_body_cascade(body).unwrap();
        assert!(body_relations.shapes.is_empty());
        assert!(body_relations.joints.is_empty());
        let body_b_relations = ledger.prepare_body_cascade(body_b).unwrap();
        assert!(body_b_relations.joints.is_empty());
    }

    #[test]
    fn current_epoch_contact_resolution_needs_no_publication_step() {
        let ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let contact = ledger.resolve_contact(contact_raw()).unwrap();
        let repeated = ledger.resolve_contact(contact_raw()).unwrap();

        assert!(ledger.authorize_contact(contact).is_ok());
        assert_eq!(contact, repeated);
    }

    #[test]
    fn contact_resource_token_rejects_same_epoch_identical_bits_after_reconciliation() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let old = ledger.resolve_contact(contact_raw()).unwrap();
        ledger.contacts.get_mut().clear();
        let replacement = ledger.resolve_contact(contact_raw()).unwrap();

        assert_ne!(old, replacement);
        assert!(matches!(
            ledger.authorize_contact(old),
            Err(Error::StaleHandle {
                kind: HandleKind::Contact
            })
        ));
        assert!(ledger.authorize_contact(replacement).is_ok());
    }
}
