use super::*;

impl World {
    /// Tries to set the target linear velocity, in length units per second, on a motor joint.
    ///
    /// All motor-joint runtime methods return [`Error::InvalidValue`] with
    /// context `joint.type` when `joint_id` belongs to another joint family.
    /// Stale and foreign handles return [`Error::StaleHandle`] and
    /// [`Error::ForeignHandle`], respectively.
    pub fn set_motor_joint_linear_velocity(
        &mut self,
        joint_id: JointId,
        velocity: impl Into<Vec3>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let velocity = velocity.into();
        validation::vec3("motor_joint.linear_velocity", velocity)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe {
                ffi::b3MotorJoint_SetLinearVelocity(joint_id.into_raw(), velocity.into_raw())
            };
        })
    }

    /// Returns the target linear velocity, in length units per second, of a motor joint.
    pub fn motor_joint_linear_velocity(&self, joint_id: JointId) -> Result<Vec3> {
        family_method!(self, joint_id, JointType::Motor, {
            Vec3::from_raw(unsafe { ffi::b3MotorJoint_GetLinearVelocity(joint_id.into_raw()) })
        })
    }

    /// Tries to set the target angular velocity, in radians per second, on a motor joint.
    pub fn set_motor_joint_angular_velocity(
        &mut self,
        joint_id: JointId,
        velocity: impl Into<Vec3>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let velocity = velocity.into();
        validation::vec3("motor_joint.angular_velocity", velocity)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe {
                ffi::b3MotorJoint_SetAngularVelocity(joint_id.into_raw(), velocity.into_raw())
            };
        })
    }

    /// Returns the target angular velocity, in radians per second, of a motor joint.
    pub fn motor_joint_angular_velocity(&self, joint_id: JointId) -> Result<Vec3> {
        family_method!(self, joint_id, JointType::Motor, {
            Vec3::from_raw(unsafe { ffi::b3MotorJoint_GetAngularVelocity(joint_id.into_raw()) })
        })
    }

    /// Tries to set the maximum velocity force, in newtons, on a motor joint.
    pub fn set_motor_joint_max_velocity_force(
        &mut self,
        joint_id: JointId,
        force: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.max_velocity_force", force)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetMaxVelocityForce(joint_id.into_raw(), force) };
        })
    }

    /// Returns the maximum velocity force, in newtons, of a motor joint.
    pub fn motor_joint_max_velocity_force(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetMaxVelocityForce(joint_id.into_raw()) }
        })
    }

    /// Tries to set the maximum velocity torque, in newton-meters, on a motor joint.
    pub fn set_motor_joint_max_velocity_torque(
        &mut self,
        joint_id: JointId,
        torque: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.max_velocity_torque", torque)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetMaxVelocityTorque(joint_id.into_raw(), torque) };
        })
    }

    /// Returns the maximum velocity torque, in newton-meters, of a motor joint.
    pub fn motor_joint_max_velocity_torque(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetMaxVelocityTorque(joint_id.into_raw()) }
        })
    }

    /// Tries to set the linear spring frequency, in hertz, on a motor joint.
    pub fn set_motor_joint_linear_hertz(&mut self, joint_id: JointId, hertz: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.linear_hertz", hertz)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetLinearHertz(joint_id.into_raw(), hertz) };
        })
    }

    /// Returns the linear spring frequency, in hertz, of a motor joint.
    pub fn motor_joint_linear_hertz(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetLinearHertz(joint_id.into_raw()) }
        })
    }

    /// Tries to set the dimensionless linear damping ratio on a motor joint.
    pub fn set_motor_joint_linear_damping_ratio(
        &mut self,
        joint_id: JointId,
        damping: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.linear_damping_ratio", damping)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetLinearDampingRatio(joint_id.into_raw(), damping) };
        })
    }

    /// Returns the dimensionless linear damping ratio of a motor joint.
    pub fn motor_joint_linear_damping_ratio(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetLinearDampingRatio(joint_id.into_raw()) }
        })
    }

    /// Tries to set the angular spring frequency, in hertz, on a motor joint.
    pub fn set_motor_joint_angular_hertz(&mut self, joint_id: JointId, hertz: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.angular_hertz", hertz)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetAngularHertz(joint_id.into_raw(), hertz) };
        })
    }

    /// Returns the angular spring frequency, in hertz, of a motor joint.
    pub fn motor_joint_angular_hertz(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetAngularHertz(joint_id.into_raw()) }
        })
    }

    /// Tries to set the dimensionless angular damping ratio on a motor joint.
    pub fn set_motor_joint_angular_damping_ratio(
        &mut self,
        joint_id: JointId,
        damping: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.angular_damping_ratio", damping)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetAngularDampingRatio(joint_id.into_raw(), damping) };
        })
    }

    /// Returns the dimensionless angular damping ratio of a motor joint.
    pub fn motor_joint_angular_damping_ratio(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetAngularDampingRatio(joint_id.into_raw()) }
        })
    }

    /// Tries to set the maximum spring force, in newtons, on a motor joint.
    pub fn set_motor_joint_max_spring_force(
        &mut self,
        joint_id: JointId,
        force: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.max_spring_force", force)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetMaxSpringForce(joint_id.into_raw(), force) };
        })
    }

    /// Returns the maximum spring force, in newtons, of a motor joint.
    pub fn motor_joint_max_spring_force(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetMaxSpringForce(joint_id.into_raw()) }
        })
    }

    /// Tries to set the maximum spring torque, in newton-meters, on a motor joint.
    pub fn set_motor_joint_max_spring_torque(
        &mut self,
        joint_id: JointId,
        torque: f32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("motor_joint.max_spring_torque", torque)?;
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_SetMaxSpringTorque(joint_id.into_raw(), torque) };
        })
    }

    /// Returns the maximum spring torque, in newton-meters, of a motor joint.
    pub fn motor_joint_max_spring_torque(&self, joint_id: JointId) -> Result<f32> {
        family_method!(self, joint_id, JointType::Motor, {
            unsafe { ffi::b3MotorJoint_GetMaxSpringTorque(joint_id.into_raw()) }
        })
    }
}
