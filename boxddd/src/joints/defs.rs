use crate::core::validation;
use crate::error::{InvalidValueReason, Result};
use crate::types::{BodyId, Quat, Transform, Vec3};
use boxddd_sys::ffi;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Native Box3D joint family reported for a joint handle.
pub enum JointType {
    /// Keeps two body frames parallel.
    Parallel,
    /// Constrains the distance between two body frames.
    Distance,
    /// Disables collision between two connected bodies.
    Filter,
    /// Drives relative linear and angular motion.
    Motor,
    /// Allows translation along one axis.
    Prismatic,
    /// Allows rotation around one axis.
    Revolute,
    /// Allows ball-and-socket rotation.
    Spherical,
    /// Locks relative translation and rotation.
    Weld,
    /// Provides suspension, spin, and steering controls.
    Wheel,
}

impl JointType {
    pub(crate) const fn from_raw(raw: ffi::b3JointType) -> Option<Self> {
        match raw {
            ffi::b3JointType_b3_parallelJoint => Some(Self::Parallel),
            ffi::b3JointType_b3_distanceJoint => Some(Self::Distance),
            ffi::b3JointType_b3_filterJoint => Some(Self::Filter),
            ffi::b3JointType_b3_motorJoint => Some(Self::Motor),
            ffi::b3JointType_b3_prismaticJoint => Some(Self::Prismatic),
            ffi::b3JointType_b3_revoluteJoint => Some(Self::Revolute),
            ffi::b3JointType_b3_sphericalJoint => Some(Self::Spherical),
            ffi::b3JointType_b3_weldJoint => Some(Self::Weld),
            ffi::b3JointType_b3_wheelJoint => Some(Self::Wheel),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
/// Frequency and damping parameters shared by joint constraints.
pub struct JointTuning {
    /// Constraint frequency in hertz.
    pub hertz: f32,
    /// Non-negative damping ratio.
    pub damping_ratio: f32,
}

impl JointTuning {
    /// Creates a tuning value.
    pub const fn new(hertz: f32, damping_ratio: f32) -> Self {
        Self {
            hertz,
            damping_ratio,
        }
    }

    pub(crate) fn validate(self) -> Result<()> {
        validation::nonnegative("joint_tuning.hertz", self.hertz)?;
        validation::nonnegative("joint_tuning.damping_ratio", self.damping_ratio)
    }
}

#[derive(Copy, Clone, Debug)]
struct JointCommon {
    body_a: BodyId,
    body_b: BodyId,
    local_frame_a: Transform,
    local_frame_b: Transform,
    force_threshold: f32,
    torque_threshold: f32,
    constraint_tuning: JointTuning,
    draw_scale: Option<f32>,
    collide_connected: bool,
}

impl JointCommon {
    const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            body_a,
            body_b,
            local_frame_a: Transform::IDENTITY,
            local_frame_b: Transform::IDENTITY,
            force_threshold: f32::MAX,
            torque_threshold: f32::MAX,
            constraint_tuning: JointTuning::new(60.0, 2.0),
            draw_scale: None,
            collide_connected: false,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.body_a == self.body_b {
            return Err(validation::invalid(
                "joint.bodies",
                InvalidValueReason::InvalidCombination,
            ));
        }
        validation::transform("joint.local_frame_a", self.local_frame_a)?;
        validation::transform("joint.local_frame_b", self.local_frame_b)?;
        validation::nonnegative("joint.force_threshold", self.force_threshold)?;
        validation::nonnegative("joint.torque_threshold", self.torque_threshold)?;
        validation::nonnegative("joint.constraint_hertz", self.constraint_tuning.hertz)?;
        validation::nonnegative(
            "joint.constraint_damping_ratio",
            self.constraint_tuning.damping_ratio,
        )?;
        if let Some(draw_scale) = self.draw_scale {
            validation::nonnegative("joint.draw_scale", draw_scale)?;
        }
        Ok(())
    }

    fn lower(&self, raw: &mut ffi::b3JointDef) {
        raw.userData = std::ptr::null_mut();
        raw.bodyIdA = self.body_a.into_raw();
        raw.bodyIdB = self.body_b.into_raw();
        raw.localFrameA = self.local_frame_a.into_raw();
        raw.localFrameB = self.local_frame_b.into_raw();
        raw.forceThreshold = self.force_threshold;
        raw.torqueThreshold = self.torque_threshold;
        raw.constraintHertz = self.constraint_tuning.hertz;
        raw.constraintDampingRatio = self.constraint_tuning.damping_ratio;
        if let Some(draw_scale) = self.draw_scale {
            raw.drawScale = draw_scale;
        }
        raw.collideConnected = self.collide_connected;
    }
}

macro_rules! impl_joint_common_api {
    ($ty:ident) => {
        impl $ty {
            /// Replaces the two bodies connected by the joint.
            pub fn body_ids(mut self, body_a: BodyId, body_b: BodyId) -> Self {
                self.common.body_a = body_a;
                self.common.body_b = body_b;
                self
            }

            /// Sets the local constraint frame on body A.
            pub fn local_frame_a(mut self, frame: Transform) -> Self {
                self.common.local_frame_a = frame;
                self
            }

            /// Sets the local constraint frame on body B.
            pub fn local_frame_b(mut self, frame: Transform) -> Self {
                self.common.local_frame_b = frame;
                self
            }

            /// Controls whether the connected bodies may collide.
            pub fn collide_connected(mut self, enabled: bool) -> Self {
                self.common.collide_connected = enabled;
                self
            }

            /// Sets the force threshold used to emit joint events, typically in newtons.
            pub fn force_threshold(mut self, threshold: f32) -> Self {
                self.common.force_threshold = threshold;
                self
            }

            /// Sets the torque threshold used to emit joint events, typically in newton-meters.
            pub fn torque_threshold(mut self, threshold: f32) -> Self {
                self.common.torque_threshold = threshold;
                self
            }

            /// Sets common constraint frequency in hertz and its dimensionless damping ratio.
            pub fn constraint_tuning(mut self, tuning: JointTuning) -> Self {
                self.common.constraint_tuning = tuning;
                self
            }

            /// Overrides the length-unit-derived native debug draw scale.
            pub fn draw_scale(mut self, draw_scale: f32) -> Self {
                self.common.draw_scale = Some(draw_scale);
                self
            }

            pub(super) const fn bodies(&self) -> (BodyId, BodyId) {
                (self.common.body_a, self.common.body_b)
            }
        }
    };
}

fn bounded(context: &'static str, value: f32, lower: f32, upper: f32) -> Result<()> {
    validation::finite(context, value)?;
    if (lower..=upper).contains(&value) {
        Ok(())
    } else {
        Err(validation::invalid(context, InvalidValueReason::OutOfRange))
    }
}

#[derive(Copy, Clone, Debug)]
/// Definition for a parallel joint.
pub struct ParallelJointDef {
    common: JointCommon,
    hertz: f32,
    damping_ratio: f32,
    max_torque: f32,
}

impl ParallelJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            hertz: 1.0,
            damping_ratio: 1.0,
            max_torque: f32::MAX,
        }
    }

    /// Sets spring frequency, damping ratio, and maximum torque.
    ///
    /// `hertz` is cycles per second, `damping_ratio` is dimensionless, and
    /// `max_torque` is typically in newton-meters.
    pub fn spring(mut self, hertz: f32, damping_ratio: f32, max_torque: f32) -> Self {
        self.hertz = hertz;
        self.damping_ratio = damping_ratio;
        self.max_torque = max_torque;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()?;
        validation::nonnegative("parallel_joint.hertz", self.hertz)?;
        validation::nonnegative("parallel_joint.damping_ratio", self.damping_ratio)?;
        validation::nonnegative("parallel_joint.max_torque", self.max_torque)
    }

    pub(super) fn lower(&self) -> ffi::b3ParallelJointDef {
        let mut raw = unsafe { ffi::b3DefaultParallelJointDef() };
        self.common.lower(&mut raw.base);
        raw.hertz = self.hertz;
        raw.dampingRatio = self.damping_ratio;
        raw.maxTorque = self.max_torque;
        raw
    }
}

impl_joint_common_api!(ParallelJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a distance joint.
pub struct DistanceJointDef {
    common: JointCommon,
    length: f32,
    enable_spring: bool,
    lower_spring_force: f32,
    upper_spring_force: f32,
    hertz: f32,
    damping_ratio: f32,
    enable_limit: bool,
    min_length: f32,
    max_length: Option<f32>,
    enable_motor: bool,
    max_motor_force: f32,
    motor_speed: f32,
}

impl DistanceJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            length: 1.0,
            enable_spring: false,
            lower_spring_force: -f32::MAX,
            upper_spring_force: f32::MAX,
            hertz: 0.0,
            damping_ratio: 0.0,
            enable_limit: false,
            min_length: 0.0,
            max_length: None,
            enable_motor: false,
            max_motor_force: 0.0,
            motor_speed: 0.0,
        }
    }

    /// Sets the positive rest length in world length units.
    pub fn length(mut self, length: f32) -> Self {
        self.length = length;
        self
    }

    /// Enables or disables spring behavior and sets frequency in hertz and a
    /// dimensionless damping ratio.
    pub fn spring(mut self, enabled: bool, hertz: f32, damping_ratio: f32) -> Self {
        self.enable_spring = enabled;
        self.hertz = hertz;
        self.damping_ratio = damping_ratio;
        self
    }

    /// Sets the ordered spring tension and compression force range, typically in newtons.
    pub fn spring_force_range(mut self, lower: f32, upper: f32) -> Self {
        self.lower_spring_force = lower;
        self.upper_spring_force = upper;
        self
    }

    /// Enables or disables length limits in world length units; `min_length`
    /// must not exceed `max_length`.
    pub fn limit(mut self, enabled: bool, min_length: f32, max_length: f32) -> Self {
        self.enable_limit = enabled;
        self.min_length = min_length;
        self.max_length = Some(max_length);
        self
    }

    /// Enables or disables the motor. `speed` is world length units per second
    /// and `max_force` is typically in newtons.
    pub fn motor(mut self, enabled: bool, speed: f32, max_force: f32) -> Self {
        self.enable_motor = enabled;
        self.motor_speed = speed;
        self.max_motor_force = max_force;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()?;
        validation::positive("distance_joint.length", self.length)?;
        validation::ordered(
            "distance_joint.spring_force_range",
            self.lower_spring_force,
            self.upper_spring_force,
        )?;
        validation::nonnegative("distance_joint.hertz", self.hertz)?;
        validation::nonnegative("distance_joint.damping_ratio", self.damping_ratio)?;
        validation::nonnegative("distance_joint.min_length", self.min_length)?;
        if let Some(max_length) = self.max_length {
            validation::nonnegative("distance_joint.max_length", max_length)?;
            validation::ordered("distance_joint.length_range", self.min_length, max_length)?;
        }
        validation::nonnegative("distance_joint.max_motor_force", self.max_motor_force)?;
        validation::finite("distance_joint.motor_speed", self.motor_speed)
    }

    pub(super) fn lower(&self) -> ffi::b3DistanceJointDef {
        let mut raw = unsafe { ffi::b3DefaultDistanceJointDef() };
        self.common.lower(&mut raw.base);
        raw.length = self.length;
        raw.enableSpring = self.enable_spring;
        raw.lowerSpringForce = self.lower_spring_force;
        raw.upperSpringForce = self.upper_spring_force;
        raw.hertz = self.hertz;
        raw.dampingRatio = self.damping_ratio;
        raw.enableLimit = self.enable_limit;
        raw.minLength = self.min_length;
        if let Some(max_length) = self.max_length {
            raw.maxLength = max_length;
        }
        raw.enableMotor = self.enable_motor;
        raw.maxMotorForce = self.max_motor_force;
        raw.motorSpeed = self.motor_speed;
        raw
    }
}

impl_joint_common_api!(DistanceJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a motor joint.
pub struct MotorJointDef {
    common: JointCommon,
    linear_velocity: Vec3,
    angular_velocity: Vec3,
    max_velocity_force: f32,
    max_velocity_torque: f32,
    linear_hertz: f32,
    linear_damping_ratio: f32,
    max_spring_force: f32,
    angular_hertz: f32,
    angular_damping_ratio: f32,
    max_spring_torque: f32,
}

impl MotorJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            max_velocity_force: 0.0,
            max_velocity_torque: 0.0,
            linear_hertz: 0.0,
            linear_damping_ratio: 0.0,
            max_spring_force: 0.0,
            angular_hertz: 0.0,
            angular_damping_ratio: 0.0,
            max_spring_torque: 0.0,
        }
    }

    /// Sets the target linear velocity in world length units per second.
    pub fn linear_velocity(mut self, velocity: impl Into<Vec3>) -> Self {
        self.linear_velocity = velocity.into();
        self
    }

    /// Sets the target angular velocity in radians per second around each axis.
    pub fn angular_velocity(mut self, velocity: impl Into<Vec3>) -> Self {
        self.angular_velocity = velocity.into();
        self
    }

    /// Sets the maximum velocity-control force, typically in newtons.
    pub fn max_velocity_force(mut self, force: f32) -> Self {
        self.max_velocity_force = force;
        self
    }

    /// Sets the maximum velocity-control torque, typically in newton-meters.
    pub fn max_velocity_torque(mut self, torque: f32) -> Self {
        self.max_velocity_torque = torque;
        self
    }

    /// Sets linear spring frequency in hertz, dimensionless damping, and
    /// maximum force, typically in newtons.
    pub fn linear_spring(mut self, hertz: f32, damping_ratio: f32, max_force: f32) -> Self {
        self.linear_hertz = hertz;
        self.linear_damping_ratio = damping_ratio;
        self.max_spring_force = max_force;
        self
    }

    /// Sets angular spring frequency in hertz, dimensionless damping, and
    /// maximum torque, typically in newton-meters.
    pub fn angular_spring(mut self, hertz: f32, damping_ratio: f32, max_torque: f32) -> Self {
        self.angular_hertz = hertz;
        self.angular_damping_ratio = damping_ratio;
        self.max_spring_torque = max_torque;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()?;
        validation::vec3("motor_joint.linear_velocity", self.linear_velocity)?;
        validation::vec3("motor_joint.angular_velocity", self.angular_velocity)?;
        validation::nonnegative("motor_joint.max_velocity_force", self.max_velocity_force)?;
        validation::nonnegative("motor_joint.max_velocity_torque", self.max_velocity_torque)?;
        validation::nonnegative("motor_joint.linear_hertz", self.linear_hertz)?;
        validation::nonnegative(
            "motor_joint.linear_damping_ratio",
            self.linear_damping_ratio,
        )?;
        validation::nonnegative("motor_joint.max_spring_force", self.max_spring_force)?;
        validation::nonnegative("motor_joint.angular_hertz", self.angular_hertz)?;
        validation::nonnegative(
            "motor_joint.angular_damping_ratio",
            self.angular_damping_ratio,
        )?;
        validation::nonnegative("motor_joint.max_spring_torque", self.max_spring_torque)
    }

    pub(super) fn lower(&self) -> ffi::b3MotorJointDef {
        let mut raw = unsafe { ffi::b3DefaultMotorJointDef() };
        self.common.lower(&mut raw.base);
        raw.linearVelocity = self.linear_velocity.into_raw();
        raw.angularVelocity = self.angular_velocity.into_raw();
        raw.maxVelocityForce = self.max_velocity_force;
        raw.maxVelocityTorque = self.max_velocity_torque;
        raw.linearHertz = self.linear_hertz;
        raw.linearDampingRatio = self.linear_damping_ratio;
        raw.maxSpringForce = self.max_spring_force;
        raw.angularHertz = self.angular_hertz;
        raw.angularDampingRatio = self.angular_damping_ratio;
        raw.maxSpringTorque = self.max_spring_torque;
        raw
    }
}

impl_joint_common_api!(MotorJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a collision-filter joint.
pub struct FilterJointDef {
    common: JointCommon,
}

impl FilterJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
        }
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()
    }

    pub(super) fn lower(&self) -> ffi::b3FilterJointDef {
        let mut raw = unsafe { ffi::b3DefaultFilterJointDef() };
        self.common.lower(&mut raw.base);
        raw
    }
}

impl_joint_common_api!(FilterJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a prismatic joint.
pub struct PrismaticJointDef {
    common: JointCommon,
    enable_spring: bool,
    hertz: f32,
    damping_ratio: f32,
    target_translation: f32,
    enable_limit: bool,
    lower_translation: f32,
    upper_translation: f32,
    enable_motor: bool,
    max_motor_force: f32,
    motor_speed: f32,
}

impl PrismaticJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            enable_spring: false,
            hertz: 0.0,
            damping_ratio: 0.0,
            target_translation: 0.0,
            enable_limit: false,
            lower_translation: 0.0,
            upper_translation: 0.0,
            enable_motor: false,
            max_motor_force: 0.0,
            motor_speed: 0.0,
        }
    }

    /// Enables or disables the spring and sets frequency in hertz and a
    /// dimensionless damping ratio.
    pub fn spring(mut self, enabled: bool, hertz: f32, damping_ratio: f32) -> Self {
        self.enable_spring = enabled;
        self.hertz = hertz;
        self.damping_ratio = damping_ratio;
        self
    }

    /// Sets the target translation in world length units.
    pub fn target_translation(mut self, target: f32) -> Self {
        self.target_translation = target;
        self
    }

    /// Enables or disables translation limits in world length units; `lower`
    /// must not exceed `upper`.
    pub fn limit(mut self, enabled: bool, lower: f32, upper: f32) -> Self {
        self.enable_limit = enabled;
        self.lower_translation = lower;
        self.upper_translation = upper;
        self
    }

    /// Enables or disables the motor. `speed` is world length units per second
    /// and `max_force` is typically in newtons.
    pub fn motor(mut self, enabled: bool, speed: f32, max_force: f32) -> Self {
        self.enable_motor = enabled;
        self.motor_speed = speed;
        self.max_motor_force = max_force;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()?;
        validation::nonnegative("prismatic_joint.hertz", self.hertz)?;
        validation::nonnegative("prismatic_joint.damping_ratio", self.damping_ratio)?;
        validation::finite(
            "prismatic_joint.target_translation",
            self.target_translation,
        )?;
        validation::ordered(
            "prismatic_joint.translation_range",
            self.lower_translation,
            self.upper_translation,
        )?;
        validation::nonnegative("prismatic_joint.max_motor_force", self.max_motor_force)?;
        validation::finite("prismatic_joint.motor_speed", self.motor_speed)
    }

    pub(super) fn lower(&self) -> ffi::b3PrismaticJointDef {
        let mut raw = unsafe { ffi::b3DefaultPrismaticJointDef() };
        self.common.lower(&mut raw.base);
        raw.enableSpring = self.enable_spring;
        raw.hertz = self.hertz;
        raw.dampingRatio = self.damping_ratio;
        raw.targetTranslation = self.target_translation;
        raw.enableLimit = self.enable_limit;
        raw.lowerTranslation = self.lower_translation;
        raw.upperTranslation = self.upper_translation;
        raw.enableMotor = self.enable_motor;
        raw.maxMotorForce = self.max_motor_force;
        raw.motorSpeed = self.motor_speed;
        raw
    }
}

impl_joint_common_api!(PrismaticJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a revolute joint.
pub struct RevoluteJointDef {
    common: JointCommon,
    target_angle: f32,
    enable_spring: bool,
    hertz: f32,
    damping_ratio: f32,
    enable_limit: bool,
    lower_angle: f32,
    upper_angle: f32,
    enable_motor: bool,
    max_motor_torque: f32,
    motor_speed: f32,
}

impl RevoluteJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            target_angle: 0.0,
            enable_spring: false,
            hertz: 0.0,
            damping_ratio: 0.0,
            enable_limit: false,
            lower_angle: 0.0,
            upper_angle: 0.0,
            enable_motor: false,
            max_motor_torque: 0.0,
            motor_speed: 0.0,
        }
    }

    /// Enables or disables the spring and sets frequency in hertz and a
    /// dimensionless damping ratio.
    pub fn spring(mut self, enabled: bool, hertz: f32, damping_ratio: f32) -> Self {
        self.enable_spring = enabled;
        self.hertz = hertz;
        self.damping_ratio = damping_ratio;
        self
    }

    /// Sets the target angle in radians. Values outside `[-pi, pi]` are rejected
    /// instead of being silently clamped by Box3D.
    pub fn target_angle(mut self, target_angle: f32) -> Self {
        self.target_angle = target_angle;
        self
    }

    /// Enables or disables an ordered angular range in radians. Both limits
    /// must lie within `+/-0.99 * pi`.
    pub fn limit(mut self, enabled: bool, lower: f32, upper: f32) -> Self {
        self.enable_limit = enabled;
        self.lower_angle = lower;
        self.upper_angle = upper;
        self
    }

    /// Enables or disables the motor. `speed` is radians per second and
    /// `max_torque` is typically in newton-meters.
    pub fn motor(mut self, enabled: bool, speed: f32, max_torque: f32) -> Self {
        self.enable_motor = enabled;
        self.motor_speed = speed;
        self.max_motor_torque = max_torque;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        const LIMIT: f32 = 0.99 * std::f32::consts::PI;
        self.common.validate()?;
        bounded(
            "revolute_joint.target_angle",
            self.target_angle,
            -std::f32::consts::PI,
            std::f32::consts::PI,
        )?;
        validation::nonnegative("revolute_joint.hertz", self.hertz)?;
        validation::nonnegative("revolute_joint.damping_ratio", self.damping_ratio)?;
        validation::ordered(
            "revolute_joint.angle_range",
            self.lower_angle,
            self.upper_angle,
        )?;
        bounded(
            "revolute_joint.lower_angle",
            self.lower_angle,
            -LIMIT,
            LIMIT,
        )?;
        bounded(
            "revolute_joint.upper_angle",
            self.upper_angle,
            -LIMIT,
            LIMIT,
        )?;
        validation::nonnegative("revolute_joint.max_motor_torque", self.max_motor_torque)?;
        validation::finite("revolute_joint.motor_speed", self.motor_speed)
    }

    pub(super) fn lower(&self) -> ffi::b3RevoluteJointDef {
        let mut raw = unsafe { ffi::b3DefaultRevoluteJointDef() };
        self.common.lower(&mut raw.base);
        raw.targetAngle = self.target_angle;
        raw.enableSpring = self.enable_spring;
        raw.hertz = self.hertz;
        raw.dampingRatio = self.damping_ratio;
        raw.enableLimit = self.enable_limit;
        raw.lowerAngle = self.lower_angle;
        raw.upperAngle = self.upper_angle;
        raw.enableMotor = self.enable_motor;
        raw.maxMotorTorque = self.max_motor_torque;
        raw.motorSpeed = self.motor_speed;
        raw
    }
}

impl_joint_common_api!(RevoluteJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a spherical joint.
pub struct SphericalJointDef {
    common: JointCommon,
    enable_spring: bool,
    hertz: f32,
    damping_ratio: f32,
    target_rotation: Quat,
    enable_cone_limit: bool,
    cone_angle: f32,
    enable_twist_limit: bool,
    lower_twist_angle: f32,
    upper_twist_angle: f32,
    enable_motor: bool,
    max_motor_torque: f32,
    motor_velocity: Vec3,
}

impl SphericalJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            enable_spring: false,
            hertz: 0.0,
            damping_ratio: 0.0,
            target_rotation: Quat::IDENTITY,
            enable_cone_limit: false,
            cone_angle: 0.0,
            enable_twist_limit: false,
            lower_twist_angle: 0.0,
            upper_twist_angle: 0.0,
            enable_motor: false,
            max_motor_torque: 0.0,
            motor_velocity: Vec3::ZERO,
        }
    }

    /// Enables or disables the rotational spring and sets frequency in hertz
    /// and a dimensionless damping ratio.
    pub fn spring(mut self, enabled: bool, hertz: f32, damping_ratio: f32) -> Self {
        self.enable_spring = enabled;
        self.hertz = hertz;
        self.damping_ratio = damping_ratio;
        self
    }

    /// Sets the target relative rotation.
    pub fn target_rotation(mut self, rotation: Quat) -> Self {
        self.target_rotation = rotation;
        self
    }

    /// Enables or disables the cone limit in radians. Angles above `pi / 2`
    /// are rejected instead of being silently clamped by Box3D.
    pub fn cone_limit(mut self, enabled: bool, angle: f32) -> Self {
        self.enable_cone_limit = enabled;
        self.cone_angle = angle;
        self
    }

    /// Enables or disables an ordered twist range in radians. Both limits must
    /// lie within `+/-0.99 * pi`.
    pub fn twist_limit(mut self, enabled: bool, lower: f32, upper: f32) -> Self {
        self.enable_twist_limit = enabled;
        self.lower_twist_angle = lower;
        self.upper_twist_angle = upper;
        self
    }

    /// Enables or disables the motor. `velocity` is radians per second around
    /// each axis and `max_torque` is typically in newton-meters.
    pub fn motor(mut self, enabled: bool, velocity: impl Into<Vec3>, max_torque: f32) -> Self {
        self.enable_motor = enabled;
        self.motor_velocity = velocity.into();
        self.max_motor_torque = max_torque;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        const TWIST_LIMIT: f32 = 0.99 * std::f32::consts::PI;
        self.common.validate()?;
        validation::nonnegative("spherical_joint.hertz", self.hertz)?;
        validation::nonnegative("spherical_joint.damping_ratio", self.damping_ratio)?;
        validation::quaternion("spherical_joint.target_rotation", self.target_rotation)?;
        bounded(
            "spherical_joint.cone_angle",
            self.cone_angle,
            0.0,
            std::f32::consts::FRAC_PI_2,
        )?;
        validation::ordered(
            "spherical_joint.twist_range",
            self.lower_twist_angle,
            self.upper_twist_angle,
        )?;
        bounded(
            "spherical_joint.lower_twist_angle",
            self.lower_twist_angle,
            -TWIST_LIMIT,
            TWIST_LIMIT,
        )?;
        bounded(
            "spherical_joint.upper_twist_angle",
            self.upper_twist_angle,
            -TWIST_LIMIT,
            TWIST_LIMIT,
        )?;
        validation::nonnegative("spherical_joint.max_motor_torque", self.max_motor_torque)?;
        validation::vec3("spherical_joint.motor_velocity", self.motor_velocity)
    }

    pub(super) fn lower(&self) -> ffi::b3SphericalJointDef {
        let mut raw = unsafe { ffi::b3DefaultSphericalJointDef() };
        self.common.lower(&mut raw.base);
        raw.enableSpring = self.enable_spring;
        raw.hertz = self.hertz;
        raw.dampingRatio = self.damping_ratio;
        raw.targetRotation = self.target_rotation.into_raw();
        raw.enableConeLimit = self.enable_cone_limit;
        raw.coneAngle = self.cone_angle;
        raw.enableTwistLimit = self.enable_twist_limit;
        raw.lowerTwistAngle = self.lower_twist_angle;
        raw.upperTwistAngle = self.upper_twist_angle;
        raw.enableMotor = self.enable_motor;
        raw.maxMotorTorque = self.max_motor_torque;
        raw.motorVelocity = self.motor_velocity.into_raw();
        raw
    }
}

impl_joint_common_api!(SphericalJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a weld joint.
pub struct WeldJointDef {
    common: JointCommon,
    linear_hertz: f32,
    angular_hertz: f32,
    linear_damping_ratio: f32,
    angular_damping_ratio: f32,
}

impl WeldJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            linear_hertz: 0.0,
            angular_hertz: 0.0,
            linear_damping_ratio: 0.0,
            angular_damping_ratio: 0.0,
        }
    }

    /// Sets linear weld frequency in hertz and a dimensionless damping ratio.
    /// Zero hertz requests maximum stiffness.
    pub fn linear_tuning(mut self, hertz: f32, damping_ratio: f32) -> Self {
        self.linear_hertz = hertz;
        self.linear_damping_ratio = damping_ratio;
        self
    }

    /// Sets angular weld frequency in hertz and a dimensionless damping ratio.
    /// Zero hertz requests maximum stiffness.
    pub fn angular_tuning(mut self, hertz: f32, damping_ratio: f32) -> Self {
        self.angular_hertz = hertz;
        self.angular_damping_ratio = damping_ratio;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()?;
        validation::nonnegative("weld_joint.linear_hertz", self.linear_hertz)?;
        validation::nonnegative("weld_joint.angular_hertz", self.angular_hertz)?;
        validation::nonnegative("weld_joint.linear_damping_ratio", self.linear_damping_ratio)?;
        validation::nonnegative(
            "weld_joint.angular_damping_ratio",
            self.angular_damping_ratio,
        )
    }

    pub(super) fn lower(&self) -> ffi::b3WeldJointDef {
        let mut raw = unsafe { ffi::b3DefaultWeldJointDef() };
        self.common.lower(&mut raw.base);
        raw.linearHertz = self.linear_hertz;
        raw.angularHertz = self.angular_hertz;
        raw.linearDampingRatio = self.linear_damping_ratio;
        raw.angularDampingRatio = self.angular_damping_ratio;
        raw
    }
}

impl_joint_common_api!(WeldJointDef);

#[derive(Copy, Clone, Debug)]
/// Definition for a wheel joint.
pub struct WheelJointDef {
    common: JointCommon,
    enable_suspension_spring: bool,
    suspension_hertz: f32,
    suspension_damping_ratio: f32,
    enable_suspension_limit: bool,
    lower_suspension_limit: f32,
    upper_suspension_limit: f32,
    enable_spin_motor: bool,
    max_spin_torque: f32,
    spin_speed: f32,
    enable_steering: bool,
    steering_hertz: f32,
    steering_damping_ratio: f32,
    target_steering_angle: f32,
    max_steering_torque: f32,
    enable_steering_limit: bool,
    lower_steering_limit: f32,
    upper_steering_limit: f32,
}

impl WheelJointDef {
    /// Creates a definition for two bodies.
    pub const fn new(body_a: BodyId, body_b: BodyId) -> Self {
        Self {
            common: JointCommon::new(body_a, body_b),
            enable_suspension_spring: true,
            suspension_hertz: 1.0,
            suspension_damping_ratio: 0.7,
            enable_suspension_limit: false,
            lower_suspension_limit: 0.0,
            upper_suspension_limit: 0.0,
            enable_spin_motor: false,
            max_spin_torque: 0.0,
            spin_speed: 0.0,
            enable_steering: false,
            steering_hertz: 1.0,
            steering_damping_ratio: 0.7,
            target_steering_angle: 0.0,
            max_steering_torque: 0.0,
            enable_steering_limit: false,
            lower_steering_limit: 0.0,
            upper_steering_limit: 0.0,
        }
    }

    /// Enables or disables suspension and sets frequency in hertz and a
    /// dimensionless damping ratio.
    pub fn suspension(mut self, enabled: bool, hertz: f32, damping_ratio: f32) -> Self {
        self.enable_suspension_spring = enabled;
        self.suspension_hertz = hertz;
        self.suspension_damping_ratio = damping_ratio;
        self
    }

    /// Enables or disables an ordered suspension range in world length units.
    pub fn suspension_limit(mut self, enabled: bool, lower: f32, upper: f32) -> Self {
        self.enable_suspension_limit = enabled;
        self.lower_suspension_limit = lower;
        self.upper_suspension_limit = upper;
        self
    }

    /// Enables or disables the spin motor. `speed` is radians per second and
    /// `max_torque` is typically in newton-meters.
    pub fn spin_motor(mut self, enabled: bool, speed: f32, max_torque: f32) -> Self {
        self.enable_spin_motor = enabled;
        self.spin_speed = speed;
        self.max_spin_torque = max_torque;
        self
    }

    /// Enables or disables steering and sets frequency in hertz and a
    /// dimensionless damping ratio.
    pub fn steering(mut self, enabled: bool, hertz: f32, damping_ratio: f32) -> Self {
        self.enable_steering = enabled;
        self.steering_hertz = hertz;
        self.steering_damping_ratio = damping_ratio;
        self
    }

    /// Enables or disables an ordered steering range in radians.
    pub fn steering_limit(mut self, enabled: bool, lower: f32, upper: f32) -> Self {
        self.enable_steering_limit = enabled;
        self.lower_steering_limit = lower;
        self.upper_steering_limit = upper;
        self
    }

    /// Sets target steering angle in radians and maximum steering torque,
    /// typically in newton-meters.
    pub fn target_steering(mut self, angle: f32, max_torque: f32) -> Self {
        self.target_steering_angle = angle;
        self.max_steering_torque = max_torque;
        self
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.common.validate()?;
        validation::nonnegative("wheel_joint.suspension_hertz", self.suspension_hertz)?;
        validation::nonnegative(
            "wheel_joint.suspension_damping_ratio",
            self.suspension_damping_ratio,
        )?;
        validation::ordered(
            "wheel_joint.suspension_range",
            self.lower_suspension_limit,
            self.upper_suspension_limit,
        )?;
        validation::nonnegative("wheel_joint.max_spin_torque", self.max_spin_torque)?;
        validation::finite("wheel_joint.spin_speed", self.spin_speed)?;
        validation::nonnegative("wheel_joint.steering_hertz", self.steering_hertz)?;
        validation::nonnegative(
            "wheel_joint.steering_damping_ratio",
            self.steering_damping_ratio,
        )?;
        validation::finite(
            "wheel_joint.target_steering_angle",
            self.target_steering_angle,
        )?;
        validation::nonnegative("wheel_joint.max_steering_torque", self.max_steering_torque)?;
        validation::ordered(
            "wheel_joint.steering_range",
            self.lower_steering_limit,
            self.upper_steering_limit,
        )
    }

    pub(super) fn lower(&self) -> ffi::b3WheelJointDef {
        let mut raw = unsafe { ffi::b3DefaultWheelJointDef() };
        self.common.lower(&mut raw.base);
        raw.enableSuspensionSpring = self.enable_suspension_spring;
        raw.suspensionHertz = self.suspension_hertz;
        raw.suspensionDampingRatio = self.suspension_damping_ratio;
        raw.enableSuspensionLimit = self.enable_suspension_limit;
        raw.lowerSuspensionLimit = self.lower_suspension_limit;
        raw.upperSuspensionLimit = self.upper_suspension_limit;
        raw.enableSpinMotor = self.enable_spin_motor;
        raw.maxSpinTorque = self.max_spin_torque;
        raw.spinSpeed = self.spin_speed;
        raw.enableSteering = self.enable_steering;
        raw.steeringHertz = self.steering_hertz;
        raw.steeringDampingRatio = self.steering_damping_ratio;
        raw.targetSteeringAngle = self.target_steering_angle;
        raw.maxSteeringTorque = self.max_steering_torque;
        raw.enableSteeringLimit = self.enable_steering_limit;
        raw.lowerSteeringLimit = self.lower_steering_limit;
        raw.upperSteeringLimit = self.upper_steering_limit;
        raw
    }
}

impl_joint_common_api!(WheelJointDef);

pub(super) trait JointDefinition {
    fn validate_definition(&self) -> Result<()>;
    fn body_ids(&self) -> (BodyId, BodyId);
    fn joint_type(&self) -> JointType;
    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId;
}

impl JointDefinition for ParallelJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Parallel
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateParallelJoint(world, &raw) }
    }
}

impl JointDefinition for DistanceJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Distance
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateDistanceJoint(world, &raw) }
    }
}

impl JointDefinition for FilterJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Filter
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateFilterJoint(world, &raw) }
    }
}

impl JointDefinition for MotorJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Motor
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateMotorJoint(world, &raw) }
    }
}

impl JointDefinition for PrismaticJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Prismatic
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreatePrismaticJoint(world, &raw) }
    }
}

impl JointDefinition for RevoluteJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Revolute
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateRevoluteJoint(world, &raw) }
    }
}

impl JointDefinition for SphericalJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Spherical
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateSphericalJoint(world, &raw) }
    }
}

impl JointDefinition for WeldJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Weld
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateWeldJoint(world, &raw) }
    }
}

impl JointDefinition for WheelJointDef {
    fn validate_definition(&self) -> Result<()> {
        self.validate()
    }

    fn body_ids(&self) -> (BodyId, BodyId) {
        self.bodies()
    }

    fn joint_type(&self) -> JointType {
        JointType::Wheel
    }

    fn create_native(&self, world: ffi::b3WorldId) -> ffi::b3JointId {
        let raw = self.lower();
        unsafe { ffi::b3CreateWheelJoint(world, &raw) }
    }
}
