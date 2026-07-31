use crate::core::{callback_state, debug_checks, validation};
use crate::error::{Error, InvalidValueReason, Result};
use crate::types::{BodyId, JointId, Quat, Transform, Vec3};
use crate::world::World;
use boxddd_sys::ffi;

mod defs;
pub use defs::*;

macro_rules! family_method {
    ($world:expr, $joint:expr, $ty:expr, $body:block) => {{
        let _call = enter_typed_joint_call($world, $joint, $ty)?;
        let result = $body;
        Ok(result)
    }};
}

mod distance;
mod motor;
mod parallel;
mod prismatic;
mod revolute;
mod spherical;
mod wheel;
mod world_api;

#[inline]
fn check_joint_body_pair_valid(body_a: BodyId, body_b: BodyId) -> Result<()> {
    if body_a != body_b {
        Ok(())
    } else {
        Err(validation::invalid(
            "joint.bodies",
            InvalidValueReason::InvalidCombination,
        ))
    }
}

#[inline]
fn validate_length_range(context: &'static str, lower: f32, upper: f32) -> Result<()> {
    validation::nonnegative(context, lower)?;
    validation::nonnegative(context, upper)?;
    validation::ordered(context, lower, upper)
}

#[inline]
pub(crate) fn enter_joint_call(
    world: &World,
    joint_id: JointId,
) -> Result<callback_state::OwnerCallFrame> {
    world.enter_joint_call(joint_id)
}

#[inline]
fn enter_typed_joint_call(
    world: &World,
    joint_id: JointId,
    expected: JointType,
) -> Result<callback_state::OwnerCallFrame> {
    let call = enter_joint_call(world, joint_id)?;
    let actual = JointType::from_raw(unsafe { ffi::b3Joint_GetType(joint_id.into_raw()) })
        .ok_or(Error::NativeFailure)?;
    if actual == expected {
        Ok(call)
    } else {
        Err(validation::invalid(
            "joint.type",
            InvalidValueReason::InvalidCombination,
        ))
    }
}
