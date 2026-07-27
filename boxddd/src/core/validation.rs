use crate::error::{Error, InvalidValueReason, Result};
use crate::types::{Pos, Quat, Transform, Vec3};
use std::ffi::CString;

#[inline]
pub(crate) const fn invalid(context: &'static str, reason: InvalidValueReason) -> Error {
    Error::InvalidValue { context, reason }
}

#[inline]
pub(crate) fn finite(context: &'static str, value: f32) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::NonFinite))
    }
}

#[inline]
pub(crate) fn nonnegative(context: &'static str, value: f32) -> Result<()> {
    finite(context, value)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::OutOfRange))
    }
}

#[inline]
pub(crate) fn positive(context: &'static str, value: f32) -> Result<()> {
    finite(context, value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::OutOfRange))
    }
}

#[inline]
pub(crate) fn ordered(context: &'static str, lower: f32, upper: f32) -> Result<()> {
    finite(context, lower)?;
    finite(context, upper)?;
    if lower <= upper {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::InvalidCombination))
    }
}

#[inline]
pub(crate) fn vec3(context: &'static str, value: Vec3) -> Result<()> {
    if value.x.is_finite() && value.y.is_finite() && value.z.is_finite() {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::NonFinite))
    }
}

#[inline]
pub(crate) fn position(context: &'static str, value: Pos) -> Result<()> {
    if value.x.is_finite() && value.y.is_finite() && value.z.is_finite() {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::NonFinite))
    }
}

#[inline]
pub(crate) fn quaternion(context: &'static str, value: Quat) -> Result<()> {
    vec3(context, value.v)?;
    finite(context, value.s)?;
    let magnitude_squared =
        value.v.x * value.v.x + value.v.y * value.v.y + value.v.z * value.v.z + value.s * value.s;
    let tolerance = 20.0 * f32::EPSILON;
    if 1.0 - tolerance < magnitude_squared && magnitude_squared < 1.0 + tolerance {
        Ok(())
    } else {
        Err(invalid(context, InvalidValueReason::Malformed))
    }
}

#[inline]
pub(crate) fn transform(context: &'static str, value: Transform) -> Result<()> {
    vec3(context, value.p)?;
    quaternion(context, value.q)
}

pub(crate) fn c_string_value(context: &'static str, value: &str) -> Result<()> {
    count_i32(context, value.len())?;
    if value.as_bytes().contains(&0) {
        Err(invalid(context, InvalidValueReason::InteriorNul))
    } else {
        Ok(())
    }
}

pub(crate) fn c_string(context: &'static str, value: &str) -> Result<CString> {
    c_string_value(context, value)?;
    Ok(CString::new(value).expect("validated string contains an interior NUL"))
}

pub(crate) fn optional_c_string(
    context: &'static str,
    value: Option<&str>,
) -> Result<Option<CString>> {
    value.map(|value| c_string(context, value)).transpose()
}

pub(crate) fn count_i32(context: &'static str, value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| invalid(context, InvalidValueReason::OutOfRange))
}
