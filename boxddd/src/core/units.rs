use crate::core::foundation::ReplayLease;
use crate::error::{Error, Result};
use boxddd_sys::ffi;
use std::sync::atomic::{AtomicBool, Ordering};

static FORCE_RESTORE_MISMATCH: AtomicBool = AtomicBool::new(false);

pub(crate) fn verify_replay_scale(lease: &ReplayLease, expected: f32) -> Result<()> {
    verify_observed(lease, expected, false)
}

pub(crate) fn restore_foundation_scale(lease: &ReplayLease) -> Result<()> {
    let expected = lease.foundation().config().length_units_per_meter;
    unsafe { ffi::b3SetLengthUnitsPerMeter(expected) };
    verify_observed(lease, expected, true)
}

fn verify_observed(lease: &ReplayLease, expected: f32, inject_restore_fault: bool) -> Result<()> {
    let observed = unsafe { ffi::b3GetLengthUnitsPerMeter() };
    let forced_mismatch =
        inject_restore_fault && FORCE_RESTORE_MISMATCH.swap(false, Ordering::AcqRel);
    if !forced_mismatch && observed.to_bits() == expected.to_bits() {
        Ok(())
    } else {
        lease.foundation().poison();
        Err(Error::FoundationPoisoned)
    }
}

pub(crate) fn force_next_restore_mismatch_for_test() {
    FORCE_RESTORE_MISMATCH.store(true, Ordering::Release);
}
