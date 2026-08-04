//! Explicit process-lifetime initialization for Box3D.
//!
//! Call [`Foundation::initialize`] before the first safe operation that can enter Box3D. The
//! normalized configuration is written to the native process globals exactly once and remains
//! fixed until process exit. Repeating initialization with the same normalized bits is
//! idempotent; a different configuration is rejected without changing native state.
//!
//! A `&'static Foundation` is a configuration handle, not an activity lease. Native owners and
//! worldless calls acquire the appropriate internal activity lease when they enter Box3D.

use crate::body::{BodyDef, BodyDefBuilder};
use crate::core::{callback_state, validation};
use crate::error::{Error, InvalidValueReason, Result};
use crate::shapes::{ShapeDef, ShapeDefBuilder};
use crate::world::{World, WorldDef, WorldDefBuilder};
use boxddd_sys::ffi;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Process-wide Box3D stall-reporting policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StallThreshold {
    /// Disable stall reporting by using Box3D's maximum finite threshold.
    Disabled,
    /// Report native operations that exceed this positive number of seconds.
    Seconds(f32),
}

impl StallThreshold {
    fn normalized(self) -> Result<Self> {
        match self {
            Self::Disabled => Ok(Self::Disabled),
            Self::Seconds(seconds) => {
                validation::positive("foundation.stall_threshold", seconds)?;
                if seconds.to_bits() == f32::MAX.to_bits() {
                    Ok(Self::Disabled)
                } else {
                    Ok(Self::Seconds(seconds))
                }
            }
        }
    }

    fn native_value(self) -> f32 {
        match self {
            Self::Disabled => f32::MAX,
            Self::Seconds(seconds) => seconds,
        }
    }
}

/// Immutable process-wide Box3D settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoundationConfig {
    /// Number of application length units represented by one meter.
    pub length_units_per_meter: f32,
    /// Native operation stall-reporting policy.
    pub stall_threshold: StallThreshold,
}

impl Default for FoundationConfig {
    fn default() -> Self {
        Self {
            length_units_per_meter: 1.0,
            stall_threshold: StallThreshold::Disabled,
        }
    }
}

impl FoundationConfig {
    fn normalized(self) -> Result<Self> {
        validate_length_units(
            "foundation.length_units_per_meter",
            self.length_units_per_meter,
        )?;
        let stall_threshold = self.stall_threshold.normalized()?;
        Ok(Self {
            length_units_per_meter: self.length_units_per_meter,
            stall_threshold,
        })
    }

    fn has_same_bits(self, other: Self) -> bool {
        self.length_units_per_meter.to_bits() == other.length_units_per_meter.to_bits()
            && self.stall_threshold.native_value().to_bits()
                == other.stall_threshold.native_value().to_bits()
    }
}

pub(crate) fn validate_length_units(context: &'static str, length_units: f32) -> Result<()> {
    validation::positive(context, length_units)?;
    let squared = length_units * length_units;
    let cubed = squared * length_units;
    #[cfg(feature = "double-precision")]
    let huge = 1.0e9 * length_units;
    #[cfg(not(feature = "double-precision"))]
    let huge = 1.0e5 * length_units;
    let values = [
        0.05 * length_units,
        3.0 * length_units,
        400.0 * length_units,
        squared,
        cubed,
        1000.0 / cubed,
        huge,
    ];
    if values.iter().all(|value| value.is_finite() && *value > 0.0) {
        Ok(())
    } else {
        Err(Error::InvalidValue {
            context,
            reason: InvalidValueReason::OutOfRange,
        })
    }
}

/// Point-in-time process activity coordinated by the Foundation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FoundationActivity {
    /// Number of long-lived ordinary native owners.
    pub ordinary_owners: u32,
    /// Number of worldless native calls currently in flight.
    pub transient_calls: u32,
    /// Whether one replay player owns exclusive process activity.
    pub replay_active: bool,
}

const COUNT_BITS: u32 = 31;
const ORDINARY_ONE: u64 = 1;
const ORDINARY_MASK: u64 = (1_u64 << COUNT_BITS) - 1;
const TRANSIENT_SHIFT: u32 = COUNT_BITS;
const TRANSIENT_ONE: u64 = 1_u64 << TRANSIENT_SHIFT;
const TRANSIENT_MASK: u64 = ORDINARY_MASK << TRANSIENT_SHIFT;
const REPLAY_BIT: u64 = 1_u64 << (COUNT_BITS * 2);
const POISON_BIT: u64 = 1_u64 << 63;

#[derive(Debug)]
struct ActivityState {
    packed: AtomicU64,
}

impl ActivityState {
    const fn new() -> Self {
        Self {
            packed: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> FoundationActivity {
        Self::unpack(self.packed.load(Ordering::Acquire))
    }

    fn unpack(packed: u64) -> FoundationActivity {
        FoundationActivity {
            ordinary_owners: (packed & ORDINARY_MASK) as u32,
            transient_calls: ((packed & TRANSIENT_MASK) >> TRANSIENT_SHIFT) as u32,
            replay_active: packed & REPLAY_BIT != 0,
        }
    }

    fn acquire_shared(&self, increment: u64, mask: u64) -> Result<()> {
        let mut current = self.packed.load(Ordering::Acquire);
        loop {
            if current & POISON_BIT != 0 {
                return Err(Error::FoundationPoisoned);
            }
            if current & REPLAY_BIT != 0 {
                return Err(Error::FoundationBusy);
            }
            if current & mask == mask {
                return Err(Error::FoundationActivityExhausted);
            }
            match self.packed.compare_exchange_weak(
                current,
                current + increment,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(observed) => current = observed,
            }
        }
    }

    fn acquire_replay(&self) -> Result<()> {
        match self
            .packed
            .compare_exchange(0, REPLAY_BIT, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(current) if current & POISON_BIT != 0 => Err(Error::FoundationPoisoned),
            Err(_) => Err(Error::FoundationBusy),
        }
    }

    fn release(&self, decrement: u64, mask: u64) {
        let previous = self.packed.fetch_sub(decrement, Ordering::AcqRel);
        debug_assert_eq!(previous & REPLAY_BIT, 0);
        debug_assert_ne!(previous & mask, 0);
    }

    fn release_replay(&self) {
        let previous = self.packed.fetch_and(!REPLAY_BIT, Ordering::AcqRel);
        debug_assert_ne!(previous & REPLAY_BIT, 0);
        debug_assert_eq!(previous & !(REPLAY_BIT | POISON_BIT), 0);
    }

    fn poison(&self) {
        self.packed.fetch_or(POISON_BIT, Ordering::AcqRel);
    }

    fn is_poisoned(&self) -> bool {
        self.packed.load(Ordering::Acquire) & POISON_BIT != 0
    }
}

/// Frozen process-wide Box3D configuration and activity coordinator.
pub struct Foundation {
    config: FoundationConfig,
    activity: ActivityState,
}

impl fmt::Debug for Foundation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Foundation")
            .field("config", &self.config)
            .field("activity", &self.activity())
            .field("poisoned", &FOUNDATION_POISONED.load(Ordering::Acquire))
            .finish()
    }
}

impl Foundation {
    /// Initializes and freezes process-wide Box3D settings until process exit.
    ///
    /// This must be the first safe operation that can enter Box3D. Initialization validates the
    /// complete configuration before writing native state, then reads the native values back
    /// before publishing the Foundation. The same normalized bit pattern may be initialized
    /// repeatedly and returns the existing process handle. A different pattern returns
    /// [`Error::FoundationConflict`]. Any failure after the first native write permanently returns
    /// [`Error::FoundationPoisoned`] from later safe admission because the native state can no
    /// longer be proven clean.
    pub fn initialize(config: FoundationConfig) -> Result<&'static Self> {
        callback_state::check_not_in_callback()?;
        let config = config.normalized()?;
        let _init_guard = FOUNDATION_INIT_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if FOUNDATION_POISONED.load(Ordering::Acquire) {
            return Err(Error::FoundationPoisoned);
        }
        if let Some(foundation) = FOUNDATION.get() {
            return if foundation.config.has_same_bits(config) {
                Ok(foundation)
            } else {
                Err(Error::FoundationConflict)
            };
        }

        let mut write_guard = InitializationWriteGuard::new();
        unsafe { ffi::b3SetLengthUnitsPerMeter(config.length_units_per_meter) };
        write_guard.native_write_started = true;
        inject_initialization_fault("after_length_setter");
        unsafe { ffi::b3SetStallThreshold(config.stall_threshold.native_value()) };
        inject_initialization_fault("after_stall_setter");

        // Windows and Apple lazily initialize Box3D's timer conversion factor in a writable
        // native global. Prime it while initialization is serialized so later Worlds may step
        // concurrently without racing that first write.
        let timer_start = unsafe { ffi::b3GetTicks() };
        let _elapsed = unsafe { ffi::b3GetMilliseconds(timer_start) };

        inject_initialization_fault("before_readback");
        let observed_length = unsafe { ffi::b3GetLengthUnitsPerMeter() };
        let observed_stall = unsafe { ffi::b3GetStallThreshold() };
        #[cfg(test)]
        let observed_stall = if std::env::var_os(FOUNDATION_FAULT_ENV).as_deref()
            == Some(std::ffi::OsStr::new("readback_mismatch"))
        {
            0.5 * config.stall_threshold.native_value()
        } else {
            observed_stall
        };
        inject_initialization_fault("after_readback");
        if observed_length.to_bits() != config.length_units_per_meter.to_bits()
            || observed_stall.to_bits() != config.stall_threshold.native_value().to_bits()
        {
            return Err(Error::FoundationPoisoned);
        }

        FOUNDATION
            .set(Self {
                config,
                activity: ActivityState::new(),
            })
            .map_err(|_| Error::FoundationPoisoned)?;
        write_guard.committed = true;
        Ok(FOUNDATION
            .get()
            .expect("Foundation was published while holding the initialization lock"))
    }

    /// Initializes the process-lifetime Foundation with one unit per meter and no stall reports.
    ///
    /// Like [`Foundation::initialize`], this is explicit first-use initialization rather than a
    /// lazy default. It conflicts if the process was already initialized with different settings.
    pub fn initialize_default() -> Result<&'static Self> {
        Self::initialize(FoundationConfig::default())
    }

    /// Returns the explicitly initialized process Foundation without initializing it.
    pub fn get() -> Result<&'static Self> {
        if FOUNDATION_POISONED.load(Ordering::Acquire) {
            Err(Error::FoundationPoisoned)
        } else {
            FOUNDATION.get().ok_or(Error::FoundationUninitialized)
        }
    }

    /// Returns the immutable normalized process configuration.
    pub const fn config(&self) -> FoundationConfig {
        self.config
    }

    /// Returns a coherent activity snapshot.
    pub fn activity(&self) -> FoundationActivity {
        self.activity.snapshot()
    }

    /// Creates a scale-aware World definition.
    pub fn world_def(&self) -> WorldDef {
        WorldDef::with_length_units_per_meter(self.config.length_units_per_meter)
    }

    /// Creates a builder initialized from a scale-aware World definition.
    pub fn world_def_builder(&self) -> WorldDefBuilder {
        WorldDefBuilder::from_def(self.world_def())
    }

    /// Creates a scale-aware body definition.
    pub fn body_def(&self) -> BodyDef {
        BodyDef::with_length_units_per_meter(self.config.length_units_per_meter)
    }

    /// Creates a builder initialized from a scale-aware body definition.
    pub fn body_def_builder(&self) -> BodyDefBuilder {
        BodyDefBuilder::from_def(self.body_def())
    }

    /// Creates a scale-aware shape definition.
    pub fn shape_def(&self) -> ShapeDef {
        ShapeDef::with_length_units_per_meter(self.config.length_units_per_meter)
    }

    /// Creates a builder initialized from a scale-aware shape definition.
    pub fn shape_def_builder(&self) -> ShapeDefBuilder {
        ShapeDefBuilder::from_def(self.shape_def())
    }

    /// Creates a World that retains ordinary Foundation activity until native destruction.
    pub fn create_world(&'static self, def: WorldDef) -> Result<World> {
        World::create(self, def)
    }

    pub(crate) fn acquire_ordinary(&'static self) -> Result<OrdinaryLease> {
        callback_state::check_not_in_callback()?;
        self.ensure_healthy()?;
        self.activity.acquire_shared(ORDINARY_ONE, ORDINARY_MASK)?;
        Ok(OrdinaryLease { foundation: self })
    }

    fn acquire_transient(&'static self) -> Result<TransientLease> {
        self.ensure_healthy()?;
        self.activity
            .acquire_shared(TRANSIENT_ONE, TRANSIENT_MASK)?;
        Ok(TransientLease { foundation: self })
    }

    pub(crate) fn enter_transient_call() -> Result<TransientCall> {
        callback_state::check_not_in_callback()?;
        let lease = Self::get()?.acquire_transient()?;
        let owner_call_frame = callback_state::OwnerCallFrame::enter();
        Ok(TransientCall {
            _owner_call_frame: owner_call_frame,
            _lease: lease,
        })
    }

    pub(crate) fn acquire_replay(&'static self) -> Result<ReplayLease> {
        callback_state::check_not_in_callback()?;
        self.ensure_healthy()?;
        self.activity.acquire_replay()?;
        Ok(ReplayLease { foundation: self })
    }

    pub(crate) fn ensure_healthy(&self) -> Result<()> {
        if self.activity.is_poisoned() || FOUNDATION_POISONED.load(Ordering::Acquire) {
            Err(Error::FoundationPoisoned)
        } else {
            Ok(())
        }
    }

    pub(crate) fn poison(&self) {
        self.activity.poison();
        FOUNDATION_POISONED.store(true, Ordering::Release);
    }
}

struct InitializationWriteGuard {
    native_write_started: bool,
    committed: bool,
}

impl InitializationWriteGuard {
    fn new() -> Self {
        Self {
            native_write_started: false,
            committed: false,
        }
    }
}

impl Drop for InitializationWriteGuard {
    fn drop(&mut self) {
        if self.native_write_started && !self.committed {
            FOUNDATION_POISONED.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug)]
pub(crate) struct OrdinaryLease {
    foundation: &'static Foundation,
}

impl OrdinaryLease {
    pub(crate) const fn foundation(&self) -> &'static Foundation {
        self.foundation
    }

    pub(crate) fn enter_call(&self) -> Result<callback_state::OwnerCallFrame> {
        callback_state::check_not_in_callback()?;
        self.foundation.ensure_healthy()?;
        Ok(callback_state::OwnerCallFrame::enter())
    }
}

impl Drop for OrdinaryLease {
    fn drop(&mut self) {
        self.foundation
            .activity
            .release(ORDINARY_ONE, ORDINARY_MASK);
    }
}

#[derive(Debug)]
pub(crate) struct TransientLease {
    foundation: &'static Foundation,
}

pub(crate) struct TransientCall {
    _owner_call_frame: callback_state::OwnerCallFrame,
    _lease: TransientLease,
}

impl Drop for TransientLease {
    fn drop(&mut self) {
        self.foundation
            .activity
            .release(TRANSIENT_ONE, TRANSIENT_MASK);
    }
}

#[derive(Debug)]
pub(crate) struct ReplayLease {
    foundation: &'static Foundation,
}

impl ReplayLease {
    pub(crate) const fn foundation(&self) -> &'static Foundation {
        self.foundation
    }

    pub(crate) fn enter_call(&self) -> Result<callback_state::OwnerCallFrame> {
        callback_state::check_not_in_callback()?;
        self.foundation.ensure_healthy()?;
        Ok(callback_state::OwnerCallFrame::enter())
    }
}

impl Drop for ReplayLease {
    fn drop(&mut self) {
        self.foundation.activity.release_replay();
    }
}

static FOUNDATION: OnceLock<Foundation> = OnceLock::new();
static FOUNDATION_INIT_LOCK: Mutex<()> = Mutex::new(());
static FOUNDATION_POISONED: AtomicBool = AtomicBool::new(false);
const MAX_WORLD_SLOTS: usize = ffi::B3_MAX_WORLDS as usize;

struct WorldSlotRegistry {
    generations: [Option<u16>; MAX_WORLD_SLOTS],
}

impl WorldSlotRegistry {
    const fn new() -> Self {
        Self {
            generations: [None; MAX_WORLD_SLOTS],
        }
    }
}

static WORLD_SLOT_MUTATION_LOCK: Mutex<WorldSlotRegistry> = Mutex::new(WorldSlotRegistry::new());

pub(crate) struct WorldSlotMutationGuard {
    registry: MutexGuard<'static, WorldSlotRegistry>,
}

pub(crate) enum WorldSlotClassification {
    Empty,
    Invalid,
    Active,
    Available(WorldSlotClaim),
}

pub(crate) struct WorldSlotClaim {
    slot: usize,
    generation: u16,
}

pub(crate) struct BoundWorldSlotClaim {
    slot: usize,
    generation: u16,
}

impl WorldSlotMutationGuard {
    pub(crate) fn classify(&self, raw: ffi::b3WorldId) -> WorldSlotClassification {
        if raw.index1 == 0 {
            return WorldSlotClassification::Empty;
        }
        let slot = usize::from(raw.index1 - 1);
        if slot >= MAX_WORLD_SLOTS {
            return WorldSlotClassification::Invalid;
        }
        if self.registry.generations[slot].is_some() {
            WorldSlotClassification::Active
        } else {
            WorldSlotClassification::Available(WorldSlotClaim {
                slot,
                generation: raw.generation,
            })
        }
    }

    pub(crate) fn bind(
        &self,
        claim: WorldSlotClaim,
        raw: ffi::b3WorldId,
    ) -> Option<BoundWorldSlotClaim> {
        let slot = raw.index1.checked_sub(1).map(usize::from)?;
        if claim.slot != slot
            || claim.generation != raw.generation
            || self
                .registry
                .generations
                .get(slot)
                .copied()
                .flatten()
                .is_some()
        {
            return None;
        }
        Some(BoundWorldSlotClaim {
            slot: claim.slot,
            generation: claim.generation,
        })
    }

    pub(crate) fn publish(&mut self, claim: BoundWorldSlotClaim) {
        assert!(
            self.registry.generations[claim.slot].is_none(),
            "claimed World slot changed before publication"
        );
        self.registry.generations[claim.slot] = Some(claim.generation);
    }

    pub(crate) fn retire(&mut self, raw: ffi::b3WorldId) -> bool {
        let Some(slot) = raw.index1.checked_sub(1).map(usize::from) else {
            return false;
        };
        let Some(generation) = self.registry.generations.get_mut(slot) else {
            return false;
        };
        if *generation != Some(raw.generation) {
            return false;
        }
        *generation = None;
        true
    }

    pub(crate) fn contains(&self, raw: ffi::b3WorldId) -> bool {
        let Some(slot) = raw.index1.checked_sub(1).map(usize::from) else {
            return false;
        };
        self.registry.generations.get(slot).copied().flatten() == Some(raw.generation)
    }
}

pub(crate) fn world_slot_mutation_lock() -> WorldSlotMutationGuard {
    WorldSlotMutationGuard {
        registry: WORLD_SLOT_MUTATION_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    }
}

#[cfg(test)]
const FOUNDATION_FAULT_ENV: &str = "BOXDDD_FOUNDATION_INTERNAL_FAULT";

#[inline]
fn inject_initialization_fault(_stage: &'static str) {
    #[cfg(test)]
    if std::env::var_os(FOUNDATION_FAULT_ENV).as_deref() == Some(std::ffi::OsStr::new(_stage)) {
        panic!("injected Foundation initialization fault at {_stage}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn partial_native_initialization_permanently_poisons_the_process() {
        if let Some(stage) = std::env::var_os(FOUNDATION_FAULT_ENV) {
            let stage = stage.to_string_lossy();
            let result = std::panic::catch_unwind(|| {
                Foundation::initialize(FoundationConfig {
                    length_units_per_meter: 2.0,
                    stall_threshold: StallThreshold::Seconds(0.25),
                })
            });
            if stage == "readback_mismatch" {
                assert_eq!(result.unwrap().unwrap_err(), Error::FoundationPoisoned);
            } else {
                assert!(result.is_err());
            }
            assert_eq!(Foundation::get().unwrap_err(), Error::FoundationPoisoned);
            assert_eq!(
                Foundation::initialize_default().unwrap_err(),
                Error::FoundationPoisoned
            );
            return;
        }

        for stage in [
            "after_length_setter",
            "after_stall_setter",
            "before_readback",
            "after_readback",
            "readback_mismatch",
        ] {
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "core::foundation::tests::partial_native_initialization_permanently_poisons_the_process",
                    "--nocapture",
                ])
                .env(FOUNDATION_FAULT_ENV, stage)
                .status()
                .unwrap();
            assert!(status.success(), "fault stage {stage} failed");
        }
    }

    #[test]
    fn activity_state_is_non_blocking_and_exclusive() {
        let activity = ActivityState::new();
        activity
            .acquire_shared(ORDINARY_ONE, ORDINARY_MASK)
            .unwrap();
        activity
            .acquire_shared(TRANSIENT_ONE, TRANSIENT_MASK)
            .unwrap();
        assert_eq!(
            activity.snapshot(),
            FoundationActivity {
                ordinary_owners: 1,
                transient_calls: 1,
                replay_active: false,
            }
        );
        assert_eq!(
            activity.acquire_replay().unwrap_err(),
            Error::FoundationBusy
        );
        activity.release(TRANSIENT_ONE, TRANSIENT_MASK);
        activity.release(ORDINARY_ONE, ORDINARY_MASK);
        activity.acquire_replay().unwrap();
        assert_eq!(
            activity
                .acquire_shared(ORDINARY_ONE, ORDINARY_MASK)
                .unwrap_err(),
            Error::FoundationBusy
        );
        activity.packed.fetch_and(!REPLAY_BIT, Ordering::AcqRel);
        assert_eq!(activity.snapshot(), FoundationActivity::default());
    }

    #[test]
    fn stale_health_observation_cannot_bypass_poison_after_replay_release() {
        use std::sync::{Arc, Barrier};

        let activity = Arc::new(ActivityState::new());
        activity.acquire_replay().unwrap();
        let health_observed = Arc::new(Barrier::new(2));
        let replay_released = Arc::new(Barrier::new(2));
        let thread_activity = Arc::clone(&activity);
        let thread_health_observed = Arc::clone(&health_observed);
        let thread_replay_released = Arc::clone(&replay_released);
        let admission = std::thread::spawn(move || {
            assert!(!thread_activity.is_poisoned());
            thread_health_observed.wait();
            thread_replay_released.wait();
            (
                thread_activity.acquire_shared(ORDINARY_ONE, ORDINARY_MASK),
                thread_activity.acquire_shared(TRANSIENT_ONE, TRANSIENT_MASK),
                thread_activity.acquire_replay(),
            )
        });

        health_observed.wait();
        activity.poison();
        activity.release_replay();
        replay_released.wait();
        let (ordinary, transient, replay) = admission.join().unwrap();

        assert_eq!(ordinary.unwrap_err(), Error::FoundationPoisoned);
        assert_eq!(transient.unwrap_err(), Error::FoundationPoisoned);
        assert_eq!(replay.unwrap_err(), Error::FoundationPoisoned);
    }

    #[test]
    fn transient_activity_outlives_deferred_owner_cleanup() {
        use std::cell::Cell;
        use std::rc::Rc;

        let foundation: &'static Foundation = Box::leak(Box::new(Foundation {
            config: FoundationConfig::default(),
            activity: ActivityState::new(),
        }));
        foundation
            .activity
            .acquire_shared(TRANSIENT_ONE, TRANSIENT_MASK)
            .unwrap();
        let call = TransientCall {
            _owner_call_frame: callback_state::OwnerCallFrame::enter(),
            _lease: TransientLease { foundation },
        };
        let cleanup_observed = Rc::new(Cell::new(false));
        let cleanup_probe = Rc::clone(&cleanup_observed);
        assert!(
            callback_state::try_defer_local_cleanup(move || {
                assert_eq!(foundation.activity().transient_calls, 1);
                cleanup_probe.set(true);
            })
            .is_ok(),
            "the transient call frame must accept deferred cleanup"
        );

        drop(call);

        assert!(cleanup_observed.get());
        assert_eq!(foundation.activity().transient_calls, 0);
    }
}
