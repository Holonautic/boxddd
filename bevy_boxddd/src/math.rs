//! Bevy math adapters for `boxddd` value types.
//!
//! The core `boxddd` crate stays engine-independent. These helpers live in the
//! Bevy adapter crate so Bevy applications do not need to repeat small
//! conversion functions in every system or example.

use bevy_math::{Quat as BevyQuat, Vec3 as BevyVec3};
use bevy_transform::components::Transform as BevyTransform;

/// Converts a Bevy `Vec3` to a local-space Box3D vector.
#[inline]
pub fn to_boxddd_vec3(value: BevyVec3) -> boxddd::Vec3 {
    boxddd::Vec3::new(value.x, value.y, value.z)
}

/// Converts a Bevy `Vec3` to a Box3D world position.
#[inline]
pub fn to_boxddd_pos(value: BevyVec3) -> boxddd::Pos {
    boxddd::Pos::new(value.x.into(), value.y.into(), value.z.into())
}

/// Converts and validates a Bevy quaternion.
#[inline]
pub fn to_boxddd_quat(value: BevyQuat) -> boxddd::Result<boxddd::Quat> {
    boxddd_quat_unchecked(value).validate()
}

/// Converts a Bevy transform to a Box3D local transform, ignoring Bevy scale.
#[inline]
pub fn to_boxddd_local_transform(value: BevyTransform) -> boxddd::Result<boxddd::Transform> {
    Ok(boxddd::Transform::new(
        to_boxddd_vec3(value.translation),
        to_boxddd_quat(value.rotation)?,
    ))
}

/// Converts a Bevy transform to a Box3D world transform, ignoring Bevy scale.
#[inline]
pub fn to_boxddd_world_transform(value: BevyTransform) -> boxddd::Result<boxddd::WorldTransform> {
    Ok(boxddd::WorldTransform::new(
        to_boxddd_pos(value.translation),
        to_boxddd_quat(value.rotation)?,
    ))
}

#[inline]
fn boxddd_quat_unchecked(value: BevyQuat) -> boxddd::Quat {
    boxddd::Quat::new(boxddd::Vec3::new(value.x, value.y, value.z), value.w)
}

/// Converts a Box3D local-space vector to a Bevy `Vec3`.
#[inline]
pub fn to_bevy_vec3(value: boxddd::Vec3) -> BevyVec3 {
    BevyVec3::new(value.x, value.y, value.z)
}

/// Converts a Box3D world position to a Bevy `Vec3`.
///
/// In double-precision builds this intentionally casts the world position down
/// to Bevy's `f32` coordinate type.
#[inline]
pub fn to_bevy_pos(value: boxddd::Pos) -> BevyVec3 {
    BevyVec3::new(value.x as f32, value.y as f32, value.z as f32)
}

/// Converts a Box3D quaternion to a Bevy quaternion.
#[inline]
pub fn to_bevy_quat(value: boxddd::Quat) -> BevyQuat {
    BevyQuat::from_xyzw(value.v.x, value.v.y, value.v.z, value.s)
}

/// Converts a Box3D local transform to a Bevy transform with unit scale.
#[inline]
pub fn to_bevy_local_transform(value: boxddd::Transform) -> BevyTransform {
    BevyTransform::from_translation(to_bevy_vec3(value.p)).with_rotation(to_bevy_quat(value.q))
}

/// Converts a Box3D world transform to a Bevy transform with unit scale.
#[inline]
pub fn to_bevy_transform(value: boxddd::WorldTransform) -> BevyTransform {
    BevyTransform::from_translation(to_bevy_pos(value.p)).with_rotation(to_bevy_quat(value.q))
}

/// Applies a Box3D world transform to an existing Bevy transform, preserving scale.
#[inline]
pub fn apply_boxddd_transform(target: &mut BevyTransform, value: boxddd::WorldTransform) {
    target.translation = to_bevy_pos(value.p);
    target.rotation = to_bevy_quat(value.q);
}
