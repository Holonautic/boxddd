//! Explicit raw interop boundary for Box3D APIs that cannot be made ordinary safe Rust APIs.
//!
//! This module is intentionally not re-exported by `boxddd::prelude`. Functions here preserve
//! Foundation admission and the crate's handle validation, but they still expose native concepts
//! such as untyped `void*` user data. Direct `boxddd-sys` calls bypass those checks entirely.

use crate::core::{callback_state, debug_checks};
use crate::error::{Error, Result};
use crate::joints::enter_joint_call;
use crate::types::{BodyId, ContactId, JointId, ShapeId};
use crate::world::World;
use boxddd_sys::ffi;
use std::ffi::c_void;
use std::fmt;

/// Scoped access to the native IDs owned by a [`World`].
///
/// The guard exclusively borrows the World and keeps an owner call frame active for its complete
/// lifetime. It only resolves IDs already present in the safe World's ledger; it never adopts
/// native resources.
pub struct WorldRawGuard<'a> {
    world: &'a mut World,
    _call: callback_state::OwnerCallFrame,
}

impl fmt::Debug for WorldRawGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorldRawGuard")
            .finish_non_exhaustive()
    }
}

impl WorldRawGuard<'_> {
    /// Returns the native world ID while the guard keeps the owner call and all sidecars alive.
    pub fn world_id(&self) -> ffi::b3WorldId {
        self.world.raw()
    }

    /// Resolves a live body handle into its native ID without calling Box3D.
    pub fn body_id(&self, id: BodyId) -> Result<ffi::b3BodyId> {
        self.world.state().ledger.authorize_body(id)
    }

    /// Resolves a live shape handle into its native ID without calling Box3D.
    pub fn shape_id(&self, id: ShapeId) -> Result<ffi::b3ShapeId> {
        self.world.state().ledger.authorize_shape(id)
    }

    /// Resolves a live joint handle into its native ID without calling Box3D.
    pub fn joint_id(&self, id: JointId) -> Result<ffi::b3JointId> {
        self.world.state().ledger.authorize_joint(id)
    }

    /// Resolves a live contact handle into its native ID.
    pub fn contact_id(&self, id: ContactId) -> Result<ffi::b3ContactId> {
        let raw = self.world.state().ledger.authorize_contact(id)?;
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
        let world = self.world.as_mut().ok_or(Error::NativeFailure)?;
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
            let world = self.world.as_ref().ok_or(Error::NativeFailure)?;
            let _call = world.enter_world_call()?;
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
    /// use boxddd::Foundation;
    ///
    /// let foundation = Foundation::initialize_default().unwrap();
    /// let world = foundation.create_world(foundation.world_def()).unwrap();
    /// let parts = world.into_raw_parts().unwrap();
    /// let _still_owned = world;
    /// drop(parts);
    /// ```
    pub fn into_raw_parts(self) -> Result<WorldRawParts> {
        {
            let _call = self.enter_world_call()?;
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
    let call = world.enter_call()?;
    world.check_world_valid()?;
    Ok(WorldRawGuard { world, _call: call })
}

/// Sets the raw Box3D `userData` pointer attached to a world.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn set_world_raw_user_data(world: &mut World, user_data: *mut c_void) -> Result<()> {
    let _call = world.enter_world_call()?;
    unsafe { ffi::b3World_SetUserData(world.raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a world.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn world_raw_user_data(world: &World) -> Result<*mut c_void> {
    let _call = world.enter_world_call()?;
    Ok(unsafe { ffi::b3World_GetUserData(world.raw()) })
}

/// Sets the raw Box3D `userData` pointer attached to a body.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn set_body_raw_user_data(
    world: &mut World,
    body_id: BodyId,
    user_data: *mut c_void,
) -> Result<()> {
    let _call = world.enter_body_call(body_id)?;
    unsafe { ffi::b3Body_SetUserData(body_id.into_raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a body.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn body_raw_user_data(world: &World, body_id: BodyId) -> Result<*mut c_void> {
    let _call = world.enter_body_call(body_id)?;
    Ok(unsafe { ffi::b3Body_GetUserData(body_id.into_raw()) })
}

/// Sets the raw Box3D `userData` pointer attached to a shape.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn set_shape_raw_user_data(
    world: &mut World,
    shape_id: ShapeId,
    user_data: *mut c_void,
) -> Result<()> {
    let _call = world.enter_shape_call(shape_id)?;
    unsafe { ffi::b3Shape_SetUserData(shape_id.into_raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a shape.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn shape_raw_user_data(world: &World, shape_id: ShapeId) -> Result<*mut c_void> {
    let _call = world.enter_shape_call(shape_id)?;
    Ok(unsafe { ffi::b3Shape_GetUserData(shape_id.into_raw()) })
}

/// Sets the raw Box3D `userData` pointer attached to a joint.
///
/// # Safety
///
/// The caller must ensure `user_data` remains valid for every native Box3D use and must not rely
/// on `boxddd` to manage, alias-check, or drop the pointed-to value.
pub unsafe fn set_joint_raw_user_data(
    world: &mut World,
    joint_id: JointId,
    user_data: *mut c_void,
) -> Result<()> {
    let _call = enter_joint_call(world, joint_id)?;
    unsafe { ffi::b3Joint_SetUserData(joint_id.into_raw(), user_data) };
    Ok(())
}

/// Returns the raw Box3D `userData` pointer attached to a joint.
///
/// # Safety
///
/// The returned pointer is not validated by `boxddd`. The caller is responsible for interpreting
/// it only according to the ownership and lifetime contract used when it was stored.
pub unsafe fn joint_raw_user_data(world: &World, joint_id: JointId) -> Result<*mut c_void> {
    let _call = enter_joint_call(world, joint_id)?;
    Ok(unsafe { ffi::b3Joint_GetUserData(joint_id.into_raw()) })
}
