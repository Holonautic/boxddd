use crate::error::{Error, Result};
use boxddd_sys::ffi;

#[inline]
pub(crate) fn check_body_valid_raw(id: ffi::b3BodyId) -> Result<()> {
    if unsafe { ffi::b3Body_IsValid(id) } {
        Ok(())
    } else {
        Err(Error::NativeFailure)
    }
}

#[inline]
pub(crate) fn check_shape_valid_raw(id: ffi::b3ShapeId) -> Result<()> {
    if unsafe { ffi::b3Shape_IsValid(id) } {
        Ok(())
    } else {
        Err(Error::NativeFailure)
    }
}

#[inline]
pub(crate) fn check_joint_valid_raw(id: ffi::b3JointId) -> Result<()> {
    if unsafe { ffi::b3Joint_IsValid(id) } {
        Ok(())
    } else {
        Err(Error::NativeFailure)
    }
}

#[inline]
pub(crate) fn check_contact_valid_raw(id: ffi::b3ContactId) -> Result<()> {
    if unsafe { ffi::b3Contact_IsValid(id) } {
        Ok(())
    } else {
        Err(Error::NativeFailure)
    }
}
