use crate::core::validation;
use crate::error::Result;
use crate::types::{MotionLocks, Pos, Quat, Vec3};
use boxddd_sys::ffi;
use std::ffi::CString;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
/// The simulation behavior assigned to a body.
pub enum BodyType {
    /// A non-moving body with infinite mass.
    Static,
    /// A body moved explicitly by the user rather than by forces.
    Kinematic,
    /// A fully simulated body affected by forces, contacts, and joints.
    Dynamic,
}

impl BodyType {
    /// Converts this body type to the raw Box3D enum value.
    #[inline]
    pub const fn into_raw(self) -> ffi::b3BodyType {
        match self {
            Self::Static => ffi::b3BodyType_b3_staticBody,
            Self::Kinematic => ffi::b3BodyType_b3_kinematicBody,
            Self::Dynamic => ffi::b3BodyType_b3_dynamicBody,
        }
    }

    /// Converts a raw Box3D body type into the safe Rust enum.
    #[inline]
    pub const fn from_raw(raw: ffi::b3BodyType) -> Option<Self> {
        match raw {
            ffi::b3BodyType_b3_staticBody => Some(Self::Static),
            ffi::b3BodyType_b3_kinematicBody => Some(Self::Kinematic),
            ffi::b3BodyType_b3_dynamicBody => Some(Self::Dynamic),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
/// Parameters used when creating a body in a world.
pub struct BodyDef {
    /// Simulation behavior assigned to the body.
    pub body_type: BodyType,
    /// Initial world-space position.
    pub position: Pos,
    /// Initial world-space rotation.
    pub rotation: Quat,
    /// Initial linear velocity of the body origin.
    pub linear_velocity: Vec3,
    /// Initial angular velocity in radians per second.
    pub angular_velocity: Vec3,
    /// Non-negative linear damping coefficient.
    pub linear_damping: f32,
    /// Non-negative angular damping coefficient.
    pub angular_damping: f32,
    /// Scale applied to world gravity.
    pub gravity_scale: f32,
    /// Non-negative speed threshold used for sleeping.
    pub sleep_threshold: f32,
    /// Optional UTF-8 debug name copied by Box3D during creation.
    pub name: Option<String>,
    /// Per-axis linear and angular motion locks.
    pub motion_locks: MotionLocks,
    /// Whether the body may sleep.
    pub enable_sleep: bool,
    /// Whether the body starts awake.
    pub awake: bool,
    /// Whether the body starts enabled.
    pub enabled: bool,
    /// Whether to use continuous collision handling for this body.
    pub bullet: bool,
    /// Whether rotational speed limits may be bypassed.
    pub allow_fast_rotation: bool,
    /// Whether Box3D may recycle contact storage for this body.
    pub enable_contact_recycling: bool,
}

impl BodyDef {
    pub(crate) fn with_length_units_per_meter(length_units: f32) -> Self {
        Self {
            body_type: BodyType::Static,
            position: Pos::ZERO,
            rotation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            linear_damping: 0.0,
            angular_damping: 0.0,
            gravity_scale: 1.0,
            sleep_threshold: 0.05 * length_units,
            name: None,
            motion_locks: MotionLocks::default(),
            enable_sleep: true,
            awake: true,
            enabled: true,
            bullet: false,
            allow_fast_rotation: false,
            enable_contact_recycling: true,
        }
    }

    /// Checks that all vectors, rotations, and scalar tuning values are finite and valid.
    pub fn validate(&self) -> Result<()> {
        validation::position("body.position", self.position)?;
        validation::quaternion("body.rotation", self.rotation)?;
        validation::vec3("body.linear_velocity", self.linear_velocity)?;
        validation::vec3("body.angular_velocity", self.angular_velocity)?;
        validation::nonnegative("body.linear_damping", self.linear_damping)?;
        validation::nonnegative("body.angular_damping", self.angular_damping)?;
        validation::finite("body.gravity_scale", self.gravity_scale)?;
        validation::nonnegative("body.sleep_threshold", self.sleep_threshold)?;
        if let Some(name) = self.name.as_deref() {
            validation::c_string_value("body.name", name)?;
        }
        Ok(())
    }

    pub(crate) fn prepare(&self) -> Result<PreparedBodyDef<'_>> {
        self.validate()?;
        let name = validation::optional_c_string("body.name", self.name.as_deref())?;
        Ok(PreparedBodyDef { def: self, name })
    }
}

pub(crate) struct PreparedBodyDef<'a> {
    def: &'a BodyDef,
    name: Option<CString>,
}

impl PreparedBodyDef<'_> {
    pub(crate) fn create(self, world: ffi::b3WorldId) -> ffi::b3BodyId {
        self.invoke(|raw| unsafe { ffi::b3CreateBody(world, raw) })
    }

    fn invoke(self, create: impl FnOnce(&ffi::b3BodyDef) -> ffi::b3BodyId) -> ffi::b3BodyId {
        let mut raw = unsafe { ffi::b3DefaultBodyDef() };
        raw.type_ = self.def.body_type.into_raw();
        raw.position = self.def.position.into_raw();
        raw.rotation = self.def.rotation.into_raw();
        raw.linearVelocity = self.def.linear_velocity.into_raw();
        raw.angularVelocity = self.def.angular_velocity.into_raw();
        raw.linearDamping = self.def.linear_damping;
        raw.angularDamping = self.def.angular_damping;
        raw.gravityScale = self.def.gravity_scale;
        raw.sleepThreshold = self.def.sleep_threshold;
        raw.name = self
            .name
            .as_ref()
            .map_or(std::ptr::null(), |name| name.as_ptr());
        raw.userData = std::ptr::null_mut();
        raw.motionLocks = self.def.motion_locks.into_raw();
        raw.enableSleep = self.def.enable_sleep;
        raw.isAwake = self.def.awake;
        raw.isEnabled = self.def.enabled;
        raw.isBullet = self.def.bullet;
        raw.allowFastRotation = self.def.allow_fast_rotation;
        raw.enableContactRecycling = self.def.enable_contact_recycling;
        create(&raw)
    }
}

#[cfg(test)]
mod prepared_definition_tests {
    use super::*;

    #[test]
    fn prepared_body_exposes_a_complete_native_create_operation() {
        let _: fn(PreparedBodyDef<'static>, ffi::b3WorldId) -> ffi::b3BodyId =
            PreparedBodyDef::create;
    }
}

#[derive(Clone, Debug)]
/// Builder for a Box3D body definition.
pub struct BodyDefBuilder {
    def: BodyDef,
}

impl BodyDefBuilder {
    pub(crate) fn from_def(def: BodyDef) -> Self {
        Self { def }
    }

    /// Sets whether the body is static, kinematic, or dynamic.
    #[inline]
    pub fn body_type(mut self, body_type: BodyType) -> Self {
        self.def.body_type = body_type;
        self
    }

    /// Sets the initial world-space body position.
    #[inline]
    pub fn position(mut self, position: impl Into<Pos>) -> Self {
        self.def.position = position.into();
        self
    }

    /// Sets the initial world-space body rotation.
    #[inline]
    pub fn rotation(mut self, rotation: Quat) -> Self {
        self.def.rotation = rotation;
        self
    }

    /// Sets the initial linear velocity.
    #[inline]
    pub fn linear_velocity(mut self, velocity: impl Into<Vec3>) -> Self {
        self.def.linear_velocity = velocity.into();
        self
    }

    /// Sets the initial angular velocity.
    #[inline]
    pub fn angular_velocity(mut self, velocity: impl Into<Vec3>) -> Self {
        self.def.angular_velocity = velocity.into();
        self
    }

    /// Scales gravity applied to this body.
    #[inline]
    pub fn gravity_scale(mut self, gravity_scale: f32) -> Self {
        self.def.gravity_scale = gravity_scale;
        self
    }

    /// Enables or disables continuous collision handling for fast-moving bodies.
    #[inline]
    pub fn bullet(mut self, is_bullet: bool) -> Self {
        self.def.bullet = is_bullet;
        self
    }

    /// Sets the optional body name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.def.name = Some(name.into());
        self
    }

    /// Sets the linear damping coefficient.
    pub fn linear_damping(mut self, damping: f32) -> Self {
        self.def.linear_damping = damping;
        self
    }

    /// Sets the angular damping coefficient.
    pub fn angular_damping(mut self, damping: f32) -> Self {
        self.def.angular_damping = damping;
        self
    }

    /// Sets the speed threshold below which the body may sleep.
    pub fn sleep_threshold(mut self, threshold: f32) -> Self {
        self.def.sleep_threshold = threshold;
        self
    }

    /// Sets per-axis motion locks.
    pub fn motion_locks(mut self, locks: MotionLocks) -> Self {
        self.def.motion_locks = locks;
        self
    }

    /// Enables or disables sleeping for this body.
    pub fn enable_sleep(mut self, enabled: bool) -> Self {
        self.def.enable_sleep = enabled;
        self
    }

    /// Sets whether the body starts awake.
    pub fn awake(mut self, awake: bool) -> Self {
        self.def.awake = awake;
        self
    }

    /// Sets whether the body starts enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.def.enabled = enabled;
        self
    }

    /// Allows this body to bypass rotational speed limits.
    pub fn allow_fast_rotation(mut self, allowed: bool) -> Self {
        self.def.allow_fast_rotation = allowed;
        self
    }

    /// Enables or disables contact recycling for this body.
    pub fn enable_contact_recycling(mut self, enabled: bool) -> Self {
        self.def.enable_contact_recycling = enabled;
        self
    }

    /// Validates and finishes the body definition.
    #[inline]
    pub fn build(self) -> Result<BodyDef> {
        self.def.validate()?;
        Ok(self.def)
    }
}
