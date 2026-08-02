use super::ledger::{AvailableIdentity, IdentityClassification};
use crate::error::{Error, Result};
use crate::types::{BodyKey, JointKey, ShapeKey};
use boxddd_sys::ffi;
use std::cell::Cell;
use std::marker::PhantomData;

pub(crate) enum BodyNative {}
pub(crate) enum ShapeNative {}
pub(crate) enum JointNative {}

pub(crate) enum CompensationProof {
    RestoresFullSemantics,
    CannotVerifyFullRestoration,
}

pub(crate) trait NativeResource {
    type Raw: Copy;
    type Key: Copy;
    type Compensation: Copy;

    const COMPENSATION_PROOF: CompensationProof;

    fn is_empty(raw: Self::Raw) -> bool;
    fn world_slot(raw: Self::Raw) -> u16;
    unsafe fn is_valid(raw: Self::Raw) -> bool;
    unsafe fn world(raw: Self::Raw) -> ffi::b3WorldId;
    unsafe fn destroy(raw: Self::Raw, compensation: Self::Compensation);
}

impl NativeResource for BodyNative {
    type Raw = ffi::b3BodyId;
    type Key = BodyKey;
    type Compensation = ();

    const COMPENSATION_PROOF: CompensationProof = CompensationProof::RestoresFullSemantics;

    fn is_empty(raw: Self::Raw) -> bool {
        raw.index1 == 0
    }

    fn world_slot(raw: Self::Raw) -> u16 {
        raw.world0
    }

    unsafe fn is_valid(raw: Self::Raw) -> bool {
        unsafe { ffi::b3Body_IsValid(raw) }
    }

    unsafe fn world(raw: Self::Raw) -> ffi::b3WorldId {
        unsafe { ffi::b3Body_GetWorld(raw) }
    }

    unsafe fn destroy(raw: Self::Raw, (): Self::Compensation) {
        unsafe { ffi::b3DestroyBody(raw) };
    }
}

impl NativeResource for ShapeNative {
    type Raw = ffi::b3ShapeId;
    type Key = ShapeKey;
    type Compensation = bool;

    const COMPENSATION_PROOF: CompensationProof = CompensationProof::RestoresFullSemantics;

    fn is_empty(raw: Self::Raw) -> bool {
        raw.index1 == 0
    }

    fn world_slot(raw: Self::Raw) -> u16 {
        raw.world0
    }

    unsafe fn is_valid(raw: Self::Raw) -> bool {
        unsafe { ffi::b3Shape_IsValid(raw) }
    }

    unsafe fn world(raw: Self::Raw) -> ffi::b3WorldId {
        unsafe { ffi::b3Shape_GetWorld(raw) }
    }

    unsafe fn destroy(raw: Self::Raw, update_body_mass: Self::Compensation) {
        unsafe { ffi::b3DestroyShape(raw, update_body_mass) };
        #[cfg(test)]
        crate::shapes::record_shape_drop(crate::shapes::ShapeDropEvent::NativeShape);
    }
}

impl NativeResource for JointNative {
    type Raw = ffi::b3JointId;
    type Key = JointKey;
    type Compensation = ();

    const COMPENSATION_PROOF: CompensationProof = CompensationProof::CannotVerifyFullRestoration;

    fn is_empty(raw: Self::Raw) -> bool {
        raw.index1 == 0
    }

    fn world_slot(raw: Self::Raw) -> u16 {
        raw.world0
    }

    unsafe fn is_valid(raw: Self::Raw) -> bool {
        unsafe { ffi::b3Joint_IsValid(raw) }
    }

    unsafe fn world(raw: Self::Raw) -> ffi::b3WorldId {
        unsafe { ffi::b3Joint_GetWorld(raw) }
    }

    unsafe fn destroy(raw: Self::Raw, (): Self::Compensation) {
        unsafe { ffi::b3DestroyJoint(raw, false) };
    }
}

pub(crate) enum Claimed {}
pub(crate) enum Bound {}

pub(crate) struct NativeCreationInput<'a, K: NativeResource> {
    raw: K::Raw,
    compensation: K::Compensation,
    target_world: ffi::b3WorldId,
    owner_poisoned: &'a Cell<bool>,
    identity: IdentityClassification<K::Key>,
}

impl<'a, K: NativeResource> NativeCreationInput<'a, K> {
    pub(crate) fn new(
        raw: K::Raw,
        compensation: K::Compensation,
        target_world: ffi::b3WorldId,
        owner_poisoned: &'a Cell<bool>,
        identity: IdentityClassification<K::Key>,
    ) -> Self {
        Self {
            raw,
            compensation,
            target_world,
            owner_poisoned,
            identity,
        }
    }
}

pub(crate) struct NativeClaim<'a, K: NativeResource> {
    pub(crate) candidate: NativeCreation<'a, K, Claimed>,
    pub(crate) available: AvailableIdentity<K::Key>,
}

pub(crate) struct UnclaimedNative<'a, K: NativeResource> {
    raw: K::Raw,
    compensation: K::Compensation,
    target_world: ffi::b3WorldId,
    owner_poisoned: &'a Cell<bool>,
    unresolved: bool,
    marker: PhantomData<fn() -> K>,
}

impl<'a, K: NativeResource> UnclaimedNative<'a, K> {
    pub(crate) fn new(
        raw: K::Raw,
        compensation: K::Compensation,
        target_world: ffi::b3WorldId,
        owner_poisoned: &'a Cell<bool>,
    ) -> Self {
        Self {
            raw,
            compensation,
            target_world,
            owner_poisoned,
            unresolved: true,
            marker: PhantomData,
        }
    }

    pub(crate) fn claim(
        mut self,
        identity: IdentityClassification<K::Key>,
    ) -> Result<NativeClaim<'a, K>> {
        if K::is_empty(self.raw) {
            self.unresolved = false;
            return Err(Error::ObjectIdentityExhausted);
        }

        if !unsafe { K::is_valid(self.raw) } {
            self.unresolved = false;
            return Err(Error::ObjectIdentityExhausted);
        }

        let Some(target_slot) = self.target_world.index1.checked_sub(1) else {
            self.owner_poisoned.set(true);
            self.unresolved = false;
            return Err(Error::OwnerPoisoned);
        };
        if K::world_slot(self.raw) != target_slot {
            self.owner_poisoned.set(true);
            self.unresolved = false;
            return Err(Error::OwnerPoisoned);
        }

        if !same_world(unsafe { K::world(self.raw) }, self.target_world) {
            self.owner_poisoned.set(true);
            self.unresolved = false;
            return Err(Error::OwnerPoisoned);
        }

        let available = match identity {
            IdentityClassification::Available(available) => available,
            IdentityClassification::Active => {
                self.owner_poisoned.set(true);
                self.unresolved = false;
                return Err(Error::OwnerPoisoned);
            }
            IdentityClassification::ObservableRetired => {
                let candidate = self.into_compensable();
                drop(candidate);
                return Err(Error::ObjectIdentityExhausted);
            }
        };
        let candidate = self.into_compensable();
        Ok(NativeClaim {
            candidate,
            available,
        })
    }

    fn into_compensable(mut self) -> NativeCreation<'a, K, Claimed> {
        self.unresolved = false;
        NativeCreation {
            raw: self.raw,
            compensation: self.compensation,
            owner_poisoned: self.owner_poisoned,
            armed: true,
            publishing: false,
            marker: PhantomData,
        }
    }
}

impl<K: NativeResource> Drop for UnclaimedNative<'_, K> {
    fn drop(&mut self) {
        if self.unresolved {
            self.owner_poisoned.set(true);
        }
    }
}

pub(crate) struct NativeCreation<'a, K: NativeResource, State> {
    raw: K::Raw,
    compensation: K::Compensation,
    owner_poisoned: &'a Cell<bool>,
    armed: bool,
    publishing: bool,
    marker: PhantomData<fn() -> (K, State)>,
}

impl<'a, K: NativeResource> NativeCreation<'a, K, Claimed> {
    pub(crate) fn bind(mut self) -> NativeCreation<'a, K, Bound> {
        self.armed = false;
        NativeCreation {
            raw: self.raw,
            compensation: self.compensation,
            owner_poisoned: self.owner_poisoned,
            armed: true,
            publishing: false,
            marker: PhantomData,
        }
    }
}

impl<K: NativeResource> NativeCreation<'_, K, Bound> {
    pub(crate) fn commit<R>(mut self, publish: impl FnOnce(K::Raw) -> R) -> Result<R> {
        self.publishing = true;
        let output = publish(self.raw);
        inject_creation_failure(CreationStage::BeforeCommitDisarm)?;
        self.armed = false;
        self.publishing = false;
        Ok(output)
    }
}

impl<K: NativeResource, State> Drop for NativeCreation<'_, K, State> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if self.publishing {
            self.owner_poisoned.set(true);
        }
        unsafe { K::destroy(self.raw, self.compensation) };
        if force_compensation_mismatch()
            || unsafe { K::is_valid(self.raw) }
            || matches!(
                K::COMPENSATION_PROOF,
                CompensationProof::CannotVerifyFullRestoration
            )
        {
            self.owner_poisoned.set(true);
        }
    }
}

pub(crate) fn finish_native_creation<K, Context, BoundState, Output>(
    input: NativeCreationInput<'_, K>,
    context: &mut Context,
    bind: impl FnOnce(&mut Context, K::Raw, AvailableIdentity<K::Key>) -> Result<BoundState>,
    postflight: impl FnOnce(&mut Context, K::Raw) -> Result<()>,
    publish: impl FnOnce(&mut Context, K::Raw, BoundState) -> Output,
) -> Result<Output>
where
    K: NativeResource,
{
    let NativeCreationInput {
        raw,
        compensation,
        target_world,
        owner_poisoned,
        identity,
    } = input;
    let NativeClaim {
        candidate,
        available,
    } = UnclaimedNative::<K>::new(raw, compensation, target_world, owner_poisoned)
        .claim(identity)?;
    inject_creation_failure(CreationStage::AfterClaim)?;
    let bound = bind(context, raw, available)?;
    let candidate = candidate.bind();
    inject_creation_failure(CreationStage::AfterBind)?;
    postflight(context, raw)?;
    inject_creation_failure(CreationStage::AfterPostflight)?;
    inject_creation_failure(CreationStage::BeforePublish)?;
    candidate.commit(|raw| publish(context, raw, bound))
}

fn same_world(left: ffi::b3WorldId, right: ffi::b3WorldId) -> bool {
    left.index1 == right.index1 && left.generation == right.generation
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CreationStage {
    AfterClaim,
    AfterBind,
    AfterPostflight,
    BeforePublish,
    BeforeCommitDisarm,
}

#[inline]
pub(crate) fn inject_creation_failure(_stage: CreationStage) -> Result<()> {
    #[cfg(test)]
    if CREATION_FAULT.with(|fault| fault.get() == Some(_stage)) {
        CREATION_FAULT.with(|fault| fault.set(None));
        return Err(Error::NativeFailure);
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static CREATION_FAULT: Cell<Option<CreationStage>> = const { Cell::new(None) };
    static COMPENSATION_MISMATCH: Cell<bool> = const { Cell::new(false) };
    static SHAPE_PARENT_OVERRIDE: Cell<Option<ffi::b3BodyId>> = const { Cell::new(None) };
    static JOINT_ENDPOINTS_OVERRIDE: Cell<Option<(ffi::b3BodyId, ffi::b3BodyId)>> =
        const { Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_creation_failure(stage: CreationStage) {
    CREATION_FAULT.with(|fault| fault.set(Some(stage)));
}

#[cfg(test)]
pub(crate) fn force_next_compensation_mismatch() {
    COMPENSATION_MISMATCH.with(|fault| fault.set(true));
}

#[cfg(test)]
pub(crate) fn force_next_shape_parent(body: ffi::b3BodyId) {
    SHAPE_PARENT_OVERRIDE.with(|value| value.set(Some(body)));
}

#[cfg(test)]
pub(crate) fn force_next_joint_endpoints(body_a: ffi::b3BodyId, body_b: ffi::b3BodyId) {
    JOINT_ENDPOINTS_OVERRIDE.with(|value| value.set(Some((body_a, body_b))));
}

pub(crate) fn observed_shape_parent(raw: ffi::b3ShapeId) -> ffi::b3BodyId {
    #[cfg(test)]
    if let Some(body) = SHAPE_PARENT_OVERRIDE.with(Cell::take) {
        return body;
    }
    unsafe { ffi::b3Shape_GetBody(raw) }
}

pub(crate) fn observed_joint_endpoints(raw: ffi::b3JointId) -> (ffi::b3BodyId, ffi::b3BodyId) {
    #[cfg(test)]
    if let Some(endpoints) = JOINT_ENDPOINTS_OVERRIDE.with(Cell::take) {
        return endpoints;
    }
    unsafe { (ffi::b3Joint_GetBodyA(raw), ffi::b3Joint_GetBodyB(raw)) }
}

pub(crate) fn force_compensation_mismatch() -> bool {
    #[cfg(test)]
    {
        COMPENSATION_MISMATCH.with(|fault| fault.replace(false))
    }
    #[cfg(not(test))]
    {
        false
    }
}
