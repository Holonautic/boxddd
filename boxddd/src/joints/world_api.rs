use super::*;

impl World {
    /// Tries to create a parallel joint.
    ///
    /// Creation validates both body handles, confirms both bodies belong to
    /// this world, and rejects invalid scalar fields before calling Box3D.
    pub fn try_create_parallel_joint(&mut self, def: ParallelJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a parallel joint or panics if creation fails.
    pub fn create_parallel_joint(&mut self, def: ParallelJointDef) -> JointId {
        self.try_create_parallel_joint(def)
            .expect("Box3D failed to create parallel joint")
    }

    /// Tries to create a distance joint.
    ///
    /// Creation validates both body handles, confirms both bodies belong to
    /// this world, and rejects invalid scalar fields before calling Box3D.
    pub fn try_create_distance_joint(&mut self, def: DistanceJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a distance joint or panics if creation fails.
    pub fn create_distance_joint(&mut self, def: DistanceJointDef) -> JointId {
        self.try_create_distance_joint(def)
            .expect("Box3D failed to create distance joint")
    }

    /// Tries to create a motor joint.
    pub fn try_create_motor_joint(&mut self, def: MotorJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a motor joint or panics if creation fails.
    pub fn create_motor_joint(&mut self, def: MotorJointDef) -> JointId {
        self.try_create_motor_joint(def)
            .expect("Box3D failed to create motor joint")
    }

    /// Tries to create a filter joint.
    pub fn try_create_filter_joint(&mut self, def: FilterJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a filter joint or panics if creation fails.
    pub fn create_filter_joint(&mut self, def: FilterJointDef) -> JointId {
        self.try_create_filter_joint(def)
            .expect("Box3D failed to create filter joint")
    }

    /// Tries to create a prismatic joint.
    pub fn try_create_prismatic_joint(&mut self, def: PrismaticJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a prismatic joint or panics if creation fails.
    pub fn create_prismatic_joint(&mut self, def: PrismaticJointDef) -> JointId {
        self.try_create_prismatic_joint(def)
            .expect("Box3D failed to create prismatic joint")
    }

    /// Tries to create a revolute joint.
    pub fn try_create_revolute_joint(&mut self, def: RevoluteJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a revolute joint or panics if creation fails.
    pub fn create_revolute_joint(&mut self, def: RevoluteJointDef) -> JointId {
        self.try_create_revolute_joint(def)
            .expect("Box3D failed to create revolute joint")
    }

    /// Tries to create a spherical joint.
    pub fn try_create_spherical_joint(&mut self, def: SphericalJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a spherical joint or panics if creation fails.
    pub fn create_spherical_joint(&mut self, def: SphericalJointDef) -> JointId {
        self.try_create_spherical_joint(def)
            .expect("Box3D failed to create spherical joint")
    }

    /// Tries to create a weld joint.
    pub fn try_create_weld_joint(&mut self, def: WeldJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a weld joint or panics if creation fails.
    pub fn create_weld_joint(&mut self, def: WeldJointDef) -> JointId {
        self.try_create_weld_joint(def)
            .expect("Box3D failed to create weld joint")
    }

    /// Tries to create a wheel joint.
    pub fn try_create_wheel_joint(&mut self, def: WheelJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Creates a wheel joint or panics if creation fails.
    pub fn create_wheel_joint(&mut self, def: WheelJointDef) -> JointId {
        self.try_create_wheel_joint(def)
            .expect("Box3D failed to create wheel joint")
    }

    fn create_joint(&mut self, def: impl JointDefinition) -> Result<JointId> {
        def.validate_definition()?;
        let (body_a, body_b) = def.body_ids();
        callback_state::check_not_in_callback()?;
        let raw_body_a = self.state.ledger.authorize_body(body_a)?;
        let raw_body_b = self.state.ledger.authorize_body(body_b)?;
        check_joint_body_pair_valid(body_a, body_b)?;
        let pending = self.state.ledger.reserve_joint(body_a, body_b)?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        debug_checks::check_body_valid_raw(raw_body_a)?;
        debug_checks::check_body_valid_raw(raw_body_b)?;
        let raw = def.create_locked(self.raw());
        if unsafe { ffi::b3Joint_IsValid(raw) } {
            Ok(self.state.ledger.publish_joint(raw, pending))
        } else {
            Err(Error::NativeFailure)
        }
    }

    /// Tries to destroy a joint.
    ///
    /// `wake_attached` forwards Box3D's option to wake the two connected bodies.
    pub fn try_destroy_joint(&mut self, joint_id: JointId, wake_attached: bool) -> Result<()> {
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3DestroyJoint(joint_id.into_raw(), wake_attached) };
        drop(_guard);
        self.state.ledger.retire_joint(joint_id);
        Ok(())
    }

    /// Destroys a joint or panics if the handle is invalid.
    pub fn destroy_joint(&mut self, joint_id: JointId, wake_attached: bool) {
        self.try_destroy_joint(joint_id, wake_attached)
            .expect("invalid JointId");
    }

    /// Returns the type of a joint.
    pub fn try_joint_type(&self, joint_id: JointId) -> Result<JointType> {
        let _guard = lock_joint_checked(self, joint_id)?;
        JointType::from_raw(unsafe { ffi::b3Joint_GetType(joint_id.into_raw()) })
            .ok_or(Error::WrongJointType)
    }

    /// Returns the joint type or panics if the handle is invalid.
    pub fn joint_type(&self, joint_id: JointId) -> JointType {
        self.try_joint_type(joint_id).expect("invalid JointId")
    }

    /// Returns body A attached to a joint.
    pub fn try_joint_body_a(&self, joint_id: JointId) -> Result<BodyId> {
        let _guard = lock_joint_checked(self, joint_id)?;
        let raw = unsafe { ffi::b3Joint_GetBodyA(joint_id.into_raw()) };
        self.state.ledger.resolve_body(raw)
    }

    /// Returns body B attached to a joint.
    pub fn try_joint_body_b(&self, joint_id: JointId) -> Result<BodyId> {
        let _guard = lock_joint_checked(self, joint_id)?;
        let raw = unsafe { ffi::b3Joint_GetBodyB(joint_id.into_raw()) };
        self.state.ledger.resolve_body(raw)
    }

    /// Tries to set local frame A on a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn try_set_joint_local_frame_a(
        &mut self,
        joint_id: JointId,
        frame: Transform,
    ) -> Result<()> {
        frame.validate()?;
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3Joint_SetLocalFrameA(joint_id.into_raw(), frame.into_raw()) };
        Ok(())
    }

    /// Returns local frame A from a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn try_joint_local_frame_a(&self, joint_id: JointId) -> Result<Transform> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(Transform::from_raw(unsafe {
            ffi::b3Joint_GetLocalFrameA(joint_id.into_raw())
        }))
    }

    /// Tries to set local frame B on a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn try_set_joint_local_frame_b(
        &mut self,
        joint_id: JointId,
        frame: Transform,
    ) -> Result<()> {
        frame.validate()?;
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3Joint_SetLocalFrameB(joint_id.into_raw(), frame.into_raw()) };
        Ok(())
    }

    /// Returns local frame B from a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn try_joint_local_frame_b(&self, joint_id: JointId) -> Result<Transform> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(Transform::from_raw(unsafe {
            ffi::b3Joint_GetLocalFrameB(joint_id.into_raw())
        }))
    }

    /// Tries to set whether attached bodies may collide.
    pub fn try_set_joint_collide_connected(
        &mut self,
        joint_id: JointId,
        collide: bool,
    ) -> Result<()> {
        let next_contact_epoch = self.state.ledger.prepare_contact_turnover()?;
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3Joint_SetCollideConnected(joint_id.into_raw(), collide) };
        self.finish_contact_turnover_locked(next_contact_epoch);
        drop(_guard);
        Ok(())
    }

    /// Returns whether attached bodies may collide.
    pub fn try_joint_collide_connected(&self, joint_id: JointId) -> Result<bool> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetCollideConnected(joint_id.into_raw()) })
    }

    /// Tries to wake the bodies attached to a joint.
    pub fn try_wake_joint_bodies(&mut self, joint_id: JointId) -> Result<()> {
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3Joint_WakeBodies(joint_id.into_raw()) };
        Ok(())
    }

    /// Returns the constraint force, in newtons, of a joint.
    pub fn try_joint_constraint_force(&self, joint_id: JointId) -> Result<Vec3> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Joint_GetConstraintForce(joint_id.into_raw())
        }))
    }

    /// Returns the constraint torque, in newton-meters, of a joint.
    pub fn try_joint_constraint_torque(&self, joint_id: JointId) -> Result<Vec3> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Joint_GetConstraintTorque(joint_id.into_raw())
        }))
    }

    /// Returns the linear separation, in world length units, of a joint.
    pub fn try_joint_linear_separation(&self, joint_id: JointId) -> Result<f32> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetLinearSeparation(joint_id.into_raw()) })
    }

    /// Returns the angular separation, in radians, of a joint.
    pub fn try_joint_angular_separation(&self, joint_id: JointId) -> Result<f32> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetAngularSeparation(joint_id.into_raw()) })
    }

    /// Tries to set generic constraint tuning on a joint.
    ///
    /// `JointTuning::hertz` is cycles per second and
    /// `JointTuning::damping_ratio` is dimensionless.
    pub fn try_set_joint_constraint_tuning(
        &mut self,
        joint_id: JointId,
        tuning: JointTuning,
    ) -> Result<()> {
        tuning.validate()?;
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe {
            ffi::b3Joint_SetConstraintTuning(
                joint_id.into_raw(),
                tuning.hertz,
                tuning.damping_ratio,
            )
        };
        Ok(())
    }

    /// Returns the constraint tuning of a joint.
    pub fn try_joint_constraint_tuning(&self, joint_id: JointId) -> Result<JointTuning> {
        let _guard = lock_joint_checked(self, joint_id)?;
        let mut hertz = 0.0;
        let mut damping_ratio = 0.0;
        unsafe {
            ffi::b3Joint_GetConstraintTuning(joint_id.into_raw(), &mut hertz, &mut damping_ratio)
        };
        Ok(JointTuning::new(hertz, damping_ratio))
    }

    /// Tries to set the force threshold, in newtons, for joint events.
    pub fn try_set_joint_force_threshold(
        &mut self,
        joint_id: JointId,
        threshold: f32,
    ) -> Result<()> {
        validate_nonnegative_scalar(threshold)?;
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3Joint_SetForceThreshold(joint_id.into_raw(), threshold) };
        Ok(())
    }

    /// Returns the force threshold, in newtons, for joint events.
    pub fn try_joint_force_threshold(&self, joint_id: JointId) -> Result<f32> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetForceThreshold(joint_id.into_raw()) })
    }

    /// Tries to set the torque threshold, in newton-meters, for joint events.
    pub fn try_set_joint_torque_threshold(
        &mut self,
        joint_id: JointId,
        threshold: f32,
    ) -> Result<()> {
        validate_nonnegative_scalar(threshold)?;
        let _guard = lock_joint_checked(self, joint_id)?;
        unsafe { ffi::b3Joint_SetTorqueThreshold(joint_id.into_raw(), threshold) };
        Ok(())
    }

    /// Returns the torque threshold, in newton-meters, for joint events.
    pub fn try_joint_torque_threshold(&self, joint_id: JointId) -> Result<f32> {
        let _guard = lock_joint_checked(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetTorqueThreshold(joint_id.into_raw()) })
    }
}
