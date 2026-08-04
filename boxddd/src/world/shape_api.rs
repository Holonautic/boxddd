use super::*;
use crate::error::InvalidValueReason;
use crate::query::ShapeRayHit;

impl World {
    /// Tries to destroy a shape attached to this world.
    pub fn destroy_shape(&mut self, shape_id: ShapeId, update_body_mass: bool) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3DestroyShape(shape_id.into_raw(), update_body_mass) };
        self.state_mut()
            .ledger
            .finish_contact_turnover(next_contact_epoch);
        let _backing = self.state_mut().ledger.retire_shape(shape_id);
        drop(_call);
        Ok(())
    }

    /// Tries to return the shape type.
    pub fn shape_type(&self, shape_id: ShapeId) -> Result<ShapeType> {
        let _call = self.enter_shape_call(shape_id)?;
        ShapeType::from_raw(unsafe { ffi::b3Shape_GetType(shape_id.into_raw()) })
            .ok_or(Error::NativeFailure)
    }

    /// Tries to return the body that owns the shape.
    pub fn shape_body(&self, shape_id: ShapeId) -> Result<BodyId> {
        let _call = self.enter_shape_call(shape_id)?;
        let raw = unsafe { ffi::b3Shape_GetBody(shape_id.into_raw()) };
        self.state().ledger.resolve_body(raw)
    }

    /// Tries to return whether the shape is configured as a sensor.
    pub fn shape_sensor(&self, shape_id: ShapeId) -> Result<bool> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_IsSensor(shape_id.into_raw()) })
    }

    /// Tries to set the shape name.
    pub fn set_shape_name(&mut self, shape_id: ShapeId, name: impl Into<Vec<u8>>) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let name = name.into();
        validation::count_i32("shape.name", name.len())?;
        let name = CString::new(name)
            .map_err(|_| validation::invalid("shape.name", InvalidValueReason::InteriorNul))?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetName(shape_id.into_raw(), name.as_ptr()) };
        Ok(())
    }

    /// Tries to return the shape name, or an empty string when no name is set.
    pub fn shape_name(&self, shape_id: ShapeId) -> Result<String> {
        let _call = self.enter_shape_call(shape_id)?;
        let ptr = unsafe { ffi::b3Shape_GetName(shape_id.into_raw()) };
        if ptr.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned())
    }

    /// Tries to set the shape density, optionally updating body mass.
    pub fn set_shape_density(
        &mut self,
        shape_id: ShapeId,
        density: f32,
        update_body_mass: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("shape.density", density)?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetDensity(shape_id.into_raw(), density, update_body_mass) };
        Ok(())
    }

    /// Tries to return the shape density.
    pub fn shape_density(&self, shape_id: ShapeId) -> Result<f32> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_GetDensity(shape_id.into_raw()) })
    }

    /// Tries to set the shape friction coefficient.
    pub fn set_shape_friction(&mut self, shape_id: ShapeId, friction: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("surface_material.friction", friction)?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetFriction(shape_id.into_raw(), friction) };
        Ok(())
    }

    /// Tries to return the shape friction coefficient.
    pub fn shape_friction(&self, shape_id: ShapeId) -> Result<f32> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_GetFriction(shape_id.into_raw()) })
    }

    /// Tries to set the shape restitution coefficient.
    pub fn set_shape_restitution(&mut self, shape_id: ShapeId, restitution: f32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validation::nonnegative("surface_material.restitution", restitution)?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetRestitution(shape_id.into_raw(), restitution) };
        Ok(())
    }

    /// Tries to return the shape restitution coefficient.
    pub fn shape_restitution(&self, shape_id: ShapeId) -> Result<f32> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_GetRestitution(shape_id.into_raw()) })
    }

    /// Tries to replace the shape's base surface material.
    pub fn set_shape_surface_material(
        &mut self,
        shape_id: ShapeId,
        material: SurfaceMaterial,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        material.validate()?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetSurfaceMaterial(shape_id.into_raw(), material.into_raw()) };
        Ok(())
    }

    /// Tries to return the shape's base surface material.
    pub fn shape_surface_material(&self, shape_id: ShapeId) -> Result<SurfaceMaterial> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(SurfaceMaterial::from_raw(unsafe {
            ffi::b3Shape_GetSurfaceMaterial(shape_id.into_raw())
        }))
    }

    /// Tries to return the number of mesh material slots on the shape.
    pub fn shape_mesh_material_count(&self, shape_id: ShapeId) -> Result<i32> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_GetMeshMaterialCount(shape_id.into_raw()) })
    }

    /// Tries to set a mesh material slot on the shape.
    pub fn set_shape_mesh_material(
        &mut self,
        shape_id: ShapeId,
        index: i32,
        material: SurfaceMaterial,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        material.validate()?;
        let _call = self.enter_shape_call(shape_id)?;
        let count = unsafe { ffi::b3Shape_GetMeshMaterialCount(shape_id.into_raw()) };
        if index < 0 || index >= count {
            return Err(validation::invalid(
                "shape.material_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        unsafe { ffi::b3Shape_SetMeshMaterial(shape_id.into_raw(), material.into_raw(), index) };
        Ok(())
    }

    /// Tries to return a mesh material slot from the shape.
    pub fn shape_mesh_surface_material(
        &self,
        shape_id: ShapeId,
        index: i32,
    ) -> Result<SurfaceMaterial> {
        let _call = self.enter_shape_call(shape_id)?;
        let count = unsafe { ffi::b3Shape_GetMeshMaterialCount(shape_id.into_raw()) };
        if index < 0 || index >= count {
            return Err(validation::invalid(
                "shape.material_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        Ok(SurfaceMaterial::from_raw(unsafe {
            ffi::b3Shape_GetMeshSurfaceMaterial(shape_id.into_raw(), index)
        }))
    }

    /// Tries to return the shape collision filter.
    pub fn shape_filter(&self, shape_id: ShapeId) -> Result<Filter> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(Filter::from_raw(unsafe {
            ffi::b3Shape_GetFilter(shape_id.into_raw())
        }))
    }

    /// Tries to set the shape collision filter.
    pub fn set_shape_filter(
        &mut self,
        shape_id: ShapeId,
        filter: Filter,
        invoke_contacts: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = invoke_contacts
            .then(|| self.state().ledger.prepare_contact_turnover())
            .transpose()?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetFilter(shape_id.into_raw(), filter.into_raw(), invoke_contacts) };
        if let Some(next_epoch) = next_contact_epoch {
            self.finish_contact_turnover(next_epoch);
        }
        drop(_call);
        Ok(())
    }

    /// Tries to enable or disable begin/end sensor events for the shape.
    pub fn enable_shape_sensor_events(&mut self, shape_id: ShapeId, enabled: bool) -> Result<()> {
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_EnableSensorEvents(shape_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to return whether sensor events are enabled for the shape.
    pub fn shape_sensor_events_enabled(&self, shape_id: ShapeId) -> Result<bool> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_AreSensorEventsEnabled(shape_id.into_raw()) })
    }

    /// Tries to enable or disable contact begin/end events for the shape.
    pub fn enable_shape_contact_events(&mut self, shape_id: ShapeId, enabled: bool) -> Result<()> {
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_EnableContactEvents(shape_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to return whether contact events are enabled for the shape.
    pub fn shape_contact_events_enabled(&self, shape_id: ShapeId) -> Result<bool> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_AreContactEventsEnabled(shape_id.into_raw()) })
    }

    /// Tries to enable or disable pre-solve callbacks for the shape.
    pub fn enable_shape_pre_solve_events(
        &mut self,
        shape_id: ShapeId,
        enabled: bool,
    ) -> Result<()> {
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_EnablePreSolveEvents(shape_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to return whether pre-solve callbacks are enabled for the shape.
    pub fn shape_pre_solve_events_enabled(&self, shape_id: ShapeId) -> Result<bool> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_ArePreSolveEventsEnabled(shape_id.into_raw()) })
    }

    /// Tries to enable or disable hit events for the shape.
    pub fn enable_shape_hit_events(&mut self, shape_id: ShapeId, enabled: bool) -> Result<()> {
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_EnableHitEvents(shape_id.into_raw(), enabled) };
        Ok(())
    }

    /// Tries to return whether hit events are enabled for the shape.
    pub fn shape_hit_events_enabled(&self, shape_id: ShapeId) -> Result<bool> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(unsafe { ffi::b3Shape_AreHitEventsEnabled(shape_id.into_raw()) })
    }

    /// Tries to return the shape's world-space AABB.
    pub fn shape_aabb(&self, shape_id: ShapeId) -> Result<Aabb> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(Aabb::from_raw(unsafe {
            ffi::b3Shape_GetAABB(shape_id.into_raw())
        }))
    }

    /// Tries to ray cast against a single shape.
    pub fn shape_cast_ray(
        &self,
        shape_id: ShapeId,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
    ) -> Result<Option<ShapeRayHit>> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        let translation = translation.into().validate()?;
        let _call = self.enter_shape_call(shape_id)?;
        let raw = unsafe {
            ffi::b3Shape_RayCast(
                shape_id.into_raw(),
                origin.into_raw(),
                translation.into_raw(),
            )
        };
        Ok(ShapeRayHit::from_raw(raw))
    }

    /// Tries to compute mass data for the shape.
    pub fn shape_mass_data(&self, shape_id: ShapeId) -> Result<MassData> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(MassData::from_raw(unsafe {
            ffi::b3Shape_ComputeMassData(shape_id.into_raw())
        }))
    }

    /// Tries to return the closest point on the shape to `target`.
    pub fn shape_closest_point(&self, shape_id: ShapeId, target: impl Into<Vec3>) -> Result<Vec3> {
        callback_state::check_not_in_callback()?;
        let target = target.into().validate()?;
        let _call = self.enter_shape_call(shape_id)?;
        Ok(Vec3::from_raw(unsafe {
            ffi::b3Shape_GetClosestPoint(shape_id.into_raw(), target.into_raw())
        }))
    }

    /// Tries to collect current contacts touching the shape.
    pub fn shape_contacts(&self, shape_id: ShapeId) -> Result<Vec<ContactData>> {
        let mut out = Vec::new();
        self.shape_contacts_into(shape_id, &mut out)?;
        Ok(out)
    }

    /// Tries to write current contacts touching the shape into `out`.
    pub fn shape_contacts_into(&self, shape_id: ShapeId, out: &mut Vec<ContactData>) -> Result<()> {
        let _call = self.enter_shape_call(shape_id)?;
        let capacity =
            unsafe { ffi::b3Shape_GetContactCapacity(shape_id.into_raw()) }.max(0) as usize;
        let raw = unsafe {
            ffi_vec::read_from_ffi(capacity, |ptr, cap| {
                ffi::b3Shape_GetContactData(shape_id.into_raw(), ptr, cap)
            })
        };
        ffi_vec::map_into_transactional(out, raw, |raw| {
            let contact_id = self.state().ledger.resolve_contact(raw.contactId)?;
            let shape_id_a = self.state().ledger.resolve_shape(raw.shapeIdA)?;
            let shape_id_b = self.state().ledger.resolve_shape(raw.shapeIdB)?;
            Ok(unsafe { ContactData::from_raw_parts(raw, contact_id, shape_id_a, shape_id_b) })
        })
    }

    /// Tries to collect shapes currently touching this sensor shape.
    pub fn shape_sensor_data(&self, shape_id: ShapeId) -> Result<Vec<ShapeId>> {
        let mut out = Vec::new();
        self.shape_sensor_data_into(shape_id, &mut out)?;
        Ok(out)
    }

    /// Tries to write shapes currently touching this sensor shape into `out`.
    pub fn shape_sensor_data_into(&self, shape_id: ShapeId, out: &mut Vec<ShapeId>) -> Result<()> {
        let _call = self.enter_shape_call(shape_id)?;
        let capacity =
            unsafe { ffi::b3Shape_GetSensorCapacity(shape_id.into_raw()) }.max(0) as usize;
        let raw = unsafe {
            ffi_vec::read_from_ffi(capacity, |ptr: *mut ffi::b3ShapeId, cap| {
                ffi::b3Shape_GetSensorData(shape_id.into_raw(), ptr, cap)
            })
        };
        let resolved = raw
            .into_iter()
            .map(|raw_shape| self.state().ledger.resolve_shape(raw_shape))
            .collect::<Result<Vec<_>>>()?;
        out.clear();
        out.extend(resolved);
        Ok(())
    }

    /// Tries to apply aerodynamic wind forces to a shape.
    pub fn apply_shape_wind(
        &mut self,
        shape_id: ShapeId,
        wind: impl Into<Vec3>,
        drag: f32,
        lift: f32,
        max_speed: f32,
        wake: bool,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let wind = wind.into();
        validation::vec3("shape.wind", wind)?;
        validation::nonnegative("shape.wind_drag", drag)?;
        validation::nonnegative("shape.wind_lift", lift)?;
        validation::positive("shape.wind_max_speed", max_speed)?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe {
            ffi::b3Shape_ApplyWind(
                shape_id.into_raw(),
                wind.into_raw(),
                drag,
                lift,
                max_speed,
                wake,
            )
        };
        Ok(())
    }

    /// Tries to return the sphere geometry for a sphere shape.
    pub fn shape_sphere(&self, shape_id: ShapeId) -> Result<Sphere> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(Sphere::from_raw(unsafe {
            ffi::b3Shape_GetSphere(shape_id.into_raw())
        }))
    }

    /// Tries to return the capsule geometry for a capsule shape.
    pub fn shape_capsule(&self, shape_id: ShapeId) -> Result<Capsule> {
        let _call = self.enter_shape_call(shape_id)?;
        Ok(Capsule::from_raw(unsafe {
            ffi::b3Shape_GetCapsule(shape_id.into_raw())
        }))
    }

    /// Tries to borrow hull geometry from a hull shape.
    ///
    /// The returned view is tied to `&self` and must not outlive the owning
    /// world or shape.
    pub fn shape_hull(&self, shape_id: ShapeId) -> Result<ShapeHull<'_>> {
        let _call = self.enter_shape_call(shape_id)?;
        ensure_shape_type(shape_id, ShapeType::Hull)?;
        let raw = unsafe { ffi::b3Shape_GetHull(shape_id.into_raw()) };
        unsafe { raw.as_ref() }
            .map(ShapeHull::from_raw)
            .ok_or(Error::NativeFailure)
    }

    /// Tries to borrow mesh geometry from a mesh shape.
    ///
    /// The returned view is tied to `&self` and must not outlive the owning
    /// world or shape.
    pub fn shape_mesh(&self, shape_id: ShapeId) -> Result<ShapeMesh<'_>> {
        let _call = self.enter_shape_call(shape_id)?;
        ensure_shape_type(shape_id, ShapeType::Mesh)?;
        ShapeMesh::from_raw(unsafe { ffi::b3Shape_GetMesh(shape_id.into_raw()) })
            .ok_or(Error::NativeFailure)
    }

    /// Tries to borrow height-field geometry from a height-field shape.
    ///
    /// The returned view is tied to `&self` and must not outlive the owning
    /// world or shape.
    pub fn shape_height_field(&self, shape_id: ShapeId) -> Result<ShapeHeightField<'_>> {
        let _call = self.enter_shape_call(shape_id)?;
        ensure_shape_type(shape_id, ShapeType::HeightField)?;
        let raw = unsafe { ffi::b3Shape_GetHeightField(shape_id.into_raw()) };
        unsafe { raw.as_ref() }
            .map(ShapeHeightField::from_raw)
            .ok_or(Error::NativeFailure)
    }

    /// Tries to borrow the compound resource backing a compound shape.
    ///
    /// The returned reference is the Rust sidecar that keeps Box3D's native
    /// compound allocation alive for this shape. It is tied to `&self` and
    /// becomes invalid if the shape is destroyed or replaced.
    pub fn shape_compound(&self, shape_id: ShapeId) -> Result<&Compound> {
        let _call = self.enter_shape_call(shape_id)?;
        ensure_shape_type(shape_id, ShapeType::Compound)?;
        match self.state().ledger.shape_resource(shape_id) {
            Some(ShapeResource::Compound { _data }) => Ok(_data),
            _ => Err(Error::NativeFailure),
        }
    }

    /// Tries to replace a shape's geometry with a sphere.
    pub fn set_shape_sphere(&mut self, shape_id: ShapeId, sphere: &Sphere) -> Result<()> {
        callback_state::check_not_in_callback()?;
        sphere.validate()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetSphere(shape_id.into_raw(), sphere.raw()) };
        self.state_mut()
            .ledger
            .finish_contact_turnover(next_contact_epoch);
        let _backing = self.state_mut().ledger.clear_shape_resource(shape_id);
        drop(_call);
        Ok(())
    }

    /// Tries to replace a shape's geometry with a capsule.
    pub fn set_shape_capsule(&mut self, shape_id: ShapeId, capsule: &Capsule) -> Result<()> {
        callback_state::check_not_in_callback()?;
        capsule.validate()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetCapsule(shape_id.into_raw(), capsule.raw()) };
        self.state_mut()
            .ledger
            .finish_contact_turnover(next_contact_epoch);
        let _backing = self.state_mut().ledger.clear_shape_resource(shape_id);
        drop(_call);
        Ok(())
    }

    /// Tries to replace a shape's geometry with an owned hull.
    pub fn set_shape_hull(&mut self, shape_id: ShapeId, hull: &Hull) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = self.enter_shape_call(shape_id)?;
        unsafe { ffi::b3Shape_SetHull(shape_id.into_raw(), hull.as_ptr()) };
        self.state_mut()
            .ledger
            .finish_contact_turnover(next_contact_epoch);
        let _backing = self.state_mut().ledger.clear_shape_resource(shape_id);
        drop(_call);
        Ok(())
    }

    /// Tries to replace a static shape's geometry with an owned mesh.
    pub fn set_shape_mesh(
        &mut self,
        shape_id: ShapeId,
        mesh: MeshData,
        scale: impl Into<Vec3>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let scale = validate_mesh_scale(scale.into())?;
        let mesh_ptr = mesh.as_ptr();
        let next_contact_epoch = self.state().ledger.prepare_contact_turnover()?;
        let _call = self.enter_shape_call(shape_id)?;
        let body = unsafe { ffi::b3Shape_GetBody(shape_id.into_raw()) };
        match BodyType::from_raw(unsafe { ffi::b3Body_GetType(body) }) {
            Some(BodyType::Static) => {}
            Some(_) => {
                return Err(validation::invalid(
                    "mesh_shape.body_type",
                    InvalidValueReason::InvalidCombination,
                ));
            }
            None => return Err(Error::NativeFailure),
        }
        unsafe { ffi::b3Shape_SetMesh(shape_id.into_raw(), mesh_ptr, scale.into_raw()) };
        self.state_mut()
            .ledger
            .finish_contact_turnover(next_contact_epoch);
        let _backing = self
            .state_mut()
            .ledger
            .replace_shape_resource(shape_id, ShapeResource::Mesh { _data: mesh });
        drop(_call);
        Ok(())
    }
}

fn ensure_shape_type(shape_id: ShapeId, expected: ShapeType) -> Result<()> {
    match ShapeType::from_raw(unsafe { ffi::b3Shape_GetType(shape_id.into_raw()) }) {
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(validation::invalid(
            "shape.type",
            InvalidValueReason::InvalidCombination,
        )),
        None => Err(Error::NativeFailure),
    }
}
