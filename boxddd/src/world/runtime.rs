use super::*;
use crate::error::InvalidValueReason;

impl World {
    /// Tries to simulate one fixed time step.
    ///
    /// This runs collision detection, integration, and constraint solving. The
    /// `time_step` should normally be fixed, and `sub_step_count` controls solver
    /// accuracy.
    #[inline]
    pub fn step(&mut self, time_step: f32, sub_step_count: i32) -> Result<()> {
        self.step_outcome(time_step, sub_step_count)?.into_result()
    }

    /// Tries to simulate one fixed time step and preserves post-step failures.
    ///
    /// An outer `Err` means validation or admission failed before Box3D was
    /// called. An `Ok` value proves that native simulation advanced and all
    /// Rust-side post-step finalizers committed, even when
    /// [`StepOutcome::post_step_error`] reports a callback or task failure.
    pub fn step_outcome(&mut self, time_step: f32, sub_step_count: i32) -> Result<StepOutcome> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("world.step.time_step", time_step)?;
        if sub_step_count < 0 {
            return Err(validation::invalid(
                "world.step.sub_step_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        let next_contact_epoch = self.state.ledger.prepare_contact_turnover()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        let invocation = callback_state::SharedCallbackState::default();
        let _invocation_guard = self
            .state
            .callbacks
            .install_invocation(invocation.clone())?;
        unsafe { ffi::b3World_Step(self.raw, time_step, sub_step_count) };

        // Native advancement is irreversible. Commit provenance before
        // projecting any callback or task failure to the caller.
        self.finish_step_provenance_locked(next_contact_epoch);
        Ok(StepOutcome::from_post_step_result(invocation.drain()))
    }

    /// Tries to return the world gravity vector.
    #[inline]
    pub fn gravity(&self) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        if !unsafe { ffi::b3World_IsValid(self.raw) } {
            return Err(Error::NativeFailure);
        }
        Ok(Vec3::from_raw(unsafe { ffi::b3World_GetGravity(self.raw) }))
    }

    /// Tries to set the world gravity vector.
    #[inline]
    pub fn set_gravity(&mut self, gravity: impl Into<Vec3>) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let gravity = gravity.into();
        validation::vec3("world.gravity", gravity)?;
        let _guard = box3d_lock::lock();
        if !unsafe { ffi::b3World_IsValid(self.raw) } {
            return Err(Error::NativeFailure);
        }
        unsafe { ffi::b3World_SetGravity(self.raw, gravity.into_raw()) };
        Ok(())
    }

    /// Tries to apply a radial explosion.
    pub fn explode(&mut self, explosion: &ExplosionDef) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let raw_explosion = explosion.lower()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_Explode(self.raw, &raw_explosion) };
        Ok(())
    }

    /// Tries to return the bounds covering the current simulation.
    #[inline]
    pub fn bounds(&self) -> Result<Aabb> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        if !unsafe { ffi::b3World_IsValid(self.raw) } {
            return Err(Error::NativeFailure);
        }
        Ok(Aabb::from_raw(unsafe { ffi::b3World_GetBounds(self.raw) }))
    }

    /// Tries to return the current world performance profile.
    #[inline]
    pub fn profile(&self) -> Result<Profile> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        if !unsafe { ffi::b3World_IsValid(self.raw) } {
            return Err(Error::NativeFailure);
        }
        Ok(Profile::from_raw(unsafe {
            ffi::b3World_GetProfile(self.raw)
        }))
    }

    /// Tries to return the current world counters and allocation sizes.
    #[inline]
    pub fn counters(&self) -> Result<Counters> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        if !unsafe { ffi::b3World_IsValid(self.raw) } {
            return Err(Error::NativeFailure);
        }
        Ok(Counters::from_raw(unsafe {
            ffi::b3World_GetCounters(self.raw)
        }))
    }

    /// Tries to enable or disable sleeping for the entire world.
    pub fn enable_sleeping(&mut self, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_EnableSleeping(self.raw, enabled) };
        Ok(())
    }

    /// Tries to return whether sleeping is enabled.
    pub fn sleeping_enabled(&self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_IsSleepingEnabled(self.raw) })
    }

    /// Tries to enable or disable continuous collision.
    pub fn enable_continuous(&mut self, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_EnableContinuous(self.raw, enabled) };
        Ok(())
    }

    /// Tries to return whether continuous collision is enabled.
    pub fn continuous_enabled(&self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_IsContinuousEnabled(self.raw) })
    }

    /// Tries to set the restitution speed threshold, usually in meters per second.
    pub fn set_restitution_threshold(&mut self, value: f32) -> Result<()> {
        validation::nonnegative("world.restitution_threshold", value)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_SetRestitutionThreshold(self.raw, value) };
        Ok(())
    }

    /// Tries to return the restitution speed threshold.
    pub fn restitution_threshold(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_GetRestitutionThreshold(self.raw) })
    }

    /// Tries to set the collision speed threshold required to emit hit events.
    pub fn set_hit_event_threshold(&mut self, value: f32) -> Result<()> {
        validation::nonnegative("world.hit_event_threshold", value)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_SetHitEventThreshold(self.raw, value) };
        Ok(())
    }

    /// Tries to return the hit-event speed threshold.
    pub fn hit_event_threshold(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_GetHitEventThreshold(self.raw) })
    }

    /// Tries to set advanced contact tuning parameters.
    pub fn set_contact_tuning(
        &mut self,
        hertz: f32,
        damping_ratio: f32,
        contact_speed: f32,
    ) -> Result<()> {
        validation::nonnegative("world.contact_hertz", hertz)?;
        validation::nonnegative("world.contact_damping_ratio", damping_ratio)?;
        validation::nonnegative("world.contact_speed", contact_speed)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_SetContactTuning(self.raw, hertz, damping_ratio, contact_speed) };
        Ok(())
    }

    /// Tries to set the contact point recycling distance.
    pub fn set_contact_recycle_distance(&mut self, distance: f32) -> Result<()> {
        validation::nonnegative("world.contact_recycle_distance", distance)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_SetContactRecycleDistance(self.raw, distance) };
        Ok(())
    }

    /// Tries to return the contact point recycling distance.
    pub fn contact_recycle_distance(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_GetContactRecycleDistance(self.raw) })
    }

    /// Tries to set the maximum linear speed, usually in meters per second.
    pub fn set_maximum_linear_speed(&mut self, speed: f32) -> Result<()> {
        validation::positive("world.maximum_linear_speed", speed)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_SetMaximumLinearSpeed(self.raw, speed) };
        Ok(())
    }

    /// Tries to return the maximum linear speed.
    pub fn maximum_linear_speed(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_GetMaximumLinearSpeed(self.raw) })
    }

    /// Tries to enable or disable constraint warm starting.
    pub fn enable_warm_starting(&mut self, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_EnableWarmStarting(self.raw, enabled) };
        Ok(())
    }

    /// Tries to return whether constraint warm starting is enabled.
    pub fn warm_starting_enabled(&self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_IsWarmStartingEnabled(self.raw) })
    }

    /// Tries to enable or disable speculative contacts.
    pub fn enable_speculative(&mut self, enabled: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_EnableSpeculative(self.raw, enabled) };
        Ok(())
    }

    /// Tries to return the number of awake bodies in the world.
    pub fn awake_body_count(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_GetAwakeBodyCount(self.raw) })
    }

    /// Tries to return the maximum capacity reached by this world.
    pub fn max_capacity(&self) -> Result<Capacity> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(Capacity::from_raw(unsafe {
            ffi::b3World_GetMaxCapacity(self.raw)
        }))
    }

    /// Tries to set the Box3D worker count for future simulation steps.
    pub fn set_worker_count(&mut self, count: i32) -> Result<()> {
        if count < 0 || count > ffi::B3_MAX_WORKERS as i32 {
            return Err(validation::invalid(
                "world.worker_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        #[cfg(target_arch = "wasm32")]
        if count > 1 {
            return Err(Error::UnsupportedOnWasm);
        }
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_SetWorkerCount(self.raw, count) };
        Ok(())
    }

    /// Tries to return the Box3D worker count.
    pub fn worker_count(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        Ok(unsafe { ffi::b3World_GetWorkerCount(self.raw) })
    }

    /// Tries to rebuild the broad-phase tree for static bodies.
    pub fn rebuild_static_tree(&mut self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        unsafe { ffi::b3World_RebuildStaticTree(self.raw) };
        Ok(())
    }
}
