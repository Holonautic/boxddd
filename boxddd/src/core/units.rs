use crate::core::{box3d_lock, callback_state};
use boxddd_sys::ffi;

/// Reads Box3D's process-global scale without recursively taking the lock from a callback.
pub(crate) fn current_length_units() -> f32 {
    if callback_state::in_callback() {
        // Box3D invokes registered callbacks while the outer operation owns the global lock.
        unsafe { ffi::b3GetLengthUnitsPerMeter() }
    } else {
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3GetLengthUnitsPerMeter() }
    }
}
