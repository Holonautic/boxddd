use crate::core::provenance::OwnerToken;
use crate::error::{Error, Result};
use boxddd_sys::ffi;
use std::sync::{Mutex, OnceLock};

static ACTIVE_LEASE: OnceLock<Mutex<Option<OwnerToken>>> = OnceLock::new();

fn active_lease() -> &'static Mutex<Option<OwnerToken>> {
    ACTIVE_LEASE.get_or_init(|| Mutex::new(None))
}

/// Exclusive ownership of Box3D's process-global length-unit setting.
///
/// Callers must hold the Box3D global lock for every operation on this type.
#[derive(Debug)]
pub(crate) struct LengthUnitsLease {
    owner: OwnerToken,
    previous: f32,
}

impl LengthUnitsLease {
    pub(crate) fn acquire_locked(owner: OwnerToken) -> Result<Self> {
        let mut active = active_lease()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if active.is_some() {
            return Err(Error::ResourceLifetimeViolation);
        }

        let previous = unsafe { ffi::b3GetLengthUnitsPerMeter() };
        *active = Some(owner);
        Ok(Self { owner, previous })
    }

    /// Repairs the native scale if player creation failed after changing it.
    pub(crate) fn restore_after_failed_create_locked(&self) {
        unsafe { ffi::b3SetLengthUnitsPerMeter(self.previous) };
    }

    pub(crate) fn release_locked(self) {
        let mut active = active_lease()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        debug_assert_eq!(*active, Some(self.owner));
        if *active == Some(self.owner) {
            *active = None;
        }
    }
}

/// Rejects safe writes that would invalidate an active replay player's native state.
pub(crate) fn ensure_writable_locked() -> Result<()> {
    let active = active_lease()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if active.is_some() {
        Err(Error::ResourceLifetimeViolation)
    } else {
        Ok(())
    }
}
