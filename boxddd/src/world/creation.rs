use super::*;

impl World {
    /// Tries to create a body in this world.
    pub fn try_create_body(&mut self, def: BodyDef) -> Result<BodyId> {
        callback_state::check_not_in_callback()?;
        let prepared = def.prepare()?;
        let pending = self.ledger.reserve_body()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe { ffi::b3CreateBody(self.raw, &raw_def) };
        if unsafe { ffi::b3Body_IsValid(raw) } {
            Ok(self.ledger.publish_body(raw, pending))
        } else {
            Err(Error::CreateBodyFailed)
        }
    }

    /// Creates a body or panics if Box3D rejects the definition.
    pub fn create_body(&mut self, def: BodyDef) -> BodyId {
        self.try_create_body(def)
            .expect("Box3D failed to create body")
    }

    /// Tries to attach a sphere shape to a body.
    pub fn try_create_sphere_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        sphere: &Sphere,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        sphere.validate()?;
        let pending = self.ledger.reserve_shape()?;
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe { ffi::b3CreateSphereShape(body_id.into_raw(), &raw_def, sphere.raw()) };
        if unsafe { ffi::b3Shape_IsValid(raw) } {
            Ok(self.ledger.publish_shape(raw, pending))
        } else {
            Err(Error::CreateShapeFailed)
        }
    }

    /// Creates a sphere shape or panics if Box3D rejects it.
    pub fn create_sphere_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        sphere: &Sphere,
    ) -> ShapeId {
        self.try_create_sphere_shape(body_id, def, sphere)
            .expect("Box3D failed to create sphere shape")
    }

    /// Tries to attach a box-hull shape to a body.
    pub fn try_create_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &BoxHull,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        let pending = self.ledger.reserve_shape()?;
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe { ffi::b3CreateHullShape(body_id.into_raw(), &raw_def, hull.hull_data()) };
        if unsafe { ffi::b3Shape_IsValid(raw) } {
            Ok(self.ledger.publish_shape(raw, pending))
        } else {
            Err(Error::CreateShapeFailed)
        }
    }

    /// Creates a box-hull shape or panics if Box3D rejects it.
    pub fn create_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &BoxHull,
    ) -> ShapeId {
        self.try_create_hull_shape(body_id, def, hull)
            .expect("Box3D failed to create hull shape")
    }

    /// Tries to attach a capsule shape to a body.
    pub fn try_create_capsule_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        capsule: &Capsule,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        capsule.validate()?;
        let pending = self.ledger.reserve_shape()?;
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe { ffi::b3CreateCapsuleShape(body_id.into_raw(), &raw_def, capsule.raw()) };
        if unsafe { ffi::b3Shape_IsValid(raw) } {
            Ok(self.ledger.publish_shape(raw, pending))
        } else {
            Err(Error::CreateShapeFailed)
        }
    }

    /// Creates a capsule shape or panics if Box3D rejects it.
    pub fn create_capsule_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        capsule: &Capsule,
    ) -> ShapeId {
        self.try_create_capsule_shape(body_id, def, capsule)
            .expect("Box3D failed to create capsule shape")
    }

    /// Tries to attach an owned convex hull resource to a body.
    pub fn try_create_created_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &Hull,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        let pending = self.ledger.reserve_shape()?;
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe { ffi::b3CreateHullShape(body_id.into_raw(), &raw_def, hull.as_ptr()) };
        if unsafe { ffi::b3Shape_IsValid(raw) } {
            Ok(self.ledger.publish_shape(raw, pending))
        } else {
            Err(Error::CreateShapeFailed)
        }
    }

    /// Tries to attach a transformed owned convex hull resource to a body.
    pub fn try_create_transformed_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &Hull,
        transform: impl Into<crate::types::Transform>,
        scale: impl Into<Vec3>,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        let transform = transform.into();
        transform.validate()?;
        let scale = scale.into().validate()?;
        let pending = self.ledger.reserve_shape()?;
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe {
            ffi::b3CreateTransformedHullShape(
                body_id.into_raw(),
                &raw_def,
                hull.as_ptr(),
                transform.into_raw(),
                scale.into_raw(),
            )
        };
        if unsafe { ffi::b3Shape_IsValid(raw) } {
            Ok(self.ledger.publish_shape(raw, pending))
        } else {
            Err(Error::CreateShapeFailed)
        }
    }

    /// Tries to attach a triangle mesh shape to a static body.
    pub fn try_create_mesh_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        mesh: MeshData,
        scale: impl Into<Vec3>,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::PerTriangle)?;
        let scale = validate_mesh_scale(scale.into())?;
        if self.try_body_type(body_id)? != BodyType::Static {
            return Err(Error::InvalidArgument);
        }
        self.resources
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let pending = self.ledger.reserve_shape()?;
        let mesh_ptr = mesh.as_ptr();
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe {
            ffi::b3CreateMeshShape(body_id.into_raw(), &raw_def, mesh_ptr, scale.into_raw())
        };
        if !unsafe { ffi::b3Shape_IsValid(raw) } {
            return Err(Error::CreateShapeFailed);
        }
        let shape_id = self.ledger.publish_shape(raw, pending);
        drop(_guard);
        self.resources
            .insert(shape_id, ShapeResource::Mesh { _data: mesh });
        Ok(shape_id)
    }

    /// Tries to attach a height-field shape to a static body.
    pub fn try_create_height_field_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        height_field: HeightField,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::PerTriangle)?;
        if self.try_body_type(body_id)? != BodyType::Static {
            return Err(Error::InvalidArgument);
        }
        self.resources
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let pending = self.ledger.reserve_shape()?;
        let height_field_ptr = height_field.as_ptr();
        let _guard = self.lock_body_checked(body_id)?;
        let raw_def = prepared.lower_locked();
        let raw = unsafe {
            ffi::b3CreateHeightFieldShape(body_id.into_raw(), &raw_def, height_field_ptr)
        };
        if !unsafe { ffi::b3Shape_IsValid(raw) } {
            return Err(Error::CreateShapeFailed);
        }
        let shape_id = self.ledger.publish_shape(raw, pending);
        drop(_guard);
        self.resources.insert(
            shape_id,
            ShapeResource::HeightField {
                _data: height_field,
            },
        );
        Ok(shape_id)
    }

    /// Tries to attach a compound shape to a static body.
    pub fn try_create_compound_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        compound: Compound,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        let mut prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        if def.sensor {
            return Err(validation::invalid(
                "compound_shape.sensor",
                crate::error::InvalidValueReason::InvalidCombination,
            ));
        }
        if self.try_body_type(body_id)? != BodyType::Static {
            return Err(Error::InvalidArgument);
        }
        self.resources
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let pending = self.ledger.reserve_shape()?;
        let compound_ptr = compound.as_ptr();
        let _guard = self.lock_body_checked(body_id)?;
        let mut raw_def = prepared.lower_locked();
        let raw = unsafe {
            ffi::b3CreateBakedCompoundShape(body_id.into_raw(), &mut raw_def, compound_ptr)
        };
        if !unsafe { ffi::b3Shape_IsValid(raw) } {
            return Err(Error::CreateShapeFailed);
        }
        let shape_id = self.ledger.publish_shape(raw, pending);
        drop(_guard);
        self.resources
            .insert(shape_id, ShapeResource::Compound { _data: compound });
        Ok(shape_id)
    }
}
