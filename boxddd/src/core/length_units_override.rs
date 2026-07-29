use boxddd_sys::ffi;
use std::sync::MutexGuard;

/// Restores Box3D's process-global length-unit setting when a native call ends.
///
/// Borrowing the global lock guard keeps the lock held until this override is dropped.
#[derive(Debug)]
pub(crate) struct LengthUnitsOverride<'guard, 'mutex> {
    previous: f32,
    _guard: &'guard MutexGuard<'mutex, ()>,
}

impl<'guard, 'mutex> LengthUnitsOverride<'guard, 'mutex> {
    pub(crate) fn new_locked(guard: &'guard MutexGuard<'mutex, ()>, length_units: f32) -> Self {
        debug_assert!(length_units.is_finite() && length_units > 0.0);
        let previous = unsafe { ffi::b3GetLengthUnitsPerMeter() };
        unsafe { ffi::b3SetLengthUnitsPerMeter(length_units) };
        Self {
            previous,
            _guard: guard,
        }
    }
}

impl Drop for LengthUnitsOverride<'_, '_> {
    fn drop(&mut self) {
        unsafe { ffi::b3SetLengthUnitsPerMeter(self.previous) };
    }
}
