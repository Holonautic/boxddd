use super::creation_transaction::{
    BodyNative, NativeCreationInput, ShapeNative, finish_native_creation, observed_shape_parent,
};
use super::ledger::{BodyResource, PendingResource, PendingShape, WorldLedger};
use super::*;
use crate::types::Transform;

#[derive(Clone, Copy)]
enum ExpectedShapeType {
    Exact(ShapeType),
    CapsuleOrSphere,
}

enum ShapeCreation<'a> {
    Sphere(&'a Sphere),
    BoxHull(&'a BoxHull),
    Capsule(&'a Capsule),
    Hull(&'a Hull),
    TransformedHull {
        hull: &'a Hull,
        transform: Transform,
        scale: Vec3,
    },
    Mesh {
        scale: Vec3,
    },
    HeightField,
    Compound,
}

impl ExpectedShapeType {
    fn accepts(self, observed: Option<ShapeType>) -> bool {
        match self {
            Self::Exact(expected) => observed == Some(expected),
            Self::CapsuleOrSphere => {
                matches!(observed, Some(ShapeType::Capsule | ShapeType::Sphere))
            }
        }
    }
}

struct ShapeBackingGuard<'a> {
    backing: Option<ShapeResource>,
    owner_poisoned: &'a Cell<bool>,
    quarantine: &'a mut Vec<ShapeResource>,
}

struct ShapeFinishContext<'a, 'owner> {
    target_world: ffi::b3WorldId,
    poisoned: &'a Cell<bool>,
    ledger: &'a mut WorldLedger,
    backing: &'a mut ShapeBackingGuard<'owner>,
}

impl<'a> ShapeBackingGuard<'a> {
    fn new(
        backing: Option<ShapeResource>,
        owner_poisoned: &'a Cell<bool>,
        quarantine: &'a mut Vec<ShapeResource>,
    ) -> Self {
        Self {
            backing,
            owner_poisoned,
            quarantine,
        }
    }

    fn slot(&mut self) -> &mut Option<ShapeResource> {
        &mut self.backing
    }

    fn resource(&self) -> Option<&ShapeResource> {
        self.backing.as_ref()
    }
}

impl Drop for ShapeBackingGuard<'_> {
    fn drop(&mut self) {
        if (self.owner_poisoned.get() || std::thread::panicking())
            && let Some(backing) = self.backing.take()
        {
            debug_assert!(self.quarantine.len() < self.quarantine.capacity());
            self.quarantine.push(backing);
        }
    }
}

impl World {
    /// Tries to create a body in this world.
    pub fn create_body(&mut self, def: BodyDef) -> Result<BodyId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let expected_type = def.body_type;
        let prepared = def.prepare()?;
        let pending = self.state_mut().ledger.reserve_body()?;
        let _call = self.enter_call()?;
        self.check_world_valid()?;
        let raw = prepared.create(self.raw());
        self.finish_body_creation(raw, pending, expected_type)
    }

    /// Tries to attach a sphere shape to a body.
    pub fn create_sphere_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        sphere: &Sphere,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        sphere.validate()?;
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            None,
            ExpectedShapeType::Exact(ShapeType::Sphere),
            ShapeCreation::Sphere(sphere),
        )
    }

    /// Tries to attach a box-hull shape to a body.
    pub fn create_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &BoxHull,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            None,
            ExpectedShapeType::Exact(ShapeType::Hull),
            ShapeCreation::BoxHull(hull),
        )
    }

    /// Tries to attach a capsule shape to a body.
    pub fn create_capsule_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        capsule: &Capsule,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        capsule.validate()?;
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            None,
            ExpectedShapeType::CapsuleOrSphere,
            ShapeCreation::Capsule(capsule),
        )
    }

    /// Tries to attach an owned convex hull resource to a body.
    pub fn create_created_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &Hull,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            None,
            ExpectedShapeType::Exact(ShapeType::Hull),
            ShapeCreation::Hull(hull),
        )
    }

    /// Tries to attach a transformed owned convex hull resource to a body.
    pub fn create_transformed_hull_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        hull: &Hull,
        transform: impl Into<crate::types::Transform>,
        scale: impl Into<Vec3>,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        let transform = transform.into();
        transform.validate()?;
        let scale = scale.into().validate()?;
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            None,
            ExpectedShapeType::Exact(ShapeType::Hull),
            ShapeCreation::TransformedHull {
                hull,
                transform,
                scale,
            },
        )
    }

    /// Tries to attach a triangle mesh shape to a static body.
    pub fn create_mesh_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        mesh: MeshData,
        scale: impl Into<Vec3>,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::PerTriangle)?;
        let scale = validate_mesh_scale(scale.into())?;
        if self.body_type(body_id)? != BodyType::Static {
            return Err(validation::invalid(
                "mesh_shape.body_type",
                crate::error::InvalidValueReason::InvalidCombination,
            ));
        }
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            Some(ShapeResource::Mesh { _data: mesh }),
            ExpectedShapeType::Exact(ShapeType::Mesh),
            ShapeCreation::Mesh { scale },
        )
    }

    /// Tries to attach a height-field shape to a static body.
    pub fn create_height_field_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        height_field: HeightField,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::PerTriangle)?;
        if self.body_type(body_id)? != BodyType::Static {
            return Err(validation::invalid(
                "height_field_shape.body_type",
                crate::error::InvalidValueReason::InvalidCombination,
            ));
        }
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            Some(ShapeResource::HeightField {
                _data: height_field,
            }),
            ExpectedShapeType::Exact(ShapeType::HeightField),
            ShapeCreation::HeightField,
        )
    }

    /// Tries to attach a compound shape to a static body.
    pub fn create_compound_shape(
        &mut self,
        body_id: BodyId,
        def: &ShapeDef,
        compound: Compound,
    ) -> Result<ShapeId> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let prepared = def.prepare(ShapeMaterialUsage::BaseOnly)?;
        if def.sensor {
            return Err(validation::invalid(
                "compound_shape.sensor",
                crate::error::InvalidValueReason::InvalidCombination,
            ));
        }
        if self.body_type(body_id)? != BodyType::Static {
            return Err(validation::invalid(
                "compound_shape.body_type",
                crate::error::InvalidValueReason::InvalidCombination,
            ));
        }
        let pending = self.state_mut().ledger.reserve_shape(body_id)?;
        self.create_shape_from_prepared(
            body_id,
            prepared,
            pending,
            Some(ShapeResource::Compound { _data: compound }),
            ExpectedShapeType::Exact(ShapeType::Compound),
            ShapeCreation::Compound,
        )
    }

    fn create_shape_from_prepared(
        &mut self,
        body_id: BodyId,
        prepared: PreparedShapeDef<'_>,
        pending: PendingShape,
        backing: Option<ShapeResource>,
        expected_type: ExpectedShapeType,
        creation: ShapeCreation<'_>,
    ) -> Result<ShapeId> {
        if backing.is_some() {
            self.state_mut()
                .backing_quarantine
                .try_reserve(1)
                .map_err(|_| Error::AllocationFailed)?;
        }
        let _call = self.enter_body_call(body_id)?;
        let update_body_mass = prepared.update_body_mass();
        let target_world = self.raw();
        let WorldState {
            poisoned,
            ledger,
            backing_quarantine,
            ..
        } = self.state_mut();
        let mut backing = ShapeBackingGuard::new(backing, poisoned, backing_quarantine);
        let raw_body = body_id.into_raw();
        let raw = match creation {
            ShapeCreation::Sphere(sphere) => prepared.create_sphere(raw_body, sphere),
            ShapeCreation::BoxHull(hull) => prepared.create_box_hull(raw_body, hull),
            ShapeCreation::Capsule(capsule) => prepared.create_capsule(raw_body, capsule),
            ShapeCreation::Hull(hull) => prepared.create_hull(raw_body, hull),
            ShapeCreation::TransformedHull {
                hull,
                transform,
                scale,
            } => prepared.create_transformed_hull(raw_body, hull, transform, scale),
            ShapeCreation::Mesh { scale } => {
                let Some(ShapeResource::Mesh { _data: mesh }) = backing.resource() else {
                    unreachable!("mesh creation requires mesh backing")
                };
                prepared.create_mesh(raw_body, mesh, scale)
            }
            ShapeCreation::HeightField => {
                let Some(ShapeResource::HeightField {
                    _data: height_field,
                }) = backing.resource()
                else {
                    unreachable!("height-field creation requires height-field backing")
                };
                prepared.create_height_field(raw_body, height_field)
            }
            ShapeCreation::Compound => {
                let Some(ShapeResource::Compound { _data: compound }) = backing.resource() else {
                    unreachable!("compound creation requires compound backing")
                };
                prepared.create_compound(raw_body, compound)
            }
        };
        let result = finish_shape_creation(
            ShapeFinishContext {
                target_world,
                poisoned,
                ledger,
                backing: &mut backing,
            },
            raw,
            pending,
            update_body_mass,
            expected_type,
        );
        drop(_call);
        drop(backing);
        result
    }

    fn finish_body_creation(
        &mut self,
        raw: ffi::b3BodyId,
        pending: PendingResource<BodyResource>,
        expected_type: BodyType,
    ) -> Result<BodyId> {
        let target_world = self.raw();
        let WorldState {
            poisoned, ledger, ..
        } = self.state_mut();
        let identity = ledger.classify_body(raw);
        finish_native_creation::<BodyNative, _, _, _>(
            NativeCreationInput::new(raw, (), target_world, poisoned, identity),
            ledger,
            |ledger, raw, available| ledger.bind_body(raw, available, pending),
            |_, raw| {
                if BodyType::from_raw(unsafe { ffi::b3Body_GetType(raw) }) == Some(expected_type) {
                    Ok(())
                } else {
                    Err(Error::NativeFailure)
                }
            },
            |ledger, _, bound| ledger.publish_body(bound),
        )
    }
}

fn finish_shape_creation(
    context: ShapeFinishContext<'_, '_>,
    raw: ffi::b3ShapeId,
    pending: PendingShape,
    update_body_mass: bool,
    expected_type: ExpectedShapeType,
) -> Result<ShapeId> {
    let ShapeFinishContext {
        target_world,
        poisoned,
        ledger,
        backing,
    } = context;
    let identity = ledger.classify_shape(raw);
    let mut context = (ledger, backing);
    finish_native_creation::<ShapeNative, _, _, _>(
        NativeCreationInput::new(raw, update_body_mass, target_world, poisoned, identity),
        &mut context,
        |context, raw, available| {
            let (ledger, _) = context;
            ledger.validate_shape_binding(&pending, observed_shape_parent(raw))?;
            ledger.bind_shape(raw, available, pending)
        },
        |_, raw| {
            if expected_type.accepts(ShapeType::from_raw(unsafe { ffi::b3Shape_GetType(raw) })) {
                Ok(())
            } else {
                Err(Error::NativeFailure)
            }
        },
        |context, _, bound| {
            let (ledger, backing) = context;
            ledger.publish_shape(bound, backing.slot())
        },
    )
}
