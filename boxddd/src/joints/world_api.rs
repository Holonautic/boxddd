use super::*;
use crate::world::creation_transaction::{
    JointNative, NativeCreationInput, finish_native_creation, observed_joint_endpoints,
};

impl World {
    /// Tries to create a parallel joint.
    ///
    /// Creation validates both body handles, confirms both bodies belong to
    /// this world, and rejects invalid scalar fields before calling Box3D.
    pub fn create_parallel_joint(&mut self, def: ParallelJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a distance joint.
    ///
    /// Creation validates both body handles, confirms both bodies belong to
    /// this world, and rejects invalid scalar fields before calling Box3D.
    pub fn create_distance_joint(&mut self, def: DistanceJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a motor joint.
    pub fn create_motor_joint(&mut self, def: MotorJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a filter joint.
    pub fn create_filter_joint(&mut self, def: FilterJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a prismatic joint.
    pub fn create_prismatic_joint(&mut self, def: PrismaticJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a revolute joint.
    pub fn create_revolute_joint(&mut self, def: RevoluteJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a spherical joint.
    pub fn create_spherical_joint(&mut self, def: SphericalJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a weld joint.
    pub fn create_weld_joint(&mut self, def: WeldJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    /// Tries to create a wheel joint.
    pub fn create_wheel_joint(&mut self, def: WheelJointDef) -> Result<JointId> {
        self.create_joint(def)
    }

    fn create_joint(&mut self, def: impl JointDefinition) -> Result<JointId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        def.validate_definition()?;
        let (body_a, body_b) = def.body_ids();
        let expected_type = def.joint_type();
        let raw_body_a = self.state().ledger.authorize_body(body_a)?;
        let raw_body_b = self.state().ledger.authorize_body(body_b)?;
        check_joint_body_pair_valid(body_a, body_b)?;
        let pending = self.state_mut().ledger.reserve_joint(body_a, body_b)?;
        let _call = self.enter_world_call()?;
        debug_checks::check_body_valid_raw(raw_body_a)?;
        debug_checks::check_body_valid_raw(raw_body_b)?;
        let raw = def.create_native(self.raw());
        let target_world = self.raw();
        let crate::world::WorldState {
            poisoned, ledger, ..
        } = self.state_mut();
        let identity = ledger.classify_joint(raw);
        finish_native_creation::<JointNative, _, _, _>(
            NativeCreationInput::new(raw, (), target_world, poisoned, identity),
            ledger,
            |ledger, raw, available| {
                let (native_body_a, native_body_b) = observed_joint_endpoints(raw);
                ledger.validate_joint_binding(&pending, native_body_a, native_body_b)?;
                ledger.bind_joint(raw, available, pending)
            },
            |_, raw| {
                if JointType::from_raw(unsafe { ffi::b3Joint_GetType(raw) }) == Some(expected_type)
                {
                    Ok(())
                } else {
                    Err(Error::NativeFailure)
                }
            },
            |ledger, _, bound| ledger.publish_joint(bound),
        )
    }

    /// Tries to destroy a joint.
    ///
    /// `wake_attached` forwards Box3D's option to wake the two connected bodies.
    pub fn destroy_joint(&mut self, joint_id: JointId, wake_attached: bool) -> Result<()> {
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3DestroyJoint(joint_id.into_raw(), wake_attached) };
        drop(_call);
        self.state_mut().ledger.retire_joint(joint_id);
        Ok(())
    }

    /// Returns the type of a joint.
    pub fn joint_type(&self, joint_id: JointId) -> Result<JointType> {
        let _call = enter_joint_call(self, joint_id)?;
        JointType::from_raw(unsafe { ffi::b3Joint_GetType(joint_id.into_raw()) })
            .ok_or(Error::NativeFailure)
    }

    /// Returns body A attached to a joint.
    pub fn joint_body_a(&self, joint_id: JointId) -> Result<BodyId> {
        let _call = enter_joint_call(self, joint_id)?;
        let raw = unsafe { ffi::b3Joint_GetBodyA(joint_id.into_raw()) };
        self.state().ledger.resolve_body(raw)
    }

    /// Returns body B attached to a joint.
    pub fn joint_body_b(&self, joint_id: JointId) -> Result<BodyId> {
        let _call = enter_joint_call(self, joint_id)?;
        let raw = unsafe { ffi::b3Joint_GetBodyB(joint_id.into_raw()) };
        self.state().ledger.resolve_body(raw)
    }

    /// Tries to set local frame A on a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn set_joint_local_frame_a(&mut self, joint_id: JointId, frame: Transform) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::transform("joint.local_frame_a", frame)?;
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3Joint_SetLocalFrameA(joint_id.into_raw(), frame.into_raw()) };
        Ok(())
    }

    /// Returns local frame A from a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn joint_local_frame_a(&self, joint_id: JointId) -> Result<Transform> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(Transform::from_raw(unsafe {
            ffi::b3Joint_GetLocalFrameA(joint_id.into_raw())
        }))
    }

    /// Tries to set local frame B on a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn set_joint_local_frame_b(&mut self, joint_id: JointId, frame: Transform) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::transform("joint.local_frame_b", frame)?;
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3Joint_SetLocalFrameB(joint_id.into_raw(), frame.into_raw()) };
        Ok(())
    }

    /// Returns local frame B from a joint.
    ///
    /// Joint local frames are measured from each body's origin.
    pub fn joint_local_frame_b(&self, joint_id: JointId) -> Result<Transform> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(Transform::from_raw(unsafe {
            ffi::b3Joint_GetLocalFrameB(joint_id.into_raw())
        }))
    }

    /// Tries to set whether attached bodies may collide.
    pub fn set_joint_collide_connected(&mut self, joint_id: JointId, collide: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3Joint_SetCollideConnected(joint_id.into_raw(), collide) };
        self.finish_contact_turnover(next_contact_epoch);
        drop(_call);
        Ok(())
    }

    /// Returns whether attached bodies may collide.
    pub fn joint_collide_connected(&self, joint_id: JointId) -> Result<bool> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetCollideConnected(joint_id.into_raw()) })
    }

    /// Tries to wake the bodies attached to a joint.
    pub fn wake_joint_bodies(&mut self, joint_id: JointId) -> Result<()> {
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3Joint_WakeBodies(joint_id.into_raw()) };
        Ok(())
    }

    /// Returns the constraint force, in newtons, of a joint.
    pub fn joint_constraint_force(&self, joint_id: JointId) -> Result<Vec3> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Joint_GetConstraintForce(joint_id.into_raw())
        }))
    }

    /// Returns the constraint torque, in newton-meters, of a joint.
    pub fn joint_constraint_torque(&self, joint_id: JointId) -> Result<Vec3> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Joint_GetConstraintTorque(joint_id.into_raw())
        }))
    }

    /// Returns the linear separation, in world length units, of a joint.
    pub fn joint_linear_separation(&self, joint_id: JointId) -> Result<f32> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetLinearSeparation(joint_id.into_raw()) })
    }

    /// Returns the angular separation, in radians, of a joint.
    pub fn joint_angular_separation(&self, joint_id: JointId) -> Result<f32> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetAngularSeparation(joint_id.into_raw()) })
    }

    /// Tries to set generic constraint tuning on a joint.
    ///
    /// `JointTuning::hertz` is cycles per second and
    /// `JointTuning::damping_ratio` is dimensionless.
    pub fn set_joint_constraint_tuning(
        &mut self,
        joint_id: JointId,
        tuning: JointTuning,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        tuning.validate()?;
        let _call = enter_joint_call(self, joint_id)?;
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
    pub fn joint_constraint_tuning(&self, joint_id: JointId) -> Result<JointTuning> {
        let _call = enter_joint_call(self, joint_id)?;
        let mut hertz = 0.0;
        let mut damping_ratio = 0.0;
        unsafe {
            ffi::b3Joint_GetConstraintTuning(joint_id.into_raw(), &mut hertz, &mut damping_ratio)
        };
        Ok(JointTuning::new(hertz, damping_ratio))
    }

    /// Tries to set the force threshold, in newtons, for joint events.
    pub fn set_joint_force_threshold(&mut self, joint_id: JointId, threshold: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("joint.force_threshold", threshold)?;
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3Joint_SetForceThreshold(joint_id.into_raw(), threshold) };
        Ok(())
    }

    /// Returns the force threshold, in newtons, for joint events.
    pub fn joint_force_threshold(&self, joint_id: JointId) -> Result<f32> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetForceThreshold(joint_id.into_raw()) })
    }

    /// Tries to set the torque threshold, in newton-meters, for joint events.
    pub fn set_joint_torque_threshold(&mut self, joint_id: JointId, threshold: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("joint.torque_threshold", threshold)?;
        let _call = enter_joint_call(self, joint_id)?;
        unsafe { ffi::b3Joint_SetTorqueThreshold(joint_id.into_raw(), threshold) };
        Ok(())
    }

    /// Returns the torque threshold, in newton-meters, for joint events.
    pub fn joint_torque_threshold(&self, joint_id: JointId) -> Result<f32> {
        let _call = enter_joint_call(self, joint_id)?;
        Ok(unsafe { ffi::b3Joint_GetTorqueThreshold(joint_id.into_raw()) })
    }
}
