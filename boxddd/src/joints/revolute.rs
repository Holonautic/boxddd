use super::*;

impl World {
    /// Tries to enable or disable the spring on a revolute joint.
    ///
    /// All revolute-joint runtime methods return [`Error::InvalidValue`] with
    /// context `joint.type` when `joint_id` belongs to another joint family.
    /// Stale and foreign handles return [`Error::StaleHandle`] and
    /// [`Error::ForeignHandle`], respectively.
    pub fn enable_revolute_joint_spring(&mut self, joint_id: JointId, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_EnableSpring(joint_id.into_raw(), enabled) };
        })
    }

    /// Returns whether the spring is enabled on a revolute joint.
    pub fn revolute_joint_spring_enabled(&self, joint_id: JointId) -> Result<bool> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_IsSpringEnabled(joint_id.into_raw()) }
        })
    }

    /// Tries to set the spring frequency, in hertz, on a revolute joint.
    pub fn set_revolute_joint_spring_hertz(&mut self, joint_id: JointId, hertz: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("revolute_joint.hertz", hertz)?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_SetSpringHertz(joint_id.into_raw(), hertz) };
        })
    }

    /// Returns the spring frequency, in hertz, of a revolute joint.
    pub fn revolute_joint_spring_hertz(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetSpringHertz(joint_id.into_raw()) }
        })
    }

    /// Tries to set the dimensionless spring damping ratio on a revolute joint.
    pub fn set_revolute_joint_spring_damping_ratio(
        &mut self,
        joint_id: JointId,
        damping_ratio: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("revolute_joint.damping_ratio", damping_ratio)?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe {
                ffi::b3RevoluteJoint_SetSpringDampingRatio(joint_id.into_raw(), damping_ratio)
            };
        })
    }

    /// Returns the dimensionless spring damping ratio of a revolute joint.
    pub fn revolute_joint_spring_damping_ratio(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetSpringDampingRatio(joint_id.into_raw()) }
        })
    }

    /// Tries to set the target angle, in radians, on a revolute joint.
    pub fn set_revolute_joint_target_angle(
        &mut self,
        joint_id: JointId,
        target: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::finite("revolute_joint.target_angle", target)?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_SetTargetAngle(joint_id.into_raw(), target) };
        })
    }

    /// Returns the target angle, in radians, of a revolute joint.
    pub fn revolute_joint_target_angle(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetTargetAngle(joint_id.into_raw()) }
        })
    }

    /// Returns the current joint angle, in radians, of a revolute joint.
    pub fn revolute_joint_angle(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetAngle(joint_id.into_raw()) }
        })
    }

    /// Tries to enable or disable the limit on a revolute joint.
    pub fn enable_revolute_joint_limit(&mut self, joint_id: JointId, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_EnableLimit(joint_id.into_raw(), enabled) };
        })
    }

    /// Returns whether the limit is enabled on a revolute joint.
    pub fn revolute_joint_limit_enabled(&self, joint_id: JointId) -> Result<bool> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_IsLimitEnabled(joint_id.into_raw()) }
        })
    }

    /// Returns the lower angle limit, in radians, of a revolute joint.
    pub fn revolute_joint_lower_limit(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetLowerLimit(joint_id.into_raw()) }
        })
    }

    /// Returns the upper angle limit, in radians, of a revolute joint.
    pub fn revolute_joint_upper_limit(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetUpperLimit(joint_id.into_raw()) }
        })
    }

    /// Tries to set angle limits, in radians, on a revolute joint.
    pub fn set_revolute_joint_limits(
        &mut self,
        joint_id: JointId,
        lower: f32,
        upper: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::ordered("revolute_joint.angle_range", lower, upper)?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_SetLimits(joint_id.into_raw(), lower, upper) };
        })
    }

    /// Tries to enable or disable the motor on a revolute joint.
    pub fn enable_revolute_joint_motor(&mut self, joint_id: JointId, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_EnableMotor(joint_id.into_raw(), enabled) };
        })
    }

    /// Returns whether the motor is enabled on a revolute joint.
    pub fn revolute_joint_motor_enabled(&self, joint_id: JointId) -> Result<bool> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_IsMotorEnabled(joint_id.into_raw()) }
        })
    }

    /// Tries to set the motor speed, in radians per second, on a revolute joint.
    pub fn set_revolute_joint_motor_speed(&mut self, joint_id: JointId, speed: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::finite("revolute_joint.motor_speed", speed)?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_SetMotorSpeed(joint_id.into_raw(), speed) };
        })
    }

    /// Returns the motor speed, in radians per second, of a revolute joint.
    pub fn revolute_joint_motor_speed(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetMotorSpeed(joint_id.into_raw()) }
        })
    }

    /// Returns the current motor torque, in newton-meters, of a revolute joint.
    pub fn revolute_joint_motor_torque(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetMotorTorque(joint_id.into_raw()) }
        })
    }

    /// Tries to set the maximum motor torque, in newton-meters, on a revolute joint.
    pub fn set_revolute_joint_max_motor_torque(
        &mut self,
        joint_id: JointId,
        torque: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("revolute_joint.max_motor_torque", torque)?;
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_SetMaxMotorTorque(joint_id.into_raw(), torque) };
        })
    }

    /// Returns the maximum motor torque, in newton-meters, of a revolute joint.
    pub fn revolute_joint_max_motor_torque(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Revolute, {
            unsafe { ffi::b3RevoluteJoint_GetMaxMotorTorque(joint_id.into_raw()) }
        })
    }
}
