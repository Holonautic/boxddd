use crate::core::provenance::{ContactEpoch, OwnerToken, ResourceToken, allocate_resource_token};
use crate::error::{Error, HandleKind, Result};
use crate::types::{BodyId, BodyKey, ContactId, ContactKey, JointId, JointKey, ShapeId, ShapeKey};
use boxddd_sys::ffi;
use std::collections::HashMap;
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
    retired: HashMap<K, ResourceToken>,
}

impl<K> Default for ResourceRegistry<K> {
    fn default() -> Self {
        Self {
            active: HashMap::new(),
            retired: HashMap::new(),
        }
    }
}

impl<K: Copy + Eq + Hash> ResourceRegistry<K> {
    fn reserve<T>(&mut self) -> Result<PendingResource<T>> {
        self.active
            .try_reserve(1)
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
        self.retired.remove(&key);
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
        self.active
            .get(&key)
            .or_else(|| self.retired.get(&key))
            .copied()
            .ok_or(Error::StaleHandle { kind })
    }

    fn retire(&mut self, key: K) -> Option<ResourceToken> {
        let token = self.active.remove(&key)?;
        self.retired.insert(key, token);
        Some(token)
    }

    fn clear_retired(&mut self) {
        self.retired.clear();
    }
}

#[derive(Debug)]
struct ContactEntry {
    token: ResourceToken,
    epoch: ContactEpoch,
    active: bool,
}

#[derive(Debug)]
pub(crate) struct PendingResource<T> {
    token: ResourceToken,
    marker: PhantomData<fn() -> T>,
}

pub(crate) enum BodyResource {}
pub(crate) enum ShapeResourceKind {}
pub(crate) enum JointResource {}

#[derive(Debug)]
pub(crate) struct WorldLedger {
    owner: OwnerToken,
    bodies: ResourceRegistry<BodyKey>,
    shapes: ResourceRegistry<ShapeKey>,
    joints: ResourceRegistry<JointKey>,
    contacts: HashMap<ContactKey, ContactEntry>,
    contact_epoch: ContactEpoch,
    callback_index: CallbackProvenanceIndex,
}

impl WorldLedger {
    pub(crate) fn new(owner: OwnerToken) -> Self {
        Self {
            owner,
            bodies: ResourceRegistry::default(),
            shapes: ResourceRegistry::default(),
            joints: ResourceRegistry::default(),
            contacts: HashMap::new(),
            contact_epoch: ContactEpoch::INITIAL,
            callback_index: CallbackProvenanceIndex::new(owner),
        }
    }

    pub(crate) fn callback_index(&self) -> CallbackProvenanceIndex {
        self.callback_index.clone()
    }

    pub(crate) fn reserve_body(&mut self) -> Result<PendingResource<BodyResource>> {
        self.bodies.reserve()
    }

    pub(crate) fn reserve_shape(&mut self) -> Result<PendingResource<ShapeResourceKind>> {
        self.callback_index.reserve_shape()?;
        self.shapes.reserve()
    }

    pub(crate) fn reserve_joint(&mut self) -> Result<PendingResource<JointResource>> {
        self.joints.reserve()
    }

    pub(crate) fn publish_body(
        &mut self,
        raw: ffi::b3BodyId,
        pending: PendingResource<BodyResource>,
    ) -> BodyId {
        let token = self.bodies.publish(BodyKey::from_raw(raw), pending);
        BodyId::from_parts(raw, self.owner, token)
    }

    pub(crate) fn publish_shape(
        &mut self,
        raw: ffi::b3ShapeId,
        pending: PendingResource<ShapeResourceKind>,
    ) -> ShapeId {
        let token = self.shapes.publish(ShapeKey::from_raw(raw), pending);
        self.callback_index.publish_shape(raw, token);
        ShapeId::from_parts(raw, self.owner, token)
    }

    pub(crate) fn publish_joint(
        &mut self,
        raw: ffi::b3JointId,
        pending: PendingResource<JointResource>,
    ) -> JointId {
        let token = self.joints.publish(JointKey::from_raw(raw), pending);
        JointId::from_parts(raw, self.owner, token)
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
        let current = self.contacts.get(&id.key()).ok_or(Error::StaleHandle {
            kind: HandleKind::Contact,
        })?;
        if current.token != id.resource_token() || current.epoch != id.epoch() || !current.active {
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

    pub(crate) fn active_body_raws(&self) -> Result<Vec<ffi::b3BodyId>> {
        let mut bodies = Vec::new();
        bodies
            .try_reserve(self.bodies.active.len())
            .map_err(|_| Error::AllocationFailed)?;
        bodies.extend(self.bodies.active.keys().map(|key| key.into_raw()));
        Ok(bodies)
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

    pub(crate) fn observe_contact(
        &mut self,
        raw: ffi::b3ContactId,
        active: bool,
    ) -> Result<ContactId> {
        let key = ContactKey::from_raw(raw);
        if let Some(entry) = self.contacts.get_mut(&key) {
            entry.active |= active;
            return Ok(ContactId::from_parts(
                raw,
                self.owner,
                entry.token,
                entry.epoch,
            ));
        }
        self.contacts
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let token = allocate_resource_token()?;
        self.contacts.insert(
            key,
            ContactEntry {
                token,
                epoch: self.contact_epoch,
                active,
            },
        );
        Ok(ContactId::from_parts(
            raw,
            self.owner,
            token,
            self.contact_epoch,
        ))
    }

    pub(crate) fn resolve_contact(&self, raw: ffi::b3ContactId) -> Result<ContactId> {
        let entry = self
            .contacts
            .get(&ContactKey::from_raw(raw))
            .ok_or(Error::StaleHandle {
                kind: HandleKind::Contact,
            })?;
        Ok(ContactId::from_parts(
            raw,
            self.owner,
            entry.token,
            entry.epoch,
        ))
    }

    pub(crate) fn retire_body(&mut self, id: BodyId) {
        self.bodies.retire(id.key());
    }

    pub(crate) fn retire_shape(&mut self, id: ShapeId) {
        self.shapes.retire(id.key());
        self.callback_index.retire_shape(id.key());
    }

    pub(crate) fn retire_joint(&mut self, id: JointId) {
        self.joints.retire(id.key());
    }

    pub(crate) fn prepare_contact_turnover(&self) -> Result<ContactEpoch> {
        self.contact_epoch.next()
    }

    pub(crate) fn finish_contact_turnover(&mut self, next: ContactEpoch) {
        self.contact_epoch = next;
        self.contacts.clear();
        self.bodies.clear_retired();
        self.shapes.clear_retired();
        self.joints.clear_retired();
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

    #[test]
    fn identical_native_bits_do_not_revive_retired_resource_tokens() {
        let mut ledger = WorldLedger::new(allocate_owner_token().unwrap());
        let pending = ledger.reserve_body().unwrap();
        let old = ledger.publish_body(body_raw(), pending);
        ledger.retire_body(old);
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
        let old = ledger.observe_contact(contact_raw(), true).unwrap();
        let next = ledger.prepare_contact_turnover().unwrap();
        ledger.finish_contact_turnover(next);
        let replacement = ledger.observe_contact(contact_raw(), true).unwrap();

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
}
