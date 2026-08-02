use super::*;
use crate::core::foundation::{
    BoundWorldSlotClaim, WorldSlotClaim, WorldSlotClassification, WorldSlotMutationGuard,
};
use crate::core::provenance::allocate_owner_token;
use crate::core::wasm;
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::debug_draw::{create_debug_shape, destroy_debug_shape};
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use crate::debug_draw::{
    register_provider_debug_registry, take_provider_debug_error, unregister_provider_debug_registry,
};

impl WorldDef {
    fn create_native(&self, owner: &WorldOwner) -> ffi::b3WorldId {
        let mut raw = unsafe { ffi::b3DefaultWorldDef() };
        raw.gravity = self.gravity.into_raw();
        raw.restitutionThreshold = self.restitution_threshold;
        raw.hitEventThreshold = self.hit_event_threshold;
        raw.contactHertz = self.contact_hertz;
        raw.contactDampingRatio = self.contact_damping_ratio;
        raw.contactSpeed = self.contact_speed;
        raw.maximumLinearSpeed = self.maximum_linear_speed;
        raw.enableSleep = self.enable_sleep;
        raw.enableContinuous = self.enable_continuous;
        raw.workerCount = self.worker_count;
        raw.capacity = self.capacity.into_raw();
        raw.frictionCallback = None;
        raw.restitutionCallback = None;
        raw.enqueueTask = None;
        raw.finishTask = None;
        raw.userTaskContext = std::ptr::null_mut();
        raw.userData = std::ptr::null_mut();
        raw.createDebugShape = None;
        raw.destroyDebugShape = None;
        raw.userDebugShapeContext = std::ptr::null_mut();
        if let Some(task_context) = owner.state._task_context.as_deref() {
            task_system::install_callbacks(&mut raw, task_context);
        }
        if !wasm::is_provider_mode() {
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                raw.createDebugShape = Some(create_debug_shape);
                raw.destroyDebugShape = Some(destroy_debug_shape);
                raw.userDebugShapeContext = (&*owner.state.debug_shapes)
                    as *const DebugShapeRegistry
                    as *mut std::ffi::c_void;
            }
        }
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        if owner.state.provider_debug_shapes_token != 0 {
            unsafe {
                ffi::boxddd_provider_debug_install_world_def(
                    &mut raw,
                    owner.state.provider_debug_shapes_token,
                )
            };
        }
        unsafe { create_world_raw(&raw) }
    }
}

struct WorldBootstrapGuard<'a> {
    owner: Option<Box<WorldOwner>>,
    foundation: &'static Foundation,
    slots: &'a mut WorldSlotMutationGuard,
    baseline_count: i32,
    callbacks_installed: bool,
    armed: bool,
    publishing: bool,
    slot_published: bool,
}

impl<'a> WorldBootstrapGuard<'a> {
    fn new(
        owner: Box<WorldOwner>,
        foundation: &'static Foundation,
        slots: &'a mut WorldSlotMutationGuard,
        baseline_count: i32,
    ) -> Self {
        Self {
            owner: Some(owner),
            foundation,
            slots,
            baseline_count,
            callbacks_installed: false,
            armed: true,
            publishing: false,
            slot_published: false,
        }
    }

    fn raw(&self) -> ffi::b3WorldId {
        self.owner.as_ref().expect("bootstrap owner is present").raw
    }

    fn install_callbacks(&mut self) {
        let owner = self.owner.as_ref().expect("bootstrap owner is present");
        owner.state.callbacks.install_raw_callbacks(owner.raw);
        self.callbacks_installed = true;
    }

    fn verify_postflight(&self) -> Result<()> {
        if !unsafe { ffi::b3World_IsValid(self.raw()) }
            || unsafe { ffi::b3GetWorldCount() } != self.baseline_count + 1
        {
            return Err(Error::NativeFailure);
        }
        Ok(())
    }
}

struct ClaimedWorldBootstrap<'a> {
    guard: WorldBootstrapGuard<'a>,
    claim: WorldSlotClaim,
}

impl<'a> ClaimedWorldBootstrap<'a> {
    fn new(
        owner: Box<WorldOwner>,
        foundation: &'static Foundation,
        slots: &'a mut WorldSlotMutationGuard,
        claim: WorldSlotClaim,
        baseline_count: i32,
    ) -> Self {
        Self {
            guard: WorldBootstrapGuard::new(owner, foundation, slots, baseline_count),
            claim,
        }
    }

    fn bind(self) -> Result<BoundWorldBootstrap<'a>> {
        let guard = self.guard;
        let raw = guard.raw();
        let claim = guard
            .slots
            .bind(self.claim, raw)
            .ok_or(Error::NativeFailure)?;
        Ok(BoundWorldBootstrap { guard, claim })
    }
}

struct BoundWorldBootstrap<'a> {
    guard: WorldBootstrapGuard<'a>,
    claim: BoundWorldSlotClaim,
}

impl BoundWorldBootstrap<'_> {
    fn install_callbacks(&mut self) {
        self.guard.install_callbacks();
    }

    fn verify_postflight(&self) -> Result<()> {
        self.guard.verify_postflight()
    }

    fn commit(mut self) -> Result<World> {
        self.guard.publishing = true;
        self.guard.slots.publish(self.claim);
        self.guard.slot_published = true;
        creation_transaction::inject_creation_failure(
            creation_transaction::CreationStage::BeforeCommitDisarm,
        )?;
        self.guard.armed = false;
        self.guard.publishing = false;
        Ok(World {
            owner: self.guard.owner.take(),
        })
    }
}

impl Drop for WorldBootstrapGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if self.publishing && !self.slot_published {
            self.foundation.poison();
        }
        let Some(mut owner) = self.owner.take() else {
            self.foundation.poison();
            return;
        };
        let raw = owner.raw;
        if !unsafe { ffi::b3World_IsValid(raw) } {
            self.foundation.poison();
            std::mem::forget(owner);
            return;
        }
        if self.callbacks_installed {
            let retired = owner.state.callbacks.clear_raw_callbacks(raw);
            let _ = retired.release_contained();
        }
        unsafe { ffi::b3DestroyWorld(raw) };
        let slot_restored = !self.slot_published || self.slots.retire(raw);
        let compensated = !unsafe { ffi::b3World_IsValid(raw) }
            && unsafe { ffi::b3GetWorldCount() } == self.baseline_count
            && slot_restored
            && !creation_transaction::force_compensation_mismatch();
        if !compensated {
            self.foundation.poison();
            std::mem::forget(owner);
            return;
        }
        cleanup_unpublished_world(&mut owner);
    }
}

impl World {
    pub(crate) fn create(foundation: &'static Foundation, def: WorldDef) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        def.validate()?;
        def.validate_platform()?;
        let foundation_lease = foundation.acquire_ordinary()?;
        let owner_token = allocate_owner_token()?;

        let ledger = WorldLedger::new(owner_token);
        let callback_index = ledger.callback_index();
        let callbacks = WorldCallbacks::new(callback_index.clone());
        let debug_shapes = Box::new(DebugShapeRegistry::new(
            callback_index,
            callbacks.invocation_slot(),
        ));
        let task_system = def.task_system.clone();
        let task_context = task_system.as_ref().map(|task_system| {
            task_system::InstalledTaskContext::new(task_system, callbacks.invocation_slot())
        });
        let mut slot_guard = crate::core::foundation::world_slot_mutation_lock();
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        let provider_debug_shapes_token = if wasm::is_provider_mode() {
            register_provider_debug_registry(&debug_shapes).ok_or(Error::ProviderCallbackFailed)?
        } else {
            0
        };

        let mut owner = Box::new(WorldOwner {
            raw: ffi::b3WorldId {
                index1: 0,
                generation: 0,
            },
            state: WorldState {
                phase: WorldPhase::Live,
                poisoned: Cell::new(false),
                ledger,
                backing_quarantine: Vec::new(),
                callbacks,
                event_scratch: EventScratch::default(),
                _task_context: task_context,
                active_recording: None,
                debug_shapes,
                #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
                provider_debug_shapes_token,
            },
            _foundation_lease: foundation_lease,
            _not_send_sync: PhantomData,
        });

        let baseline_count = unsafe { ffi::b3GetWorldCount() };
        if !(0..=ffi::B3_MAX_WORLDS as i32).contains(&baseline_count) {
            foundation.poison();
            cleanup_unpublished_world(&mut owner);
            return Err(Error::FoundationPoisoned);
        }
        if baseline_count == ffi::B3_MAX_WORLDS as i32 {
            cleanup_unpublished_world(&mut owner);
            return Err(Error::ObjectIdentityExhausted);
        }

        let raw = def.create_native(&owner);
        owner.raw = raw;
        let observed_count = unsafe { ffi::b3GetWorldCount() };
        let claim = match slot_guard.classify(raw) {
            WorldSlotClassification::Empty | WorldSlotClassification::Invalid => {
                if observed_count != baseline_count {
                    foundation.poison();
                    std::mem::forget(owner);
                    return Err(Error::FoundationPoisoned);
                }
                cleanup_unpublished_world(&mut owner);
                return Err(Error::ObjectIdentityExhausted);
            }
            WorldSlotClassification::Active => {
                foundation.poison();
                std::mem::forget(owner);
                return Err(Error::FoundationPoisoned);
            }
            WorldSlotClassification::Available(claim) => claim,
        };
        if !unsafe { ffi::b3World_IsValid(raw) } {
            if observed_count != baseline_count {
                foundation.poison();
                std::mem::forget(owner);
                return Err(Error::FoundationPoisoned);
            }
            cleanup_unpublished_world(&mut owner);
            return Err(Error::ObjectIdentityExhausted);
        }
        if observed_count != baseline_count + 1 {
            foundation.poison();
            std::mem::forget(owner);
            return Err(Error::FoundationPoisoned);
        }

        let bootstrap =
            ClaimedWorldBootstrap::new(owner, foundation, &mut slot_guard, claim, baseline_count);
        creation_transaction::inject_creation_failure(
            creation_transaction::CreationStage::AfterClaim,
        )?;
        let mut bootstrap = bootstrap.bind()?;
        creation_transaction::inject_creation_failure(
            creation_transaction::CreationStage::AfterBind,
        )?;
        bootstrap.verify_postflight()?;
        bootstrap.install_callbacks();
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        if let Some(error) = take_provider_debug_error(
            bootstrap
                .guard
                .owner
                .as_ref()
                .expect("bootstrap owner is present")
                .state
                .provider_debug_shapes_token,
        ) {
            return Err(error);
        }
        creation_transaction::inject_creation_failure(
            creation_transaction::CreationStage::AfterPostflight,
        )?;
        creation_transaction::inject_creation_failure(
            creation_transaction::CreationStage::BeforePublish,
        )?;
        bootstrap.commit()
    }
}

#[cfg(not(feature = "double-precision"))]
#[inline]
unsafe fn create_world_raw(def: *const ffi::b3WorldDef) -> ffi::b3WorldId {
    unsafe { ffi::b3CreateWorld(def) }
}

#[cfg(feature = "double-precision")]
#[inline]
unsafe fn create_world_raw(def: *const ffi::b3WorldDef) -> ffi::b3WorldId {
    unsafe { ffi::b3CreateWorldDoublePrecision(def) }
}

fn native_world_count_allows_teardown(count: i32) -> bool {
    (1..=ffi::B3_MAX_WORLDS as i32).contains(&count)
}

impl Drop for World {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let cleanup = move || shutdown_world(owner);
        if callback_state::in_callback() {
            callback_state::defer_local_cleanup_or_retain(cleanup);
        } else {
            cleanup();
        }
    }
}

fn cleanup_unpublished_world(owner: &mut WorldOwner) {
    owner.state.debug_shapes.clear_all();
    owner.state.ledger.finish_drop();
    owner.state.backing_quarantine.clear();
    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
    if owner.state.provider_debug_shapes_token != 0 {
        let _ = take_provider_debug_error(owner.state.provider_debug_shapes_token);
        unregister_provider_debug_registry(owner.state.provider_debug_shapes_token);
        owner.state.provider_debug_shapes_token = 0;
    }
    owner.state.phase = WorldPhase::Destroyed;
}

fn shutdown_world(owner: Box<WorldOwner>) {
    let mut owner = callback_state::RetainOnUnwind::new(owner);
    let raw = owner.raw;
    let foundation = owner._foundation_lease.foundation();
    let owner_call_frame = callback_state::OwnerCallFrame::enter();
    if owner.state.phase != WorldPhase::Live {
        drop(owner_call_frame);
        owner.finish();
        return;
    }
    owner.state.phase = WorldPhase::Dropping;
    let debug_failure_generation = owner.state.debug_shapes.failure_generation();
    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
    if owner.state.provider_debug_shapes_token != 0 {
        let _ = take_provider_debug_error(owner.state.provider_debug_shapes_token);
    }
    let (retired_callbacks, destroyed) = {
        let mut slot_guard = crate::core::foundation::world_slot_mutation_lock();
        if !slot_guard.contains(raw) {
            foundation.poison();
            drop(owner_call_frame);
            return;
        }
        let baseline_count = unsafe { ffi::b3GetWorldCount() };
        if !native_world_count_allows_teardown(baseline_count) {
            foundation.poison();
            drop(owner_call_frame);
            return;
        }
        crate::recording::stop_recording_owner(raw, &mut owner.state);
        if unsafe { ffi::b3World_IsValid(raw) } {
            let retired_callbacks = owner.state.callbacks.clear_raw_callbacks(raw);
            unsafe { ffi::b3DestroyWorld(raw) };
            let destroyed = !unsafe { ffi::b3World_IsValid(raw) }
                && unsafe { ffi::b3GetWorldCount() } == baseline_count - 1
                && slot_guard.retire(raw);
            (Some(retired_callbacks), destroyed)
        } else {
            let _ = slot_guard.retire(raw);
            (None, false)
        }
    };
    if !destroyed {
        foundation.poison();
        drop(owner_call_frame);
        return;
    }
    owner.state.debug_shapes.clear_all();
    owner.state.ledger.finish_drop();
    owner.state.backing_quarantine.clear();
    let mut cleanup_failed =
        owner.state.debug_shapes.failure_generation() != debug_failure_generation;
    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
    {
        cleanup_failed |=
            take_provider_debug_error(owner.state.provider_debug_shapes_token).is_some();
    }
    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
    if owner.state.provider_debug_shapes_token != 0 {
        unregister_provider_debug_registry(owner.state.provider_debug_shapes_token);
        owner.state.provider_debug_shapes_token = 0;
    }
    owner.state.phase = WorldPhase::Destroyed;
    cleanup_failed |= retired_callbacks
        .expect("valid World destruction retired callbacks")
        .release_contained()
        .is_err();
    if cleanup_failed {
        foundation.poison();
    }
    drop(owner_call_frame);
    owner.finish();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
    use crate::debug_draw::provider_debug_registry_count;
    use crate::world::creation_transaction::{
        CreationStage, force_creation_failure, force_next_compensation_mismatch,
    };
    use std::process::Command;

    const WORLD_BOOTSTRAP_MISMATCH_ENV: &str = "BOXDDD_WORLD_BOOTSTRAP_MISMATCH";
    const WORLD_CAPACITY_ENV: &str = "BOXDDD_WORLD_CAPACITY";

    fn foundation() -> &'static Foundation {
        Foundation::initialize_default().unwrap()
    }

    #[test]
    fn native_world_creation_is_a_complete_operation() {
        let _: fn(&WorldDef, &WorldOwner) -> ffi::b3WorldId = WorldDef::create_native;
    }

    #[test]
    fn world_teardown_rejects_invalid_native_count_ranges() {
        let maximum = ffi::B3_MAX_WORLDS as i32;
        assert!(native_world_count_allows_teardown(1));
        assert!(native_world_count_allows_teardown(maximum));
        for invalid in [i32::MIN, -1, 0, maximum + 1, i32::MAX] {
            assert!(!native_world_count_allows_teardown(invalid));
        }
    }

    #[test]
    fn creation_transaction_world_bootstrap_compensates_every_fallible_stage() {
        for stage in [
            CreationStage::AfterClaim,
            CreationStage::AfterBind,
            CreationStage::AfterPostflight,
            CreationStage::BeforePublish,
            CreationStage::BeforeCommitDisarm,
        ] {
            let foundation = foundation();
            let baseline = unsafe { ffi::b3GetWorldCount() };
            let activity_baseline = foundation.activity();
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            let provider_registry_baseline = provider_debug_registry_count();
            force_creation_failure(stage);
            assert_eq!(
                foundation.create_world(foundation.world_def()).unwrap_err(),
                Error::NativeFailure
            );
            assert_eq!(unsafe { ffi::b3GetWorldCount() }, baseline);
            assert_eq!(foundation.activity(), activity_baseline);
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            assert_eq!(provider_debug_registry_count(), provider_registry_baseline);
            let world = foundation.create_world(foundation.world_def()).unwrap();
            assert_eq!(unsafe { ffi::b3GetWorldCount() }, baseline + 1);
            drop(world);
            assert_eq!(unsafe { ffi::b3GetWorldCount() }, baseline);
            assert_eq!(foundation.activity(), activity_baseline);
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            assert_eq!(provider_debug_registry_count(), provider_registry_baseline);
        }
    }

    #[test]
    fn creation_transaction_world_capacity_is_rejected_before_native_create() {
        if std::env::var_os(WORLD_CAPACITY_ENV).is_some() {
            let foundation = Foundation::initialize_default().unwrap();
            let baseline = unsafe { ffi::b3GetWorldCount() };
            let capacity = ffi::B3_MAX_WORLDS as i32;
            assert!((0..=capacity).contains(&baseline));
            let mut worlds = Vec::new();
            worlds.try_reserve((capacity - baseline) as usize).unwrap();
            for _ in baseline..capacity {
                worlds.push(foundation.create_world(foundation.world_def()).unwrap());
            }
            assert_eq!(unsafe { ffi::b3GetWorldCount() }, capacity);
            assert_eq!(
                foundation.create_world(foundation.world_def()).unwrap_err(),
                Error::ObjectIdentityExhausted
            );
            assert!(Foundation::get().is_ok());
            drop(worlds);
            assert_eq!(unsafe { ffi::b3GetWorldCount() }, baseline);
            return;
        }

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "world::lifecycle::tests::creation_transaction_world_capacity_is_rejected_before_native_create",
                "--nocapture",
            ])
            .env(WORLD_CAPACITY_ENV, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn creation_transaction_world_bootstrap_mismatch_poisons_foundation() {
        if std::env::var_os(WORLD_BOOTSTRAP_MISMATCH_ENV).is_some() {
            let foundation = Foundation::initialize_default().unwrap();
            let baseline = unsafe { ffi::b3GetWorldCount() };
            force_creation_failure(CreationStage::AfterClaim);
            force_next_compensation_mismatch();
            assert_eq!(
                foundation.create_world(foundation.world_def()).unwrap_err(),
                Error::NativeFailure
            );
            assert_eq!(unsafe { ffi::b3GetWorldCount() }, baseline);
            assert_eq!(Foundation::get().unwrap_err(), Error::FoundationPoisoned);
            assert_eq!(
                foundation.create_world(foundation.world_def()).unwrap_err(),
                Error::FoundationPoisoned
            );
            return;
        }

        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "world::lifecycle::tests::creation_transaction_world_bootstrap_mismatch_poisons_foundation",
                "--nocapture",
            ])
            .env(WORLD_BOOTSTRAP_MISMATCH_ENV, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
