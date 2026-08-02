use super::*;
use crate::collision::{ShapeCastInput, ShapeProxy};
use crate::error::InvalidValueReason;
use crate::query::{BodyCastHit, BodyClosestPoint, MoverPlane, QueryFilter};

const INITIAL_BODY_MOVER_PLANE_CAPACITY: usize = 4;
const MAX_BODY_MOVER_PLANE_CAPACITY: usize = 64;

impl World {
    /// Tries to return the body origin in world coordinates.
    #[inline]
    pub fn body_position(&self, body_id: BodyId) -> Result<Pos> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Pos::from_raw(unsafe {
            ffi::b3Body_GetPosition(body_id.into_raw())
        }))
    }

    /// Tries to return the body rotation.
    #[inline]
    pub fn body_rotation(&self, body_id: BodyId) -> Result<Quat> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Quat::from_raw(unsafe {
            ffi::b3Body_GetRotation(body_id.into_raw())
        }))
    }

    /// Tries to return the body transform.
    #[inline]
    pub fn body_transform(&self, body_id: BodyId) -> Result<WorldTransform> {
        let _call = self.enter_body_call(body_id)?;
        Ok(WorldTransform::from_raw(unsafe {
            ffi::b3Body_GetTransform(body_id.into_raw())
        }))
    }

    /// Tries to destroy a body and its attached shapes.
    pub fn destroy_body(&mut self, body_id: BodyId) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let cascade = self.state().ledger.prepare_body_cascade(body_id)?;
        {
            let _call = self.enter_body_call(body_id)?;
            unsafe { ffi::b3DestroyBody(body_id.into_raw()) };
            self.state_mut()
                .ledger
                .finish_contact_turnover(next_contact_epoch);
        }
        self.state_mut()
            .ledger
            .finish_body_cascade(body_id, cascade);
        Ok(())
    }

    /// Tries to return the body type.
    pub fn body_type(&self, body_id: BodyId) -> Result<BodyType> {
        let _call = self.enter_body_call(body_id)?;
        BodyType::from_raw(unsafe { ffi::b3Body_GetType(body_id.into_raw()) })
            .ok_or(Error::NativeFailure)
    }

    /// Tries to set the body type.
    pub fn set_body_type(&mut self, body_id: BodyId, body_type: BodyType) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let shape_ids = self.state().ledger.body_shapes(body_id)?;
        let _call = self.enter_body_call(body_id)?;
        if body_type != BodyType::Static {
            for shape_id in shape_ids {
                match ShapeType::from_raw(unsafe { ffi::b3Shape_GetType(shape_id.into_raw()) }) {
                    Some(ShapeType::Compound | ShapeType::HeightField | ShapeType::Mesh) => {
                        return Err(validation::invalid(
                            "body.type",
                            InvalidValueReason::InvalidCombination,
                        ));
                    }
                    Some(_) => {}
                    None => return Err(Error::NativeFailure),
                }
            }
        }
        unsafe { ffi::b3Body_SetType(body_id.into_raw(), body_type.into_raw()) };
        self.state_mut()
            .ledger
            .finish_contact_turnover(next_contact_epoch);
        drop(_call);
        Ok(())
    }

    /// Tries to set the body name.
    pub fn set_body_name(&mut self, body_id: BodyId, name: impl Into<Vec<u8>>) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let name = name.into();
        validation::count_i32("body.name", name.len())?;
        let name = CString::new(name)
            .map_err(|_| validation::invalid("body.name", InvalidValueReason::InteriorNul))?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetName(body_id.into_raw(), name.as_ptr()) };
        Ok(())
    }

    /// Tries to return the body name, if one is set.
    pub fn body_name(&self, body_id: BodyId) -> Result<Option<String>> {
        let _call = self.enter_body_call(body_id)?;
        let ptr = unsafe { ffi::b3Body_GetName(body_id.into_raw()) };
        if ptr.is_null() {
            Ok(None)
        } else {
            Ok(Some(
                unsafe { CStr::from_ptr(ptr) }
                    .to_string_lossy()
                    .into_owned(),
            ))
        }
    }

    /// Tries to allow or disallow high angular velocity for an axially symmetric body.
    pub fn set_body_fast_rotation_allowed(&mut self, body_id: BodyId, allowed: bool) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_AllowFastRotation(body_id.into_raw(), allowed) };
        Ok(())
    }

    /// Tries to return whether high angular velocity is allowed for a body.
    pub fn body_fast_rotation_allowed(&self, body_id: BodyId) -> Result<bool> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_IsFastRotationAllowed(body_id.into_raw()) })
    }

    /// Tries to set the body position and rotation.
    pub fn set_body_transform(
        &mut self,
        body_id: BodyId,
        position: impl Into<Pos>,
        rotation: Quat,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let position = position.into().validate()?;
        rotation.validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe {
            ffi::b3Body_SetTransform(body_id.into_raw(), position.into_raw(), rotation.into_raw())
        };
        Ok(())
    }

    /// Tries to set a target transform for kinematic-style movement.
    pub fn set_body_target_transform(
        &mut self,
        body_id: BodyId,
        target: WorldTransform,
        time_step: f32,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::position("body.target_transform.position", target.p)?;
        validation::quaternion("body.target_transform.rotation", target.q)?;
        validation::positive("body.target_transform.time_step", time_step)?;
        let _call = self.enter_body_call(body_id)?;
        unsafe {
            ffi::b3Body_SetTargetTransform(body_id.into_raw(), target.into_raw(), time_step, wake)
        };
        Ok(())
    }

    /// Tries to transform a world-space point into body-local coordinates.
    pub fn body_local_point(&self, body_id: BodyId, world_point: impl Into<Pos>) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let world_point = world_point.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetLocalPoint(body_id.into_raw(), world_point.into_raw())
        }))
    }

    /// Tries to transform a body-local point into world coordinates.
    pub fn body_world_point(&self, body_id: BodyId, local_point: impl Into<Vec3>) -> Result<Pos> {
        callback_state::check_not_in_callback()?;
        let local_point = local_point.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        Ok(Pos::from_raw(unsafe {
            ffi::b3Body_GetWorldPoint(body_id.into_raw(), local_point.into_raw())
        }))
    }

    /// Tries to transform a world-space vector into body-local coordinates.
    pub fn body_local_vector(
        &self,
        body_id: BodyId,
        world_vector: impl Into<Vec3>,
    ) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let world_vector = world_vector.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetLocalVector(body_id.into_raw(), world_vector.into_raw())
        }))
    }

    /// Tries to transform a body-local vector into world coordinates.
    pub fn body_world_vector(
        &self,
        body_id: BodyId,
        local_vector: impl Into<Vec3>,
    ) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let local_vector = local_vector.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetWorldVector(body_id.into_raw(), local_vector.into_raw())
        }))
    }

    /// Tries to return the velocity of a body-local point.
    pub fn body_local_point_velocity(
        &self,
        body_id: BodyId,
        local_point: impl Into<Vec3>,
    ) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let local_point = local_point.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetLocalPointVelocity(body_id.into_raw(), local_point.into_raw())
        }))
    }

    /// Tries to return the velocity of a world-space point on the body.
    pub fn body_world_point_velocity(
        &self,
        body_id: BodyId,
        world_point: impl Into<Pos>,
    ) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let world_point = world_point.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetWorldPointVelocity(body_id.into_raw(), world_point.into_raw())
        }))
    }

    /// Tries to return the body's linear velocity.
    pub fn body_linear_velocity(&self, body_id: BodyId) -> Result<Vec3> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetLinearVelocity(body_id.into_raw())
        }))
    }

    /// Tries to set the body's linear velocity.
    pub fn set_body_linear_velocity(
        &mut self,
        body_id: BodyId,
        velocity: impl Into<Vec3>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let velocity = velocity.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetLinearVelocity(body_id.into_raw(), velocity.into_raw()) };
        Ok(())
    }

    /// Tries to return the body's angular velocity.
    pub fn body_angular_velocity(&self, body_id: BodyId) -> Result<Vec3> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetAngularVelocity(body_id.into_raw())
        }))
    }

    /// Tries to set the body's angular velocity.
    pub fn set_body_angular_velocity(
        &mut self,
        body_id: BodyId,
        velocity: impl Into<Vec3>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let velocity = velocity.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetAngularVelocity(body_id.into_raw(), velocity.into_raw()) };
        Ok(())
    }

    /// Tries to apply a force at a world-space point.
    pub fn apply_force(
        &mut self,
        body_id: BodyId,
        force: impl Into<Vec3>,
        point: impl Into<Pos>,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let force = force.into().validate()?;
        let point = point.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe {
            ffi::b3Body_ApplyForce(body_id.into_raw(), force.into_raw(), point.into_raw(), wake)
        };
        Ok(())
    }

    /// Tries to apply a force at the body's center of mass.
    pub fn apply_force_to_center(
        &mut self,
        body_id: BodyId,
        force: impl Into<Vec3>,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let force = force.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_ApplyForceToCenter(body_id.into_raw(), force.into_raw(), wake) };
        Ok(())
    }

    /// Tries to apply a torque to the body.
    pub fn apply_torque(
        &mut self,
        body_id: BodyId,
        torque: impl Into<Vec3>,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let torque = torque.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_ApplyTorque(body_id.into_raw(), torque.into_raw(), wake) };
        Ok(())
    }

    /// Tries to apply a linear impulse at a world-space point.
    pub fn apply_linear_impulse(
        &mut self,
        body_id: BodyId,
        impulse: impl Into<Vec3>,
        point: impl Into<Pos>,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let impulse = impulse.into().validate()?;
        let point = point.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe {
            ffi::b3Body_ApplyLinearImpulse(
                body_id.into_raw(),
                impulse.into_raw(),
                point.into_raw(),
                wake,
            )
        };
        Ok(())
    }

    /// Tries to apply a linear impulse at the body's center of mass.
    pub fn apply_linear_impulse_to_center(
        &mut self,
        body_id: BodyId,
        impulse: impl Into<Vec3>,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let impulse = impulse.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe {
            ffi::b3Body_ApplyLinearImpulseToCenter(body_id.into_raw(), impulse.into_raw(), wake)
        };
        Ok(())
    }

    /// Tries to apply an angular impulse to the body.
    pub fn apply_angular_impulse(
        &mut self,
        body_id: BodyId,
        impulse: impl Into<Vec3>,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let impulse = impulse.into().validate()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_ApplyAngularImpulse(body_id.into_raw(), impulse.into_raw(), wake) };
        Ok(())
    }

    /// Tries to return the body mass.
    pub fn body_mass(&self, body_id: BodyId) -> Result<f32> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_GetMass(body_id.into_raw()) })
    }

    /// Tries to return the local rotational inertia tensor.
    pub fn body_local_rotational_inertia(&self, body_id: BodyId) -> Result<Matrix3> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Matrix3::from_raw(unsafe {
            ffi::b3Body_GetLocalRotationalInertia(body_id.into_raw())
        }))
    }

    /// Tries to return the inverse mass.
    pub fn body_inverse_mass(&self, body_id: BodyId) -> Result<f32> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_GetInverseMass(body_id.into_raw()) })
    }

    /// Tries to return the world inverse rotational inertia tensor.
    pub fn body_world_inverse_rotational_inertia(&self, body_id: BodyId) -> Result<Matrix3> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Matrix3::from_raw(unsafe {
            ffi::b3Body_GetWorldInverseRotationalInertia(body_id.into_raw())
        }))
    }

    /// Tries to return the local center of mass.
    pub fn body_local_center_of_mass(&self, body_id: BodyId) -> Result<Vec3> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Body_GetLocalCenter(body_id.into_raw())
        }))
    }

    /// Tries to return the world center of mass.
    pub fn body_world_center_of_mass(&self, body_id: BodyId) -> Result<Pos> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Pos::from_raw(unsafe {
            ffi::b3Body_GetWorldCenter(body_id.into_raw())
        }))
    }

    /// Tries to return the full body mass data.
    pub fn body_mass_data(&self, body_id: BodyId) -> Result<MassData> {
        let _call = self.enter_body_call(body_id)?;
        Ok(MassData::from_raw(unsafe {
            ffi::b3Body_GetMassData(body_id.into_raw())
        }))
    }

    /// Tries to override the body mass data.
    pub fn set_body_mass_data(&mut self, body_id: BodyId, mass_data: MassData) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("body.mass", mass_data.mass)?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetMassData(body_id.into_raw(), mass_data.into_raw()) };
        Ok(())
    }

    /// Tries to recompute body mass from attached shapes.
    pub fn apply_mass_from_shapes(&mut self, body_id: BodyId) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_ApplyMassFromShapes(body_id.into_raw()) };
        Ok(())
    }

    /// Tries to set the body's linear damping.
    pub fn set_body_linear_damping(&mut self, body_id: BodyId, damping: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("body.linear_damping", damping)?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetLinearDamping(body_id.into_raw(), damping) };
        Ok(())
    }

    /// Tries to return the body's linear damping.
    pub fn body_linear_damping(&self, body_id: BodyId) -> Result<f32> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_GetLinearDamping(body_id.into_raw()) })
    }

    /// Tries to set the body's angular damping.
    pub fn set_body_angular_damping(&mut self, body_id: BodyId, damping: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("body.angular_damping", damping)?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetAngularDamping(body_id.into_raw(), damping) };
        Ok(())
    }

    /// Tries to return the body's angular damping.
    pub fn body_angular_damping(&self, body_id: BodyId) -> Result<f32> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_GetAngularDamping(body_id.into_raw()) })
    }

    /// Tries to set the body's gravity scale.
    pub fn set_body_gravity_scale(&mut self, body_id: BodyId, scale: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::finite("body.gravity_scale", scale)?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetGravityScale(body_id.into_raw(), scale) };
        Ok(())
    }

    /// Tries to return the body's gravity scale.
    pub fn body_gravity_scale(&self, body_id: BodyId) -> Result<f32> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_GetGravityScale(body_id.into_raw()) })
    }

    /// Tries to return whether the body is awake.
    pub fn body_awake(&self, body_id: BodyId) -> Result<bool> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_IsAwake(body_id.into_raw()) })
    }

    /// Tries to wake or sleep the body.
    pub fn set_body_awake(&mut self, body_id: BodyId, awake: bool) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetAwake(body_id.into_raw(), awake) };
        Ok(())
    }

    /// Tries to enable or disable sleeping for the body.
    pub fn enable_body_sleep(&mut self, body_id: BodyId, enabled: bool) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_EnableSleep(body_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to return whether sleeping is enabled for the body.
    pub fn body_sleep_enabled(&self, body_id: BodyId) -> Result<bool> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_IsSleepEnabled(body_id.into_raw()) })
    }

    /// Tries to set the body's sleep speed threshold.
    pub fn set_body_sleep_threshold(&mut self, body_id: BodyId, threshold: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("body.sleep_threshold", threshold)?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetSleepThreshold(body_id.into_raw(), threshold) };
        Ok(())
    }

    /// Tries to return the body's sleep speed threshold.
    pub fn body_sleep_threshold(&self, body_id: BodyId) -> Result<f32> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_GetSleepThreshold(body_id.into_raw()) })
    }

    /// Tries to return whether the body is enabled in the simulation.
    pub fn body_enabled(&self, body_id: BodyId) -> Result<bool> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_IsEnabled(body_id.into_raw()) })
    }

    /// Tries to enable the body in the simulation.
    pub fn enable_body(&mut self, body_id: BodyId) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_Enable(body_id.into_raw()) };
        Ok(())
    }

    /// Tries to disable the body in the simulation.
    pub fn disable_body(&mut self, body_id: BodyId) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_Disable(body_id.into_raw()) };
        self.finish_contact_turnover(next_contact_epoch);
        drop(_call);
        Ok(())
    }

    /// Tries to set body translation and rotation locks.
    pub fn set_body_motion_locks(&mut self, body_id: BodyId, locks: MotionLocks) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetMotionLocks(body_id.into_raw(), locks.into_raw()) };
        Ok(())
    }

    /// Tries to return body translation and rotation locks.
    pub fn body_motion_locks(&self, body_id: BodyId) -> Result<MotionLocks> {
        let _call = self.enter_body_call(body_id)?;
        Ok(MotionLocks::from_raw(unsafe {
            ffi::b3Body_GetMotionLocks(body_id.into_raw())
        }))
    }

    /// Tries to set whether the body uses bullet-style continuous collision.
    pub fn set_body_bullet(&mut self, body_id: BodyId, bullet: bool) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_SetBullet(body_id.into_raw(), bullet) };
        Ok(())
    }

    /// Tries to return whether the body uses bullet-style continuous collision.
    pub fn body_bullet(&self, body_id: BodyId) -> Result<bool> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_IsBullet(body_id.into_raw()) })
    }

    /// Tries to enable or disable contact recycling for the body.
    pub fn enable_body_contact_recycling(&mut self, body_id: BodyId, enabled: bool) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_EnableContactRecycling(body_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to return whether contact recycling is enabled for the body.
    pub fn body_contact_recycling_enabled(&self, body_id: BodyId) -> Result<bool> {
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe { ffi::b3Body_IsContactRecyclingEnabled(body_id.into_raw()) })
    }

    /// Tries to enable or disable hit events for the body.
    pub fn enable_body_hit_events(&mut self, body_id: BodyId, enabled: bool) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        unsafe { ffi::b3Body_EnableHitEvents(body_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to compute the body AABB from its attached shapes.
    pub fn body_aabb(&self, body_id: BodyId) -> Result<Aabb> {
        let _call = self.enter_body_call(body_id)?;
        Ok(Aabb::from_raw(unsafe {
            ffi::b3Body_ComputeAABB(body_id.into_raw())
        }))
    }

    /// Tries to find the closest point on the body to `target`.
    pub fn body_closest_point(
        &self,
        body_id: BodyId,
        target: impl Into<Vec3>,
    ) -> Result<BodyClosestPoint> {
        callback_state::check_not_in_callback()?;
        let target = target.into().validate()?;
        let mut point = Vec3::ZERO.into_raw();
        let _call = self.enter_body_call(body_id)?;
        let distance = unsafe {
            ffi::b3Body_GetClosestPoint(body_id.into_raw(), &mut point, target.into_raw())
        };
        Ok(BodyClosestPoint {
            point: Vec3::from_raw(point),
            distance,
        })
    }

    /// Tries to ray cast against the body's attached shapes.
    pub fn body_cast_ray(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
    ) -> Result<Option<BodyCastHit>> {
        let body_transform = self.body_transform(body_id)?;
        self.body_cast_ray_with_transform(body_id, origin, translation, filter, body_transform)
    }

    /// Tries to ray cast against the body using an explicit body transform.
    pub fn body_cast_ray_with_transform(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
        body_transform: WorldTransform,
    ) -> Result<Option<BodyCastHit>> {
        self.body_cast_ray_with_options(body_id, origin, translation, filter, 1.0, body_transform)
    }

    /// Tries to ray cast against the body with an explicit transform and max fraction.
    pub fn body_cast_ray_with_options(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
        max_fraction: f32,
        body_transform: WorldTransform,
    ) -> Result<Option<BodyCastHit>> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        let translation = translation.into().validate()?;
        validation::nonnegative("body.cast_ray.max_fraction", max_fraction)?;
        validate_world_transform(body_transform)?;
        let _call = self.enter_body_call(body_id)?;
        let raw = unsafe {
            ffi::b3Body_CastRay(
                body_id.into_raw(),
                origin.into_raw(),
                translation.into_raw(),
                filter.raw(),
                max_fraction,
                body_transform.into_raw(),
            )
        };
        if raw.hit {
            let shape_id = self.state().ledger.resolve_shape(raw.shapeId)?;
            Ok(BodyCastHit::from_raw(raw, shape_id))
        } else {
            Ok(None)
        }
    }

    /// Tries to sweep a shape proxy against the body's attached shapes.
    pub fn body_cast_shape(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        input: ShapeCastInput,
        filter: QueryFilter,
    ) -> Result<Option<BodyCastHit>> {
        let body_transform = self.body_transform(body_id)?;
        self.body_cast_shape_with_transform(body_id, origin, input, filter, body_transform)
    }

    /// Tries to sweep a shape proxy against the body using an explicit body transform.
    pub fn body_cast_shape_with_transform(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        input: ShapeCastInput,
        filter: QueryFilter,
        body_transform: WorldTransform,
    ) -> Result<Option<BodyCastHit>> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        let input = input.validate()?;
        validate_world_transform(body_transform)?;
        let raw_input = input.raw();
        let _call = self.enter_body_call(body_id)?;
        let raw = unsafe {
            ffi::b3Body_CastShape(
                body_id.into_raw(),
                origin.into_raw(),
                &raw_input.proxy,
                raw_input.translation,
                filter.raw(),
                raw_input.maxFraction,
                raw_input.canEncroach,
                body_transform.into_raw(),
            )
        };
        if raw.hit {
            let shape_id = self.state().ledger.resolve_shape(raw.shapeId)?;
            Ok(BodyCastHit::from_raw(raw, shape_id))
        } else {
            Ok(None)
        }
    }

    /// Tries to test whether a shape proxy overlaps the body.
    pub fn body_overlap_shape(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        proxy: &ShapeProxy,
        filter: QueryFilter,
    ) -> Result<bool> {
        let body_transform = self.body_transform(body_id)?;
        self.body_overlap_shape_with_transform(body_id, origin, proxy, filter, body_transform)
    }

    /// Tries to test shape overlap against the body using an explicit body transform.
    pub fn body_overlap_shape_with_transform(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        proxy: &ShapeProxy,
        filter: QueryFilter,
        body_transform: WorldTransform,
    ) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        validate_world_transform(body_transform)?;
        let raw_proxy = proxy.raw();
        let _call = self.enter_body_call(body_id)?;
        Ok(unsafe {
            ffi::b3Body_OverlapShape(
                body_id.into_raw(),
                origin.into_raw(),
                &raw_proxy,
                filter.raw(),
                body_transform.into_raw(),
            )
        })
    }

    /// Tries to collect mover collision planes against the body.
    pub fn body_collide_mover(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        mover: &Capsule,
        filter: QueryFilter,
    ) -> Result<Vec<MoverPlane>> {
        let body_transform = self.body_transform(body_id)?;
        self.body_collide_mover_with_transform(body_id, origin, mover, filter, body_transform)
    }

    /// Tries to collect mover collision planes using an explicit body transform.
    pub fn body_collide_mover_with_transform(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        mover: &Capsule,
        filter: QueryFilter,
        body_transform: WorldTransform,
    ) -> Result<Vec<MoverPlane>> {
        let mut out = Vec::new();
        self.body_collide_mover_into_with_transform(
            body_id,
            origin,
            mover,
            filter,
            body_transform,
            &mut out,
        )?;
        Ok(out)
    }

    /// Tries to write mover collision planes into `out`.
    pub fn body_collide_mover_into_with_transform(
        &self,
        body_id: BodyId,
        origin: impl Into<Pos>,
        mover: &Capsule,
        filter: QueryFilter,
        body_transform: WorldTransform,
        out: &mut Vec<MoverPlane>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        mover.validate()?;
        validate_world_transform(body_transform)?;

        let _call = self.enter_body_call(body_id)?;
        let mut capacity = INITIAL_BODY_MOVER_PLANE_CAPACITY;
        loop {
            let mut raw_planes = Vec::<ffi::b3BodyPlaneResult>::with_capacity(capacity);
            let raw_count = unsafe {
                ffi::b3Body_CollideMover(
                    body_id.into_raw(),
                    raw_planes.as_mut_ptr(),
                    capacity as i32,
                    origin.into_raw(),
                    mover.raw(),
                    filter.raw(),
                    body_transform.into_raw(),
                )
            };
            let filled = raw_count.clamp(0, capacity as i32) as usize;
            unsafe { raw_planes.set_len(filled) };

            if raw_count < capacity as i32 || capacity == MAX_BODY_MOVER_PLANE_CAPACITY {
                let planes = raw_planes
                    .into_iter()
                    .map(|raw| {
                        self.state()
                            .ledger
                            .resolve_shape(raw.shapeId)
                            .map(|shape_id| MoverPlane::from_raw(shape_id, raw.result))
                    })
                    .collect::<Result<Vec<_>>>()?;
                out.clear();
                out.extend(planes);
                return Ok(());
            }

            capacity = (capacity * 2).min(MAX_BODY_MOVER_PLANE_CAPACITY);
        }
    }

    /// Tries to collect shape ids attached to the body.
    pub fn body_shapes(&self, body_id: BodyId) -> Result<Vec<ShapeId>> {
        let mut out = Vec::new();
        self.body_shapes_into(body_id, &mut out)?;
        Ok(out)
    }

    /// Tries to write shape ids attached to the body into `out`.
    pub fn body_shapes_into(&self, body_id: BodyId, out: &mut Vec<ShapeId>) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        let capacity = unsafe { ffi::b3Body_GetShapeCount(body_id.into_raw()) }.max(0) as usize;
        let raw = unsafe {
            ffi_vec::read_from_ffi(capacity, |ptr: *mut ffi::b3ShapeId, cap| {
                ffi::b3Body_GetShapes(body_id.into_raw(), ptr.cast(), cap)
            })
        };
        let resolved = raw
            .into_iter()
            .map(|raw| self.state().ledger.resolve_shape(raw))
            .collect::<Result<Vec<_>>>()?;
        out.clear();
        out.extend(resolved);
        Ok(())
    }

    /// Tries to collect joint ids attached to the body.
    pub fn body_joints(&self, body_id: BodyId) -> Result<Vec<JointId>> {
        let mut out = Vec::new();
        self.body_joints_into(body_id, &mut out)?;
        Ok(out)
    }

    /// Tries to write joint ids attached to the body into `out`.
    pub fn body_joints_into(&self, body_id: BodyId, out: &mut Vec<JointId>) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        let capacity = unsafe { ffi::b3Body_GetJointCount(body_id.into_raw()) }.max(0) as usize;
        let raw = unsafe {
            ffi_vec::read_from_ffi(capacity, |ptr: *mut ffi::b3JointId, cap| {
                ffi::b3Body_GetJoints(body_id.into_raw(), ptr.cast(), cap)
            })
        };
        let resolved = raw
            .into_iter()
            .map(|raw| self.state().ledger.resolve_joint(raw))
            .collect::<Result<Vec<_>>>()?;
        out.clear();
        out.extend(resolved);
        Ok(())
    }

    /// Tries to collect current contact data for the body.
    pub fn body_contacts(&self, body_id: BodyId) -> Result<Vec<ContactData>> {
        let mut out = Vec::new();
        self.body_contacts_into(body_id, &mut out)?;
        Ok(out)
    }

    /// Tries to write current contact data for the body into `out`.
    pub fn body_contacts_into(&self, body_id: BodyId, out: &mut Vec<ContactData>) -> Result<()> {
        let _call = self.enter_body_call(body_id)?;
        let capacity =
            unsafe { ffi::b3Body_GetContactCapacity(body_id.into_raw()) }.max(0) as usize;
        let raw = unsafe {
            crate::core::ffi_vec::read_from_ffi(capacity, |ptr, cap| {
                ffi::b3Body_GetContactData(body_id.into_raw(), ptr, cap)
            })
        };
        ffi_vec::map_into_transactional(out, raw, |raw| {
            let contact_id = self.state().ledger.resolve_contact(raw.contactId)?;
            let shape_id_a = self.state().ledger.resolve_shape(raw.shapeIdA)?;
            let shape_id_b = self.state().ledger.resolve_shape(raw.shapeIdB)?;
            Ok(unsafe { ContactData::from_raw_parts(raw, contact_id, shape_id_a, shape_id_b) })
        })
    }
}

#[inline]
fn validate_world_transform(transform: WorldTransform) -> Result<()> {
    validation::position("body.query_transform.position", transform.p)?;
    validation::quaternion("body.query_transform.rotation", transform.q)
}
