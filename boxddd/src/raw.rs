//! Explicit raw interop boundary for Box3D APIs that cannot be made ordinary safe Rust APIs.
//!
//! This module is intentionally not re-exported by `boxddd::prelude`. Functions here preserve
//! the crate's handle validation and Box3D global lock, but they still expose native concepts
//! such as process-global settings and untyped `void*` user data.

use crate::core::{box3d_lock, callback_state, debug_checks, length_units_lease};
use crate::error::{Error, Result};
use crate::joints::lock_joint_checked;
use crate::types::{BodyId, ContactId, JointId, ShapeId};
use crate::world::World;
use boxddd_sys::ffi;
use std::ffi::c_void;
use std::fmt;

/// Scoped access to the native IDs owned by a [`World`].
///
/// The guard holds boxddd's process-wide Box3D lock for its complete lifetime. It only resolves
/// IDs already present in the safe World's ledger; it never adopts native resources.
pub struct WorldRawGuard<'a> {
    world: &'a mut World,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl fmt::Debug for WorldRawGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorldRawGuard")
            .finish_non_exhaustive()
    }
}

impl WorldRawGuard<'_> {
    /// Returns the native world ID while the guard keeps all Rust sidecars alive and Box3D locked.
    pub fn world_id(&self) -> ffi::b3WorldId {
        self.world.raw()
    }

    /// Resolves a live body handle into its native ID without calling Box3D.
    pub fn body_id(&self, id: BodyId) -> Result<ffi::b3BodyId> {
        self.world.state.ledger.authorize_body(id)
    }

    /// Resolves a live shape handle into its native ID without calling Box3D.
    pub fn shape_id(&self, id: ShapeId) -> Result<ffi::b3ShapeId> {
        self.world.state.ledger.authorize_shape(id)
    }

    /// Resolves a live joint handle into its native ID without calling Box3D.
    pub fn joint_id(&self, id: JointId) -> Result<ffi::b3JointId> {
        self.world.state.ledger.authorize_joint(id)
    }

    /// Resolves a live contact handle into its native ID.
    pub fn contact_id(&self, id: ContactId) -> Result<ffi::b3ContactId> {
        let raw = self.world.state.ledger.authorize_contact(id)?;
        debug_checks::check_contact_valid_raw(raw)?;
        Ok(raw)
    }
}

/// Complete native ownership transfer for a [`World`].
///
/// Unlike a naked `b3WorldId`, this bundle owns every Rust ledger, retained shape allocation,
/// callback/task context, provider token, and debug registry required to keep the native world
/// valid and destroy it exactly once. Dropping the bundle destroys the world normally.
#[must_use = "dropping raw world parts destroys the native world and all retained sidecars"]
#[derive(Debug)]
pub struct WorldRawParts {
    world: Option<World>,
}

impl WorldRawParts {
    /// Creates a scoped raw guard over the transferred world.
    ///
    /// # Safety
    ///
    /// Native calls made with IDs from the guard must not create, destroy, or adopt structural
    /// resources. They must also obey Box3D's pointer, callback, and thread-safety contracts.
    pub unsafe fn world_guard(&mut self) -> Result<WorldRawGuard<'_>> {
        let world = self.world.as_mut().ok_or(Error::InvalidWorldId)?;
        unsafe { world_raw_guard(world) }
    }

    /// Reconstitutes the safe owner after a raw handoff.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that no structural native mutation occurred while the bundle was
    /// raw-owned, no native callback or task can still enter, and no copied raw ID will be used
    /// after this call. Arbitrary native mutations cannot be reconciled by scanning Box3D.
    pub unsafe fn into_world(mut self) -> Result<World> {
        callback_state::check_not_in_callback()?;
        {
            let world = self.world.as_ref().ok_or(Error::InvalidWorldId)?;
            let _guard = box3d_lock::lock();
            world.check_world_valid_locked()?;
        }
        Ok(self.world.take().expect("raw parts still own their world"))
    }

    /// Explicitly destroys the transferred world and all retained Rust state.
    ///
    /// # Safety
    ///
    /// No native callback, task, pointer, or copied raw ID may be used after this call.
    pub unsafe fn destroy(self) {
        drop(self);
    }
}

impl World {
    /// Consumes this safe owner and transfers the complete native world into an owning bundle.
    ///
    /// ```compile_fail
    /// use boxddd::{World, WorldDef};
    ///
    /// let world = World::new(WorldDef::default()).unwrap();
    /// let parts = world.into_raw_parts().unwrap();
    /// let _still_owned = world;
    /// drop(parts);
    /// ```
    pub fn into_raw_parts(self) -> Result<WorldRawParts> {
        callback_state::check_not_in_callback()?;
        {
            let _guard = box3d_lock::lock();
            self.check_world_valid_locked()?;
        }
        Ok(WorldRawParts { world: Some(self) })
    }
}

/// Opens a scoped raw view of a safe World.
///
/// # Safety
///
/// Native calls made with IDs from the guard must be observational or non-structural and must not
/// retain borrowed pointers past the guard. Structural mutation requires consuming
/// [`World::into_raw_parts`] and complete unsafe reconciliation before returning to safe APIs.
pub unsafe fn world_raw_guard(world: &mut World) -> Result<WorldRawGuard<'_>> {
    callback_state::check_not_in_callback()?;
    let lock = box3d_lock::lock();
    world.check_world_valid_locked()?;
    Ok(WorldRawGuard { world, _lock: lock })
}

/// Sets the raw Box3D `userData` pointer attached to a world.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn try_set_world_raw_user_data(world: &mut World, user_data: *mut c_void) -> Result<()> {
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    world.check_world_valid_locked()?;
    unsafe { ffi::b3World_SetUserData(world.raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a world.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn try_world_raw_user_data(world: &World) -> Result<*mut c_void> {
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    world.check_world_valid_locked()?;
    Ok(unsafe { ffi::b3World_GetUserData(world.raw()) })
}

/// Sets the raw Box3D `userData` pointer attached to a body.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn try_set_body_raw_user_data(
    world: &mut World,
    body_id: BodyId,
    user_data: *mut c_void,
) -> Result<()> {
    let _guard = world.lock_body_checked(body_id)?;
    unsafe { ffi::b3Body_SetUserData(body_id.into_raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a body.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn try_body_raw_user_data(world: &World, body_id: BodyId) -> Result<*mut c_void> {
    let _guard = world.lock_body_checked(body_id)?;
    Ok(unsafe { ffi::b3Body_GetUserData(body_id.into_raw()) })
}

/// Sets the raw Box3D `userData` pointer attached to a shape.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn try_set_shape_raw_user_data(
    world: &mut World,
    shape_id: ShapeId,
    user_data: *mut c_void,
) -> Result<()> {
    let _guard = world.lock_shape_checked(shape_id)?;
    unsafe { ffi::b3Shape_SetUserData(shape_id.into_raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a shape.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn try_shape_raw_user_data(world: &World, shape_id: ShapeId) -> Result<*mut c_void> {
    let _guard = world.lock_shape_checked(shape_id)?;
    Ok(unsafe { ffi::b3Shape_GetUserData(shape_id.into_raw()) })
}

/// Sets the raw Box3D `userData` pointer attached to a joint.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn try_set_joint_raw_user_data(
    world: &mut World,
    joint_id: JointId,
    user_data: *mut c_void,
) -> Result<()> {
    let _guard = lock_joint_checked(world, joint_id)?;
    unsafe { ffi::b3Joint_SetUserData(joint_id.into_raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a joint.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn try_joint_raw_user_data(world: &World, joint_id: JointId) -> Result<*mut c_void> {
    let _guard = lock_joint_checked(world, joint_id)?;
    Ok(unsafe { ffi::b3Joint_GetUserData(joint_id.into_raw()) })
}

/// Returns Box3D's process-global length unit scale.
///
/// This is process-global native state. Prefer leaving it unchanged unless interoperating with
/// existing Box3D code that has intentionally changed the unit scale.
pub fn length_units_per_meter() -> f32 {
    try_length_units_per_meter().expect("boxddd raw API called from a Box3D callback")
}

/// Tries to return Box3D's process-global length unit scale.
pub fn try_length_units_per_meter() -> Result<f32> {
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    Ok(unsafe { ffi::b3GetLengthUnitsPerMeter() })
}

/// Sets Box3D's process-global length unit scale.
///
/// The value must be finite and greater than zero.
pub fn try_set_length_units_per_meter(length_units: f32) -> Result<()> {
    validate_positive(length_units)?;
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    length_units_lease::ensure_writable_locked()?;
    unsafe { ffi::b3SetLengthUnitsPerMeter(length_units) };
    Ok(())
}

/// Returns Box3D's process-global stall threshold in seconds.
pub fn stall_threshold() -> f32 {
    try_stall_threshold().expect("boxddd raw API called from a Box3D callback")
}

/// Tries to return Box3D's process-global stall threshold in seconds.
pub fn try_stall_threshold() -> Result<f32> {
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    Ok(unsafe { ffi::b3GetStallThreshold() })
}

/// Sets Box3D's process-global stall threshold in seconds.
///
/// The value must be finite and non-negative.
pub fn try_set_stall_threshold(seconds: f32) -> Result<()> {
    validate_nonnegative(seconds)?;
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    unsafe { ffi::b3SetStallThreshold(seconds) };
    Ok(())
}

#[inline]
fn validate_positive(value: f32) -> Result<()> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(Error::InvalidArgument)
    }
}

#[inline]
fn validate_nonnegative(value: f32) -> Result<()> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(Error::InvalidArgument)
    }
}
