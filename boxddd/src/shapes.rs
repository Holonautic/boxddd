use crate::core::callback_state;
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::core::callback_state::LocalCallbackState;
use crate::core::foundation::{Foundation, OrdinaryLease};
use crate::core::validation;
use crate::error::{Error, InvalidValueReason, Result};
use crate::types::{Aabb, Filter, Transform, Vec3};
use boxddd_sys::ffi;
#[cfg(test)]
use std::cell::RefCell;
use std::ffi::CString;
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;
use std::slice;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShapeDropEvent {
    NativeShape,
    MeshBacking,
    HeightFieldBacking,
    CompoundBacking,
}

#[cfg(test)]
thread_local! {
    static SHAPE_DROP_TRACE: RefCell<Option<Vec<ShapeDropEvent>>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn begin_shape_drop_trace() {
    SHAPE_DROP_TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
}

#[cfg(test)]
pub(crate) fn record_shape_drop(event: ShapeDropEvent) {
    SHAPE_DROP_TRACE.with(|trace| {
        if let Some(events) = trace.borrow_mut().as_mut() {
            events.push(event);
        }
    });
}

#[cfg(test)]
pub(crate) fn take_shape_drop_trace() -> Vec<ShapeDropEvent> {
    SHAPE_DROP_TRACE.with(|trace| trace.borrow_mut().take().unwrap_or_default())
}

fn cleanup_local_owner<T: 'static>(inner: T, destroy: fn(T)) {
    if callback_state::in_callback() {
        callback_state::defer_local_cleanup_or_retain(move || destroy(inner));
    } else {
        destroy(inner);
    }
}

fn create_native_owner<T>(create: impl FnOnce() -> *mut T) -> Result<(NonNull<T>, OrdinaryLease)> {
    let foundation_lease = Foundation::get()?.acquire_ordinary()?;
    let raw = NonNull::new(create()).ok_or(Error::NativeFailure)?;
    Ok((raw, foundation_lease))
}

/// Height-field material marker that makes a cell behave as a hole.
pub const HEIGHT_FIELD_HOLE: u8 = ffi::B3_HEIGHT_FIELD_HOLE as u8;
/// Maximum number of material slots a compound mesh child can reference.
pub const MAX_COMPOUND_MESH_MATERIALS: usize = ffi::B3_MAX_COMPOUND_MESH_MATERIALS as usize;

#[repr(C)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
/// Surface properties assigned to a shape or compound child.
pub struct SurfaceMaterial {
    /// Coulomb friction coefficient used by contacts.
    pub friction: f32,
    /// Restitution coefficient used to make contacts bounce.
    pub restitution: f32,
    /// Resistance applied to rolling contacts.
    pub rolling_resistance: f32,
    /// Tangential surface speed used for conveyor-style contacts.
    pub tangent_velocity: Vec3,
    /// User-defined material identifier carried through contact data.
    pub user_material_id: u64,
    /// Optional debug-render color in Box3D's packed color format.
    pub custom_color: u32,
}

impl Default for SurfaceMaterial {
    fn default() -> Self {
        Self {
            friction: 0.6,
            restitution: 0.0,
            rolling_resistance: 0.0,
            tangent_velocity: Vec3::ZERO,
            user_material_id: 0,
            custom_color: 0,
        }
    }
}

impl SurfaceMaterial {
    #[inline]
    /// Converts a raw Box3D surface material into the safe wrapper.
    pub fn from_raw(raw: ffi::b3SurfaceMaterial) -> Self {
        Self {
            friction: raw.friction,
            restitution: raw.restitution,
            rolling_resistance: raw.rollingResistance,
            tangent_velocity: Vec3::from_raw(raw.tangentVelocity),
            user_material_id: raw.userMaterialId,
            custom_color: raw.customColor,
        }
    }

    #[inline]
    /// Converts this material into the raw Box3D representation.
    pub fn into_raw(self) -> ffi::b3SurfaceMaterial {
        ffi::b3SurfaceMaterial {
            friction: self.friction,
            restitution: self.restitution,
            rollingResistance: self.rolling_resistance,
            tangentVelocity: self.tangent_velocity.into_raw(),
            userMaterialId: self.user_material_id,
            customColor: self.custom_color,
            padding: 0,
        }
    }

    /// Validates that material coefficients are finite and non-negative.
    pub fn validate(self) -> Result<()> {
        validation::nonnegative("surface_material.friction", self.friction)?;
        validation::nonnegative("surface_material.restitution", self.restitution)?;
        validation::nonnegative(
            "surface_material.rolling_resistance",
            self.rolling_resistance,
        )?;
        validation::vec3("surface_material.tangent_velocity", self.tangent_velocity)
    }
}

#[derive(Clone, Debug)]
/// Shared construction parameters used when attaching a shape to a body.
pub struct ShapeDef {
    /// Optional UTF-8 debug name copied by Box3D during creation.
    pub name: Option<String>,
    /// Per-triangle materials copied by Box3D during mesh creation.
    pub materials: Vec<SurfaceMaterial>,
    /// Material used by convex shapes and as the mesh fallback.
    pub base_material: SurfaceMaterial,
    /// Shape density, usually in kilograms per cubic meter.
    pub density: f32,
    /// Scale applied when the shape is affected by an explosion.
    pub explosion_scale: f32,
    /// Collision filter for contacts and queries.
    pub filter: Filter,
    /// Whether this shape participates in custom filtering callbacks.
    pub enable_custom_filtering: bool,
    /// Whether this shape is a non-solid sensor.
    pub sensor: bool,
    /// Whether sensor overlap events are produced.
    pub enable_sensor_events: bool,
    /// Whether contact begin and end events are produced.
    pub enable_contact_events: bool,
    /// Whether high-speed contact hit events are produced.
    pub enable_hit_events: bool,
    /// Whether pre-solve callbacks are enabled.
    pub enable_pre_solve_events: bool,
    /// Whether static shape creation immediately scans for contacts.
    pub invoke_contact_creation: bool,
    /// Whether creation updates the owning body's mass data.
    pub update_body_mass: bool,
    /// Whether speculative collision is enabled.
    pub enable_speculative_contact: bool,
}

impl ShapeDef {
    pub(crate) fn with_length_units_per_meter(length_units: f32) -> Self {
        Self {
            name: None,
            materials: Vec::new(),
            base_material: SurfaceMaterial::default(),
            density: 1000.0 / (length_units * length_units * length_units),
            explosion_scale: 1.0,
            filter: Filter {
                category_bits: u64::MAX,
                mask_bits: u64::MAX,
                group_index: 0,
            },
            enable_custom_filtering: false,
            sensor: false,
            enable_sensor_events: false,
            enable_contact_events: false,
            enable_hit_events: false,
            enable_pre_solve_events: false,
            invoke_contact_creation: true,
            update_body_mass: true,
            enable_speculative_contact: true,
        }
    }

    /// Returns the collision filter configured for this shape.
    pub const fn filter(&self) -> Filter {
        self.filter
    }

    /// Returns the base surface material configured for this shape.
    pub const fn surface_material(&self) -> SurfaceMaterial {
        self.base_material
    }

    /// Validates numeric fields and nested material data.
    pub fn validate(&self) -> Result<()> {
        if let Some(name) = self.name.as_deref() {
            validation::c_string_value("shape.name", name)?;
        }
        validation::count_i32("shape.materials", self.materials.len())?;
        self.base_material.validate()?;
        for material in &self.materials {
            material.validate()?;
        }
        validation::nonnegative("shape.density", self.density)?;
        validation::finite("shape.explosion_scale", self.explosion_scale)
    }

    pub(crate) fn prepare(
        &self,
        material_usage: ShapeMaterialUsage,
    ) -> Result<PreparedShapeDef<'_>> {
        self.validate()?;
        let name = validation::optional_c_string("shape.name", self.name.as_deref())?;
        let materials = match material_usage {
            ShapeMaterialUsage::BaseOnly => Vec::new(),
            ShapeMaterialUsage::PerTriangle => self
                .materials
                .iter()
                .copied()
                .map(SurfaceMaterial::into_raw)
                .collect(),
        };
        Ok(PreparedShapeDef {
            def: self,
            name,
            materials,
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum ShapeMaterialUsage {
    BaseOnly,
    PerTriangle,
}

pub(crate) struct PreparedShapeDef<'a> {
    def: &'a ShapeDef,
    name: Option<CString>,
    materials: Vec<ffi::b3SurfaceMaterial>,
}

impl PreparedShapeDef<'_> {
    pub(crate) fn update_body_mass(&self) -> bool {
        self.def.update_body_mass
    }

    pub(crate) fn create_sphere(self, body: ffi::b3BodyId, sphere: &Sphere) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe { ffi::b3CreateSphereShape(body, raw, sphere.raw()) })
    }

    pub(crate) fn create_box_hull(self, body: ffi::b3BodyId, hull: &BoxHull) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe { ffi::b3CreateHullShape(body, raw, hull.hull_data()) })
    }

    pub(crate) fn create_capsule(self, body: ffi::b3BodyId, capsule: &Capsule) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe { ffi::b3CreateCapsuleShape(body, raw, capsule.raw()) })
    }

    pub(crate) fn create_hull(self, body: ffi::b3BodyId, hull: &Hull) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe { ffi::b3CreateHullShape(body, raw, hull.as_ptr()) })
    }

    pub(crate) fn create_transformed_hull(
        self,
        body: ffi::b3BodyId,
        hull: &Hull,
        transform: Transform,
        scale: Vec3,
    ) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe {
            ffi::b3CreateTransformedHullShape(
                body,
                raw,
                hull.as_ptr(),
                transform.into_raw(),
                scale.into_raw(),
            )
        })
    }

    pub(crate) fn create_mesh(
        self,
        body: ffi::b3BodyId,
        mesh: &MeshData,
        scale: Vec3,
    ) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe {
            ffi::b3CreateMeshShape(body, raw, mesh.as_ptr(), scale.into_raw())
        })
    }

    pub(crate) fn create_height_field(
        self,
        body: ffi::b3BodyId,
        height_field: &HeightField,
    ) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe {
            ffi::b3CreateHeightFieldShape(body, raw, height_field.as_ptr())
        })
    }

    pub(crate) fn create_compound(
        self,
        body: ffi::b3BodyId,
        compound: &Compound,
    ) -> ffi::b3ShapeId {
        self.invoke(|raw| unsafe { ffi::b3CreateBakedCompoundShape(body, raw, compound.as_ptr()) })
    }

    fn invoke(
        mut self,
        create: impl FnOnce(&mut ffi::b3ShapeDef) -> ffi::b3ShapeId,
    ) -> ffi::b3ShapeId {
        let mut raw = unsafe { ffi::b3DefaultShapeDef() };
        raw.name = self
            .name
            .as_ref()
            .map_or(std::ptr::null(), |name| name.as_ptr());
        raw.userData = std::ptr::null_mut();
        raw.materials = if self.materials.is_empty() {
            std::ptr::null_mut()
        } else {
            self.materials.as_mut_ptr()
        };
        raw.materialCount = i32::try_from(self.materials.len())
            .expect("validated shape material count no longer fits in i32");
        raw.baseMaterial = self.def.base_material.into_raw();
        raw.density = self.def.density;
        raw.explosionScale = self.def.explosion_scale;
        raw.filter = self.def.filter.into_raw();
        raw.enableCustomFiltering = self.def.enable_custom_filtering;
        raw.isSensor = self.def.sensor;
        raw.enableSensorEvents = self.def.enable_sensor_events;
        raw.enableContactEvents = self.def.enable_contact_events;
        raw.enableHitEvents = self.def.enable_hit_events;
        raw.enablePreSolveEvents = self.def.enable_pre_solve_events;
        raw.invokeContactCreation = self.def.invoke_contact_creation;
        raw.updateBodyMass = self.def.update_body_mass;
        raw.enableSpeculativeContact = self.def.enable_speculative_contact;
        create(&mut raw)
    }
}

#[cfg(test)]
mod prepared_definition_tests {
    use super::*;

    #[test]
    fn prepared_shape_exposes_only_complete_native_create_operations() {
        let _: fn(PreparedShapeDef<'static>, ffi::b3BodyId, &'static Sphere) -> ffi::b3ShapeId =
            PreparedShapeDef::create_sphere;
        let _: fn(
            PreparedShapeDef<'static>,
            ffi::b3BodyId,
            &'static MeshData,
            Vec3,
        ) -> ffi::b3ShapeId = PreparedShapeDef::create_mesh;
    }
}

#[derive(Clone, Debug)]
/// Builder for `ShapeDef`.
pub struct ShapeDefBuilder {
    def: ShapeDef,
}

impl ShapeDefBuilder {
    pub(crate) fn from_def(def: ShapeDef) -> Self {
        Self { def }
    }

    #[inline]
    /// Sets the shape density used for mass properties.
    pub fn density(mut self, density: f32) -> Self {
        self.def.density = density;
        self
    }

    #[inline]
    /// Sets the base material friction coefficient.
    pub fn friction(mut self, friction: f32) -> Self {
        self.def.base_material.friction = friction;
        self
    }

    #[inline]
    /// Sets the base material restitution coefficient.
    pub fn restitution(mut self, restitution: f32) -> Self {
        self.def.base_material.restitution = restitution;
        self
    }

    #[inline]
    /// Sets the collision filter.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.def.filter = filter;
        self
    }

    #[inline]
    /// Replaces the complete base surface material.
    pub fn surface_material(mut self, material: SurfaceMaterial) -> Self {
        self.def.base_material = material;
        self
    }

    #[inline]
    /// Sets the user material id on the base surface material.
    pub fn user_material_id(mut self, user_material_id: u64) -> Self {
        self.def.base_material.user_material_id = user_material_id;
        self
    }

    #[inline]
    /// Marks the shape as a sensor instead of a solid collider.
    pub fn sensor(mut self, is_sensor: bool) -> Self {
        self.def.sensor = is_sensor;
        self
    }

    #[inline]
    /// Enables begin/end sensor overlap events for this shape.
    pub fn enable_sensor_events(mut self, enabled: bool) -> Self {
        self.def.enable_sensor_events = enabled;
        self
    }

    #[inline]
    /// Enables contact begin/end events for this shape.
    pub fn enable_contact_events(mut self, enabled: bool) -> Self {
        self.def.enable_contact_events = enabled;
        self
    }

    #[inline]
    /// Enables high-speed hit events for this shape.
    pub fn enable_hit_events(mut self, enabled: bool) -> Self {
        self.def.enable_hit_events = enabled;
        self
    }

    #[inline]
    /// Enables pre-solve callbacks for this shape.
    pub fn enable_pre_solve_events(mut self, enabled: bool) -> Self {
        self.def.enable_pre_solve_events = enabled;
        self
    }

    #[inline]
    /// Enables custom filtering callbacks for this shape.
    pub fn enable_custom_filtering(mut self, enabled: bool) -> Self {
        self.def.enable_custom_filtering = enabled;
        self
    }

    /// Sets the optional UTF-8 debug name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.def.name = Some(name.into());
        self
    }

    /// Sets materials copied for per-triangle mesh use.
    pub fn materials(mut self, materials: impl IntoIterator<Item = SurfaceMaterial>) -> Self {
        self.def.materials = materials.into_iter().collect();
        self
    }

    /// Sets the explosion impulse scale.
    pub fn explosion_scale(mut self, scale: f32) -> Self {
        self.def.explosion_scale = scale;
        self
    }

    /// Controls whether static creation immediately scans for contacts.
    pub fn invoke_contact_creation(mut self, enabled: bool) -> Self {
        self.def.invoke_contact_creation = enabled;
        self
    }

    /// Controls whether creation updates the owning body's mass data.
    pub fn update_body_mass(mut self, enabled: bool) -> Self {
        self.def.update_body_mass = enabled;
        self
    }

    /// Enables or disables speculative contact generation.
    pub fn enable_speculative_contact(mut self, enabled: bool) -> Self {
        self.def.enable_speculative_contact = enabled;
        self
    }

    #[inline]
    /// Validates and finishes the shape definition.
    pub fn build(self) -> Result<ShapeDef> {
        self.def.validate()?;
        Ok(self.def)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
/// Sphere shape geometry.
pub struct Sphere {
    raw: ffi::b3Sphere,
}

impl Sphere {
    #[inline]
    /// Creates a sphere from a center point and positive radius.
    pub fn new(center: impl Into<Vec3>, radius: f32) -> Self {
        Self {
            raw: ffi::b3Sphere {
                center: center.into().into_raw(),
                radius,
            },
        }
    }

    #[inline]
    /// Wraps a raw Box3D sphere.
    pub const fn from_raw(raw: ffi::b3Sphere) -> Self {
        Self { raw }
    }

    #[inline]
    /// Returns the raw Box3D sphere.
    pub const fn raw(&self) -> &ffi::b3Sphere {
        &self.raw
    }

    /// Validates that the center is finite and the radius is positive.
    pub fn validate(&self) -> Result<()> {
        validation::vec3("sphere.center", Vec3::from_raw(self.raw.center))?;
        validation::positive("sphere.radius", self.raw.radius)
    }
}

impl PartialEq for Sphere {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        raw_vec3_eq(self.raw.center, other.raw.center) && self.raw.radius == other.raw.radius
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
/// Capsule shape geometry defined by two segment endpoints and a radius.
pub struct Capsule {
    raw: ffi::b3Capsule,
}

impl Capsule {
    #[inline]
    /// Creates a capsule from two centerline endpoints and a positive radius.
    pub fn new(center1: impl Into<Vec3>, center2: impl Into<Vec3>, radius: f32) -> Self {
        Self {
            raw: ffi::b3Capsule {
                center1: center1.into().into_raw(),
                center2: center2.into().into_raw(),
                radius,
            },
        }
    }

    #[inline]
    /// Wraps a raw Box3D capsule.
    pub const fn from_raw(raw: ffi::b3Capsule) -> Self {
        Self { raw }
    }

    #[inline]
    /// Returns the raw Box3D capsule.
    pub const fn raw(&self) -> &ffi::b3Capsule {
        &self.raw
    }

    /// Validates that endpoints are finite and the radius is positive.
    pub fn validate(&self) -> Result<()> {
        validation::vec3("capsule.center1", Vec3::from_raw(self.raw.center1))?;
        validation::vec3("capsule.center2", Vec3::from_raw(self.raw.center2))?;
        validation::positive("capsule.radius", self.raw.radius)
    }
}

impl PartialEq for Capsule {
    fn eq(&self, other: &Self) -> bool {
        raw_vec3_eq(self.raw.center1, other.raw.center1)
            && raw_vec3_eq(self.raw.center2, other.raw.center2)
            && self.raw.radius == other.raw.radius
    }
}

#[derive(Copy, Clone, Debug)]
/// Convex hull data for box-shaped hulls generated by Box3D.
pub struct BoxHull {
    raw: ffi::b3BoxHull,
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Result of scaling box half-widths and transform together.
pub struct ScaledBox {
    /// Adjusted box half-widths after scaling and clamping.
    pub half_widths: Vec3,
    /// Adjusted transform that keeps the scaled box representation stable.
    pub transform: Transform,
}

impl BoxHull {
    #[inline]
    /// Creates an axis-aligned cube hull from a positive half-width.
    pub fn cube(half_width: f32) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        validation::positive("box_hull.half_width", half_width)?;
        let _call = Foundation::enter_transient_call()?;
        Ok(Self {
            raw: unsafe { ffi::b3MakeCubeHull(half_width) },
        })
    }

    #[inline]
    /// Creates an axis-aligned box hull from positive half-widths.
    pub fn new(hx: f32, hy: f32, hz: f32) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let half_widths = validate_box_half_widths(Vec3::new(hx, hy, hz))?;
        let _call = Foundation::enter_transient_call()?;
        Ok(Self {
            raw: unsafe { ffi::b3MakeBoxHull(half_widths.x, half_widths.y, half_widths.z) },
        })
    }

    #[inline]
    /// Creates an axis-aligned box hull offset from the local origin.
    pub fn offset(hx: f32, hy: f32, hz: f32, offset: impl Into<Vec3>) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let half_widths = validate_box_half_widths(Vec3::new(hx, hy, hz))?;
        let offset = offset.into();
        validation::vec3("box_hull.offset", offset)?;
        let _call = Foundation::enter_transient_call()?;
        Ok(Self {
            raw: unsafe {
                ffi::b3MakeOffsetBoxHull(
                    half_widths.x,
                    half_widths.y,
                    half_widths.z,
                    offset.into_raw(),
                )
            },
        })
    }

    #[inline]
    /// Creates a box hull with a local transform baked into the hull data.
    pub fn transformed(hx: f32, hy: f32, hz: f32, transform: Transform) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let half_widths = validate_box_half_widths(Vec3::new(hx, hy, hz))?;
        validation::transform("box_hull.transform", transform)?;
        let _call = Foundation::enter_transient_call()?;
        Ok(Self {
            raw: unsafe {
                ffi::b3MakeTransformedBoxHull(
                    half_widths.x,
                    half_widths.y,
                    half_widths.z,
                    transform.into_raw(),
                )
            },
        })
    }

    #[inline]
    /// Creates a box hull after applying a post scale.
    pub fn scaled(
        half_widths: impl Into<Vec3>,
        transform: Transform,
        post_scale: impl Into<Vec3>,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let half_widths = validate_box_half_widths(half_widths.into())?;
        validation::transform("box_hull.transform", transform)?;
        let post_scale = post_scale.into();
        validation::vec3("box_hull.post_scale", post_scale)?;
        let _call = Foundation::enter_transient_call()?;
        Ok(Self {
            raw: unsafe {
                ffi::b3MakeScaledBoxHull(
                    half_widths.into_raw(),
                    transform.into_raw(),
                    post_scale.into_raw(),
                )
            },
        })
    }

    /// Scales box half-widths and transform while preserving a minimum half-width.
    pub fn scale_box(
        half_widths: impl Into<Vec3>,
        transform: Transform,
        post_scale: impl Into<Vec3>,
        min_half_width: f32,
    ) -> Result<ScaledBox> {
        callback_state::check_not_in_callback()?;
        let mut half_widths = validate_box_half_widths(half_widths.into())?.into_raw();
        validation::transform("box_hull.transform", transform)?;
        let mut transform = transform.into_raw();
        let post_scale = post_scale.into();
        validation::vec3("box_hull.post_scale", post_scale)?;
        validation::positive("box_hull.min_half_width", min_half_width)?;
        let _call = Foundation::enter_transient_call()?;

        unsafe {
            ffi::b3ScaleBox(
                &mut half_widths,
                &mut transform,
                post_scale.into_raw(),
                min_half_width,
            );
        }

        let half_widths = Vec3::from_raw(half_widths);
        let transform = Transform::from_raw(transform);
        validation::vec3("box_hull.scaled_half_widths", half_widths)
            .map_err(|_| Error::NativeFailure)?;
        validation::transform("box_hull.scaled_transform", transform)
            .map_err(|_| Error::NativeFailure)?;
        Ok(ScaledBox {
            half_widths,
            transform,
        })
    }

    #[inline]
    /// Returns the raw Box3D box hull.
    pub const fn raw(&self) -> &ffi::b3BoxHull {
        &self.raw
    }

    #[inline]
    /// Returns the hull portion of the box hull data.
    pub const fn hull_data(&self) -> &ffi::b3HullData {
        &self.raw.base
    }
}

impl PartialEq for BoxHull {
    fn eq(&self, other: &Self) -> bool {
        raw_hull_data_eq(&self.raw.base, &other.raw.base)
            && raw_hull_vertices_eq(&self.raw.boxVertices, &other.raw.boxVertices)
            && raw_vec3_array_eq(&self.raw.boxPoints, &other.raw.boxPoints)
            && raw_hull_edges_eq(&self.raw.boxEdges, &other.raw.boxEdges)
            && raw_hull_faces_eq(&self.raw.boxFaces, &other.raw.boxFaces)
            && raw_planes_eq(&self.raw.boxPlanes, &other.raw.boxPlanes)
    }
}

#[inline]
fn raw_vec3_eq(a: ffi::b3Vec3, b: ffi::b3Vec3) -> bool {
    a.x == b.x && a.y == b.y && a.z == b.z
}

#[inline]
fn raw_matrix3_eq(a: &ffi::b3Matrix3, b: &ffi::b3Matrix3) -> bool {
    raw_vec3_eq(a.cx, b.cx) && raw_vec3_eq(a.cy, b.cy) && raw_vec3_eq(a.cz, b.cz)
}

#[inline]
fn raw_aabb_eq(a: &ffi::b3AABB, b: &ffi::b3AABB) -> bool {
    raw_vec3_eq(a.lowerBound, b.lowerBound) && raw_vec3_eq(a.upperBound, b.upperBound)
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
fn clamp_height_field_query_bounds(a: Aabb, b: ffi::b3AABB) -> Result<Option<Aabb>> {
    let b = Aabb::from_raw(b).validate()?;
    if a.upper_bound.x < b.lower_bound.x
        || a.lower_bound.x > b.upper_bound.x
        || a.upper_bound.y < b.lower_bound.y
        || a.lower_bound.y > b.upper_bound.y
        || a.upper_bound.z < b.lower_bound.z
        || a.lower_bound.z > b.upper_bound.z
    {
        Ok(None)
    } else {
        Aabb {
            lower_bound: Vec3::new(
                a.lower_bound.x.max(b.lower_bound.x),
                a.lower_bound.y,
                a.lower_bound.z.max(b.lower_bound.z),
            ),
            upper_bound: Vec3::new(
                a.upper_bound.x.min(b.upper_bound.x),
                a.upper_bound.y,
                a.upper_bound.z.min(b.upper_bound.z),
            ),
        }
        .validate()
        .map(Some)
    }
}

fn raw_hull_data_eq(a: &ffi::b3HullData, b: &ffi::b3HullData) -> bool {
    a.version == b.version
        && a.byteCount == b.byteCount
        && a.hash == b.hash
        && raw_aabb_eq(&a.aabb, &b.aabb)
        && a.surfaceArea == b.surfaceArea
        && a.volume == b.volume
        && a.innerRadius == b.innerRadius
        && raw_vec3_eq(a.center, b.center)
        && raw_matrix3_eq(&a.centralInertia, &b.centralInertia)
        && a.vertexCount == b.vertexCount
        && a.vertexOffset == b.vertexOffset
        && a.pointOffset == b.pointOffset
        && a.edgeCount == b.edgeCount
        && a.edgeOffset == b.edgeOffset
        && a.faceCount == b.faceCount
        && a.faceOffset == b.faceOffset
        && a.planeOffset == b.planeOffset
}

fn raw_hull_vertices_eq(a: &[ffi::b3HullVertex; 8], b: &[ffi::b3HullVertex; 8]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.edge == b.edge)
}

fn raw_vec3_array_eq(a: &[ffi::b3Vec3; 8], b: &[ffi::b3Vec3; 8]) -> bool {
    a.iter().zip(b).all(|(a, b)| raw_vec3_eq(*a, *b))
}

fn raw_hull_edges_eq(a: &[ffi::b3HullHalfEdge; 24], b: &[ffi::b3HullHalfEdge; 24]) -> bool {
    a.iter().zip(b).all(|(a, b)| {
        a.next == b.next && a.twin == b.twin && a.origin == b.origin && a.face == b.face
    })
}

fn raw_hull_faces_eq(a: &[ffi::b3HullFace; 6], b: &[ffi::b3HullFace; 6]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.edge == b.edge)
}

fn raw_planes_eq(a: &[ffi::b3Plane; 6], b: &[ffi::b3Plane; 6]) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| raw_vec3_eq(a.normal, b.normal) && a.offset == b.offset)
}

fn triangle_area_squared(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let ab = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let ac = Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z);
    let cross = Vec3::new(
        ab.y * ac.z - ab.z * ac.y,
        ab.z * ac.x - ab.x * ac.z,
        ab.x * ac.y - ab.y * ac.x,
    );
    cross.x * cross.x + cross.y * cross.y + cross.z * cross.z
}

fn min_max_finite(values: &[f32]) -> Option<(f32, f32)> {
    let mut iter = values.iter().copied();
    let first = iter.next()?;
    if !first.is_finite() {
        return None;
    }
    let mut min = first;
    let mut max = first;
    for value in iter {
        if !value.is_finite() {
            return None;
        }
        min = min.min(value);
        max = max.max(value);
    }
    Some((min, max))
}

pub(crate) fn validate_mesh_scale(scale: Vec3) -> Result<Vec3> {
    validation::vec3("shape.scale", scale)?;
    if scale.x.abs() <= f32::EPSILON
        || scale.y.abs() <= f32::EPSILON
        || scale.z.abs() <= f32::EPSILON
    {
        Err(validation::invalid(
            "shape.scale",
            InvalidValueReason::OutOfRange,
        ))
    } else {
        Ok(scale)
    }
}

fn validate_box_half_widths(half_widths: Vec3) -> Result<Vec3> {
    validate_positive_vec3("box_hull.half_widths", half_widths)
}

fn validate_positive_vec3(context: &'static str, value: Vec3) -> Result<Vec3> {
    validation::vec3(context, value)?;
    if value.x <= 0.0 || value.y <= 0.0 || value.z <= 0.0 {
        Err(validation::invalid(context, InvalidValueReason::OutOfRange))
    } else {
        Ok(value)
    }
}

fn validate_height_field_dimensions(row_count: i32, column_count: i32) -> Result<usize> {
    if row_count < 2 {
        return Err(validation::invalid(
            "height_field.row_count",
            InvalidValueReason::OutOfRange,
        ));
    }
    if column_count < 2 {
        return Err(validation::invalid(
            "height_field.column_count",
            InvalidValueReason::OutOfRange,
        ));
    }
    let sample_count = (row_count as usize)
        .checked_mul(column_count as usize)
        .ok_or_else(|| {
            validation::invalid("height_field.sample_count", InvalidValueReason::OutOfRange)
        })?;
    if sample_count > i32::MAX as usize {
        Err(validation::invalid(
            "height_field.sample_count",
            InvalidValueReason::OutOfRange,
        ))
    } else {
        Ok(sample_count)
    }
}

#[derive(Debug)]
/// Owned convex hull data allocated by Box3D.
///
/// Hull values own native memory and are intentionally not `Send` or `Sync`.
pub struct Hull {
    inner: Option<HullInner>,
}

#[derive(Debug)]
struct HullInner {
    raw: NonNull<ffi::b3HullData>,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl HullInner {
    fn destroy(self) {
        let owner = callback_state::RetainOnUnwind::new(self);
        unsafe { ffi::b3DestroyHull(owner.raw.as_ptr()) };
        owner.finish();
    }
}

impl Hull {
    /// Builds a convex hull from points with an upper bound on generated vertices.
    pub fn from_points(points: impl AsRef<[Vec3]>, max_vertex_count: i32) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let points = points.as_ref();
        let point_count = validation::count_i32("hull.points", points.len())?;
        if points.len() < 4 {
            return Err(validation::invalid(
                "hull.points",
                InvalidValueReason::OutOfRange,
            ));
        }
        if max_vertex_count <= 0 {
            return Err(validation::invalid(
                "hull.max_vertex_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        for point in points {
            validation::vec3("hull.points", *point)?;
        }
        Self::from_native(|| unsafe {
            ffi::b3CreateHull(points.as_ptr().cast(), point_count, max_vertex_count)
        })
    }

    /// Creates a cylinder hull.
    pub fn cylinder(height: f32, radius: f32, y_offset: f32, sides: i32) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        validation::positive("hull.cylinder.height", height)?;
        validation::positive("hull.cylinder.radius", radius)?;
        validation::finite("hull.cylinder.y_offset", y_offset)?;
        if !(3..=32).contains(&sides) {
            return Err(validation::invalid(
                "hull.cylinder.sides",
                InvalidValueReason::OutOfRange,
            ));
        }
        Self::from_native(|| unsafe { ffi::b3CreateCylinder(height, radius, y_offset, sides) })
    }

    /// Creates a cone or truncated cone hull.
    pub fn cone(height: f32, radius1: f32, radius2: f32, slices: i32) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        validation::positive("hull.cone.height", height)?;
        validation::positive("hull.cone.radius1", radius1)?;
        validation::positive("hull.cone.radius2", radius2)?;
        if !(4..=32).contains(&slices) {
            return Err(validation::invalid(
                "hull.cone.slices",
                InvalidValueReason::OutOfRange,
            ));
        }
        Self::from_native(|| unsafe { ffi::b3CreateCone(height, radius1, radius2, slices) })
    }

    /// Creates an irregular rock-like convex hull.
    pub fn rock(radius: f32) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        validation::positive("hull.rock.radius", radius)?;
        Self::from_native(|| unsafe { ffi::b3CreateRock(radius) })
    }

    /// Clones this hull into a new owned Box3D allocation.
    pub fn try_clone(&self) -> Result<Self> {
        Self::from_native(|| unsafe { ffi::b3CloneHull(self.as_ptr()) })
    }

    /// Clones this hull after applying a transform and non-zero scale.
    pub fn clone_transformed(&self, transform: Transform, scale: impl Into<Vec3>) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let transform = transform.validate()?;
        let scale = validate_mesh_scale(scale.into())?;
        Self::from_native(|| unsafe {
            ffi::b3CloneAndTransformHull(self.as_ptr(), transform.into_raw(), scale.into_raw())
        })
    }

    #[inline]
    /// Returns the raw Box3D hull data.
    pub fn as_hull_data(&self) -> &ffi::b3HullData {
        unsafe { self.inner().raw.as_ref() }
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *const ffi::b3HullData {
        self.inner().raw.as_ptr()
    }

    fn from_native(create: impl FnOnce() -> *mut ffi::b3HullData) -> Result<Self> {
        create_native_owner(create)
            .map(|(raw, foundation_lease)| HullInner {
                raw,
                _foundation_lease: foundation_lease,
                _not_send_sync: PhantomData,
            })
            .map(|inner| Self { inner: Some(inner) })
    }

    fn inner(&self) -> &HullInner {
        self.inner
            .as_ref()
            .expect("live hull always owns its complete inner")
    }
}

impl Drop for Hull {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            cleanup_local_owner(inner, HullInner::destroy);
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Options controlling mesh cooking.
pub struct MeshDataOptions {
    /// Vertex welding tolerance used when `weld_vertices` is enabled.
    pub weld_tolerance: f32,
    /// Whether mesh cooking should merge nearby vertices.
    pub weld_vertices: bool,
    /// Whether the cooked mesh BVH should use median splitting.
    pub use_median_split: bool,
    /// Whether Box3D should identify internal and boundary edges.
    pub identify_edges: bool,
}

impl MeshDataOptions {
    #[inline]
    /// Creates default mesh cooking options.
    pub const fn new() -> Self {
        Self {
            weld_tolerance: 0.0,
            weld_vertices: false,
            use_median_split: false,
            identify_edges: true,
        }
    }

    #[inline]
    /// Sets the vertex welding tolerance.
    pub const fn weld_tolerance(mut self, weld_tolerance: f32) -> Self {
        self.weld_tolerance = weld_tolerance;
        self
    }

    #[inline]
    /// Enables or disables vertex welding.
    pub const fn weld_vertices(mut self, weld_vertices: bool) -> Self {
        self.weld_vertices = weld_vertices;
        self
    }

    #[inline]
    /// Enables or disables median splitting for the mesh tree.
    pub const fn use_median_split(mut self, use_median_split: bool) -> Self {
        self.use_median_split = use_median_split;
        self
    }

    #[inline]
    /// Enables or disables edge identification during mesh cooking.
    pub const fn identify_edges(mut self, identify_edges: bool) -> Self {
        self.identify_edges = identify_edges;
        self
    }

    fn validate(self) -> Result<()> {
        validation::nonnegative("mesh.weld_tolerance", self.weld_tolerance)
    }
}

impl Default for MeshDataOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
/// Builder for `MeshData` triangle meshes.
pub struct MeshDataBuilder {
    vertices: Vec<Vec3>,
    indices: Vec<i32>,
    material_indices: Option<Vec<u8>>,
    options: MeshDataOptions,
}

impl MeshDataBuilder {
    #[inline]
    /// Starts a mesh builder from vertices and triangle indices.
    pub fn new(vertices: impl Into<Vec<Vec3>>, indices: impl Into<Vec<i32>>) -> Self {
        Self {
            vertices: vertices.into(),
            indices: indices.into(),
            material_indices: None,
            options: MeshDataOptions::default(),
        }
    }

    #[inline]
    /// Sets one material index per triangle.
    pub fn material_indices(mut self, material_indices: impl Into<Vec<u8>>) -> Self {
        self.material_indices = Some(material_indices.into());
        self
    }

    #[inline]
    /// Sets the vertex welding tolerance.
    pub fn weld_tolerance(mut self, weld_tolerance: f32) -> Self {
        self.options.weld_tolerance = weld_tolerance;
        self
    }

    #[inline]
    /// Enables or disables vertex welding.
    pub fn weld_vertices(mut self, weld_vertices: bool) -> Self {
        self.options.weld_vertices = weld_vertices;
        self
    }

    #[inline]
    /// Enables or disables median splitting for the mesh tree.
    pub fn use_median_split(mut self, use_median_split: bool) -> Self {
        self.options.use_median_split = use_median_split;
        self
    }

    #[inline]
    /// Enables or disables edge identification.
    pub fn identify_edges(mut self, identify_edges: bool) -> Self {
        self.options.identify_edges = identify_edges;
        self
    }

    #[inline]
    /// Cooks the configured mesh into owned Box3D mesh data.
    pub fn build(self) -> Result<MeshData> {
        MeshData::from_triangles(
            &self.vertices,
            &self.indices,
            self.material_indices.as_deref(),
            self.options,
        )
    }
}

#[derive(Debug)]
/// Owned cooked triangle mesh data allocated by Box3D.
///
/// Mesh values own native memory and are intentionally not `Send` or `Sync`.
pub struct MeshData {
    inner: Option<MeshDataInner>,
}

#[derive(Debug)]
struct MeshDataInner {
    raw: NonNull<ffi::b3MeshData>,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl MeshDataInner {
    fn destroy(self) {
        let owner = callback_state::RetainOnUnwind::new(self);
        unsafe { ffi::b3DestroyMesh(owner.raw.as_ptr()) };
        #[cfg(test)]
        record_shape_drop(ShapeDropEvent::MeshBacking);
        owner.finish();
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Triangle returned by a mesh or height-field overlap query.
pub struct MeshTriangleHit {
    /// First triangle vertex in local shape space.
    pub a: Vec3,
    /// Second triangle vertex in local shape space.
    pub b: Vec3,
    /// Third triangle vertex in local shape space.
    pub c: Vec3,
    /// Index of the triangle in the source mesh or height field.
    pub triangle_index: i32,
}

impl MeshData {
    #[inline]
    /// Starts a mesh builder from vertices and triangle indices.
    pub fn builder(
        vertices: impl Into<Vec<Vec3>>,
        indices: impl Into<Vec<i32>>,
    ) -> MeshDataBuilder {
        MeshDataBuilder::new(vertices, indices)
    }

    /// Cooks triangle vertices and indices into owned mesh data.
    pub fn from_triangles(
        vertices: impl AsRef<[Vec3]>,
        indices: impl AsRef<[i32]>,
        material_indices: Option<&[u8]>,
        options: MeshDataOptions,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        options.validate()?;
        let vertices = vertices.as_ref();
        let indices = indices.as_ref();
        if vertices.len() < 3 {
            return Err(validation::invalid(
                "mesh.vertices",
                InvalidValueReason::OutOfRange,
            ));
        }
        let vertex_count = validation::count_i32("mesh.vertices", vertices.len())?;
        for vertex in vertices {
            validation::vec3("mesh.vertices", *vertex)?;
        }
        if indices.is_empty() {
            return Err(validation::invalid(
                "mesh.indices",
                InvalidValueReason::OutOfRange,
            ));
        }
        if indices.len() % 3 != 0 {
            return Err(validation::invalid(
                "mesh.indices",
                InvalidValueReason::Malformed,
            ));
        }
        let triangle_count = indices.len() / 3;
        let triangle_count_i32 = validation::count_i32("mesh.triangles", triangle_count)?;
        if let Some(material_indices) = material_indices
            && material_indices.len() != triangle_count
        {
            return Err(validation::invalid(
                "mesh.material_indices",
                InvalidValueReason::InvalidCombination,
            ));
        }

        for triangle in indices.chunks_exact(3) {
            let [a, b, c]: [i32; 3] = triangle.try_into().expect("chunk size is fixed");
            if a < 0
                || b < 0
                || c < 0
                || a as usize >= vertices.len()
                || b as usize >= vertices.len()
                || c as usize >= vertices.len()
            {
                return Err(validation::invalid(
                    "mesh.indices",
                    InvalidValueReason::OutOfRange,
                ));
            }
            if triangle_area_squared(
                vertices[a as usize],
                vertices[b as usize],
                vertices[c as usize],
            ) <= f32::MIN_POSITIVE
            {
                return Err(validation::invalid(
                    "mesh.triangles",
                    InvalidValueReason::Malformed,
                ));
            }
        }

        let mut vertices: Vec<ffi::b3Vec3> =
            vertices.iter().map(|vertex| vertex.into_raw()).collect();
        let mut indices = indices.to_vec();
        let mut materials = material_indices.map(<[u8]>::to_vec);
        let def = ffi::b3MeshDef {
            vertices: vertices.as_mut_ptr(),
            indices: indices.as_mut_ptr(),
            materialIndices: materials
                .as_mut()
                .map_or(std::ptr::null_mut(), |materials| materials.as_mut_ptr()),
            weldTolerance: options.weld_tolerance,
            vertexCount: vertex_count,
            triangleCount: triangle_count_i32,
            weldVertices: options.weld_vertices,
            useMedianSplit: options.use_median_split,
            identifyEdges: options.identify_edges,
        };

        Self::from_native(|| unsafe { ffi::b3CreateMesh(&def, std::ptr::null_mut(), 0) })
    }

    /// Creates a generated box mesh.
    pub fn box_mesh(
        center: impl Into<Vec3>,
        extent: impl Into<Vec3>,
        identify_edges: bool,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let center = center.into();
        let extent = extent.into();
        validation::vec3("mesh.box.center", center)?;
        let extent = validate_positive_vec3("mesh.box.extent", extent)?;
        Self::from_native(|| unsafe {
            ffi::b3CreateBoxMesh(center.into_raw(), extent.into_raw(), identify_edges)
        })
    }

    /// Creates a generated grid mesh.
    pub fn grid_mesh(
        x_count: i32,
        z_count: i32,
        cell_width: f32,
        material_count: i32,
        identify_edges: bool,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        if x_count < 2 {
            return Err(validation::invalid(
                "mesh.grid.x_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        if z_count < 2 {
            return Err(validation::invalid(
                "mesh.grid.z_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        validation::positive("mesh.grid.cell_width", cell_width)?;
        if material_count <= 0 {
            return Err(validation::invalid(
                "mesh.grid.material_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        Self::from_native(|| unsafe {
            ffi::b3CreateGridMesh(x_count, z_count, cell_width, material_count, identify_edges)
        })
    }

    /// Creates a generated wave mesh.
    pub fn wave_mesh(
        x_count: i32,
        z_count: i32,
        cell_width: f32,
        amplitude: f32,
        row_frequency: f32,
        column_frequency: f32,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        if x_count < 2 {
            return Err(validation::invalid(
                "mesh.wave.x_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        if z_count < 2 {
            return Err(validation::invalid(
                "mesh.wave.z_count",
                InvalidValueReason::OutOfRange,
            ));
        }
        validation::positive("mesh.wave.cell_width", cell_width)?;
        validation::finite("mesh.wave.amplitude", amplitude)?;
        validation::finite("mesh.wave.row_frequency", row_frequency)?;
        validation::finite("mesh.wave.column_frequency", column_frequency)?;
        Self::from_native(|| unsafe {
            ffi::b3CreateWaveMesh(
                x_count,
                z_count,
                cell_width,
                amplitude,
                row_frequency,
                column_frequency,
            )
        })
    }

    /// Creates a generated torus mesh.
    pub fn torus_mesh(
        radial_resolution: i32,
        tubular_resolution: i32,
        radius: f32,
        thickness: f32,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        if radial_resolution < 3 {
            return Err(validation::invalid(
                "mesh.torus.radial_resolution",
                InvalidValueReason::OutOfRange,
            ));
        }
        if tubular_resolution < 3 {
            return Err(validation::invalid(
                "mesh.torus.tubular_resolution",
                InvalidValueReason::OutOfRange,
            ));
        }
        validation::positive("mesh.torus.radius", radius)?;
        validation::positive("mesh.torus.thickness", thickness)?;
        Self::from_native(|| unsafe {
            ffi::b3CreateTorusMesh(radial_resolution, tubular_resolution, radius, thickness)
        })
    }

    /// Creates a generated hollow box mesh.
    pub fn hollow_box_mesh(center: impl Into<Vec3>, extent: impl Into<Vec3>) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let center = center.into();
        let extent = extent.into();
        validation::vec3("mesh.hollow_box.center", center)?;
        let extent = validate_positive_vec3("mesh.hollow_box.extent", extent)?;
        Self::from_native(|| unsafe {
            ffi::b3CreateHollowBoxMesh(center.into_raw(), extent.into_raw())
        })
    }

    /// Creates a generated platform mesh.
    pub fn platform_mesh(
        center: impl Into<Vec3>,
        height: f32,
        top_width: f32,
        bottom_width: f32,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let center = center.into();
        validation::vec3("mesh.platform.center", center)?;
        validation::positive("mesh.platform.height", height)?;
        validation::positive("mesh.platform.top_width", top_width)?;
        validation::positive("mesh.platform.bottom_width", bottom_width)?;
        Self::from_native(|| unsafe {
            ffi::b3CreatePlatformMesh(center.into_raw(), height, top_width, bottom_width)
        })
    }

    #[inline]
    /// Returns the native byte count of the cooked mesh.
    pub fn byte_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().byteCount }
    }

    #[inline]
    /// Returns the height of the cooked mesh acceleration tree.
    pub fn tree_height(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().treeHeight }
    }

    #[inline]
    /// Returns the number of cooked mesh vertices.
    pub fn vertex_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().vertexCount }
    }

    #[inline]
    /// Returns the number of cooked mesh triangles.
    pub fn triangle_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().triangleCount }
    }

    #[inline]
    /// Returns the number of material slots referenced by the cooked mesh.
    pub fn material_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().materialCount }
    }

    /// Collects triangles whose bounds overlap an AABB at the given scale.
    pub fn query_triangles(
        &self,
        bounds: Aabb,
        scale: impl Into<Vec3>,
    ) -> Result<Vec<MeshTriangleHit>> {
        let mut out = Vec::new();
        self.query_triangles_into(bounds, scale, &mut out)?;
        Ok(out)
    }

    /// Writes triangles whose bounds overlap an AABB into `out`.
    pub fn query_triangles_into(
        &self,
        bounds: Aabb,
        scale: impl Into<Vec3>,
        out: &mut Vec<MeshTriangleHit>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_triangles(bounds, scale, |hit| {
            out.push(hit);
            true
        })
    }

    /// Visits triangles whose bounds overlap an AABB at the given scale.
    ///
    /// Return `false` from the visitor to stop traversal early.
    pub fn visit_triangles<F>(&self, bounds: Aabb, scale: impl Into<Vec3>, visitor: F) -> Result<()>
    where
        F: FnMut(MeshTriangleHit) -> bool,
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (bounds, scale, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let bounds = bounds.validate()?;
            let scale = validate_mesh_scale(scale.into())?;
            let raw_mesh = ffi::b3Mesh {
                data: self.as_ptr(),
                scale: scale.into_raw(),
            };
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let mut ctx = MeshTriangleQueryContext {
                visitor,
                state: LocalCallbackState::new(),
            };
            {
                let _call = self.inner()._foundation_lease.enter_call()?;
                unsafe {
                    ffi::b3QueryMesh(
                        &raw_mesh,
                        bounds.into_raw(),
                        Some(mesh_triangle_query_trampoline::<F>),
                        (&mut ctx as *mut MeshTriangleQueryContext<_>).cast(),
                    );
                }
            }
            let result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            result
        }
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *const ffi::b3MeshData {
        self.inner().raw.as_ptr()
    }

    fn from_native(create: impl FnOnce() -> *mut ffi::b3MeshData) -> Result<Self> {
        create_native_owner(create)
            .map(|(raw, foundation_lease)| MeshDataInner {
                raw,
                _foundation_lease: foundation_lease,
                _not_send_sync: PhantomData,
            })
            .map(|inner| Self { inner: Some(inner) })
    }

    fn inner(&self) -> &MeshDataInner {
        self.inner
            .as_ref()
            .expect("live mesh always owns its complete inner")
    }
}

impl Drop for MeshData {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            cleanup_local_owner(inner, MeshDataInner::destroy);
        }
    }
}

#[derive(Clone, Debug)]
/// Builder for `HeightField`.
pub struct HeightFieldBuilder {
    row_count: i32,
    column_count: i32,
    heights: Vec<f32>,
    material_indices: Option<Vec<u8>>,
    clockwise_winding: bool,
}

impl HeightFieldBuilder {
    #[inline]
    /// Starts a height-field builder from row count, column count, and samples.
    pub fn new(row_count: i32, column_count: i32, heights: impl Into<Vec<f32>>) -> Self {
        Self {
            row_count,
            column_count,
            heights: heights.into(),
            material_indices: None,
            clockwise_winding: false,
        }
    }

    #[inline]
    /// Sets one material index per height-field cell.
    pub fn material_indices(mut self, material_indices: impl Into<Vec<u8>>) -> Self {
        self.material_indices = Some(material_indices.into());
        self
    }

    #[inline]
    /// Sets whether cells use clockwise triangle winding.
    pub fn clockwise_winding(mut self, clockwise_winding: bool) -> Self {
        self.clockwise_winding = clockwise_winding;
        self
    }

    #[inline]
    /// Builds owned height-field data using the supplied sample scale.
    pub fn build(self, scale: impl Into<Vec3>) -> Result<HeightField> {
        HeightField::from_samples(
            self.row_count,
            self.column_count,
            &self.heights,
            scale,
            self.material_indices.as_deref(),
            self.clockwise_winding,
        )
    }
}

#[derive(Debug)]
/// Owned height-field data allocated by Box3D.
///
/// Height fields own native memory and are intentionally not `Send` or `Sync`.
pub struct HeightField {
    inner: Option<HeightFieldInner>,
}

#[derive(Debug)]
struct HeightFieldInner {
    raw: NonNull<ffi::b3HeightFieldData>,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl HeightFieldInner {
    fn destroy(self) {
        let owner = callback_state::RetainOnUnwind::new(self);
        unsafe { ffi::b3DestroyHeightField(owner.raw.as_ptr()) };
        #[cfg(test)]
        record_shape_drop(ShapeDropEvent::HeightFieldBacking);
        owner.finish();
    }
}

impl HeightField {
    #[inline]
    /// Starts a height-field builder from row count, column count, and samples.
    pub fn builder(
        row_count: i32,
        column_count: i32,
        heights: impl Into<Vec<f32>>,
    ) -> HeightFieldBuilder {
        HeightFieldBuilder::new(row_count, column_count, heights)
    }

    /// Creates a height field from explicit samples.
    pub fn from_samples(
        row_count: i32,
        column_count: i32,
        heights: impl AsRef<[f32]>,
        scale: impl Into<Vec3>,
        material_indices: Option<&[u8]>,
        clockwise_winding: bool,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let scale = scale.into();
        let heights = heights.as_ref();
        let sample_count = validate_height_field_dimensions(row_count, column_count)?;
        if heights.len() != sample_count {
            return Err(validation::invalid(
                "height_field.heights",
                InvalidValueReason::InvalidCombination,
            ));
        }
        validate_positive_vec3("height_field.scale", scale)?;
        for height in heights {
            validation::finite("height_field.heights", *height)?;
        }

        let cell_count = (row_count as usize - 1) * (column_count as usize - 1);
        if let Some(material_indices) = material_indices
            && material_indices.len() != cell_count
        {
            return Err(validation::invalid(
                "height_field.material_indices",
                InvalidValueReason::InvalidCombination,
            ));
        }

        let mut heights = heights.to_vec();
        let mut materials = material_indices.map(<[u8]>::to_vec);
        let (global_minimum_height, global_maximum_height) =
            min_max_finite(&heights).expect("validated height samples are non-empty and finite");
        let def = ffi::b3HeightFieldDef {
            heights: heights.as_mut_ptr(),
            materialIndices: materials
                .as_mut()
                .map_or(std::ptr::null_mut(), |materials| materials.as_mut_ptr()),
            scale: scale.into_raw(),
            countX: column_count,
            countZ: row_count,
            globalMinimumHeight: global_minimum_height,
            globalMaximumHeight: global_maximum_height,
            clockwiseWinding: clockwise_winding,
        };

        Self::from_native(|| unsafe { ffi::b3CreateHeightField(&def) })
    }

    /// Creates a generated grid height field.
    pub fn grid(
        row_count: i32,
        column_count: i32,
        scale: impl Into<Vec3>,
        make_holes: bool,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let scale = scale.into();
        validate_height_field_dimensions(row_count, column_count)?;
        let scale = validate_positive_vec3("height_field.scale", scale)?;
        Self::from_native(|| unsafe {
            ffi::b3CreateGrid(row_count, column_count, scale.into_raw(), make_holes)
        })
    }

    /// Creates a generated wave height field.
    pub fn wave(
        row_count: i32,
        column_count: i32,
        scale: impl Into<Vec3>,
        row_frequency: f32,
        column_frequency: f32,
        make_holes: bool,
    ) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let scale = scale.into();
        validate_height_field_dimensions(row_count, column_count)?;
        let scale = validate_positive_vec3("height_field.scale", scale)?;
        validation::finite("height_field.row_frequency", row_frequency)?;
        validation::finite("height_field.column_frequency", column_frequency)?;
        Self::from_native(|| unsafe {
            ffi::b3CreateWave(
                row_count,
                column_count,
                scale.into_raw(),
                row_frequency,
                column_frequency,
                make_holes,
            )
        })
    }

    #[inline]
    /// Returns the native byte count of the height-field data.
    pub fn byte_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().byteCount }
    }

    #[inline]
    /// Returns the number of sample rows.
    pub fn row_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().rowCount }
    }

    #[inline]
    /// Returns the number of sample columns.
    pub fn column_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().columnCount }
    }

    /// Collects height-field triangles whose bounds overlap an AABB.
    pub fn query_triangles(&self, bounds: Aabb) -> Result<Vec<MeshTriangleHit>> {
        let mut out = Vec::new();
        self.query_triangles_into(bounds, &mut out)?;
        Ok(out)
    }

    /// Writes height-field triangles whose bounds overlap an AABB into `out`.
    pub fn query_triangles_into(&self, bounds: Aabb, out: &mut Vec<MeshTriangleHit>) -> Result<()> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_triangles(bounds, |hit| out.push(hit))
    }

    /// Visits height-field triangles whose bounds overlap an AABB.
    pub fn visit_triangles<F>(&self, bounds: Aabb, visitor: F) -> Result<()>
    where
        F: FnMut(MeshTriangleHit),
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (bounds, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let Some(bounds) = clamp_height_field_query_bounds(bounds.validate()?, unsafe {
                self.inner().raw.as_ref().aabb
            })?
            else {
                return Ok(());
            };
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let mut ctx = HeightFieldTriangleQueryContext {
                visitor,
                state: LocalCallbackState::new(),
            };
            {
                let _call = self.inner()._foundation_lease.enter_call()?;
                unsafe {
                    ffi::b3QueryHeightField(
                        self.as_ptr(),
                        bounds.into_raw(),
                        Some(height_field_triangle_query_trampoline::<F>),
                        (&mut ctx as *mut HeightFieldTriangleQueryContext<_>).cast(),
                    );
                }
            }
            let result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            result
        }
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *const ffi::b3HeightFieldData {
        self.inner().raw.as_ptr()
    }

    fn from_native(create: impl FnOnce() -> *mut ffi::b3HeightFieldData) -> Result<Self> {
        create_native_owner(create)
            .map(|(raw, foundation_lease)| HeightFieldInner {
                raw,
                _foundation_lease: foundation_lease,
                _not_send_sync: PhantomData,
            })
            .map(|inner| Self { inner: Some(inner) })
    }

    fn inner(&self) -> &HeightFieldInner {
        self.inner
            .as_ref()
            .expect("live height field always owns its complete inner")
    }
}

impl Drop for HeightField {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            cleanup_local_owner(inner, HeightFieldInner::destroy);
        }
    }
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
struct MeshTriangleQueryContext<F> {
    visitor: F,
    state: LocalCallbackState,
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
unsafe extern "C" fn mesh_triangle_query_trampoline<F>(
    a: ffi::b3Vec3,
    b: ffi::b3Vec3,
    c: ffi::b3Vec3,
    triangle_index: i32,
    context: *mut c_void,
) -> bool
where
    F: FnMut(MeshTriangleHit) -> bool,
{
    let ctx = unsafe { &mut *context.cast::<MeshTriangleQueryContext<F>>() };
    ctx.state.invoke(false, || {
        let hit = MeshTriangleHit {
            a: Vec3::from_raw(a),
            b: Vec3::from_raw(b),
            c: Vec3::from_raw(c),
            triangle_index,
        };
        (ctx.visitor)(hit)
    })
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
struct HeightFieldTriangleQueryContext<F> {
    visitor: F,
    state: LocalCallbackState,
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
unsafe extern "C" fn height_field_triangle_query_trampoline<F>(
    a: ffi::b3Vec3,
    b: ffi::b3Vec3,
    c: ffi::b3Vec3,
    triangle_index: i32,
    context: *mut c_void,
) -> bool
where
    F: FnMut(MeshTriangleHit),
{
    let ctx = unsafe { &mut *context.cast::<HeightFieldTriangleQueryContext<F>>() };
    ctx.state.invoke(false, || {
        let hit = MeshTriangleHit {
            a: Vec3::from_raw(a),
            b: Vec3::from_raw(b),
            c: Vec3::from_raw(c),
            triangle_index,
        };
        (ctx.visitor)(hit);
        true
    })
}

#[derive(Copy, Clone, Debug)]
/// Borrowed view of hull data owned by another Box3D shape resource.
///
/// This type does not own native storage. It is tied to the `World` or
/// `Compound` borrow that produced it and becomes invalid if the owning shape,
/// compound, or resource-backed geometry is destroyed or replaced.
pub struct ShapeHull<'a> {
    raw: &'a ffi::b3HullData,
}

impl<'a> ShapeHull<'a> {
    #[inline]
    pub(crate) const fn from_raw(raw: &'a ffi::b3HullData) -> Self {
        Self { raw }
    }

    #[inline]
    /// Returns the native byte count of the hull data.
    pub const fn byte_count(&self) -> i32 {
        self.raw.byteCount
    }

    #[inline]
    /// Returns Box3D's stable hash for the hull data.
    pub const fn hash(&self) -> u32 {
        self.raw.hash
    }

    #[inline]
    /// Returns the hull's local-space AABB.
    pub const fn aabb(&self) -> Aabb {
        Aabb::from_raw(self.raw.aabb)
    }

    #[inline]
    /// Returns the hull surface area.
    pub const fn surface_area(&self) -> f32 {
        self.raw.surfaceArea
    }

    #[inline]
    /// Returns the hull volume.
    pub const fn volume(&self) -> f32 {
        self.raw.volume
    }

    #[inline]
    /// Returns the hull inner radius.
    pub const fn inner_radius(&self) -> f32 {
        self.raw.innerRadius
    }

    #[inline]
    /// Returns the hull center of mass.
    pub const fn center(&self) -> Vec3 {
        Vec3::from_raw(self.raw.center)
    }

    #[inline]
    /// Returns the number of hull vertices.
    pub const fn vertex_count(&self) -> i32 {
        self.raw.vertexCount
    }

    #[inline]
    /// Returns the number of hull half-edges.
    pub const fn edge_count(&self) -> i32 {
        self.raw.edgeCount
    }

    #[inline]
    /// Returns the number of hull faces.
    pub const fn face_count(&self) -> i32 {
        self.raw.faceCount
    }
}

#[derive(Copy, Clone, Debug)]
/// Borrowed view of mesh data owned by another Box3D shape resource.
///
/// This type does not own native storage. It is tied to the `World` or
/// `Compound` borrow that produced it and becomes invalid if the owning shape,
/// compound, or resource-backed geometry is destroyed or replaced.
pub struct ShapeMesh<'a> {
    data: &'a ffi::b3MeshData,
    scale: Vec3,
}

impl<'a> ShapeMesh<'a> {
    #[inline]
    pub(crate) fn from_raw(raw: ffi::b3Mesh) -> Option<Self> {
        unsafe { raw.data.as_ref() }.map(|data| Self {
            data,
            scale: Vec3::from_raw(raw.scale),
        })
    }

    #[inline]
    /// Returns the scale applied to this mesh instance.
    pub const fn scale(&self) -> Vec3 {
        self.scale
    }

    #[inline]
    /// Returns the native byte count of the mesh data.
    pub const fn byte_count(&self) -> i32 {
        self.data.byteCount
    }

    #[inline]
    /// Returns Box3D's stable hash for the mesh data.
    pub const fn hash(&self) -> u32 {
        self.data.hash
    }

    #[inline]
    /// Returns the mesh local-space bounds.
    pub const fn bounds(&self) -> Aabb {
        Aabb::from_raw(self.data.bounds)
    }

    #[inline]
    /// Returns the mesh surface area.
    pub const fn surface_area(&self) -> f32 {
        self.data.surfaceArea
    }

    #[inline]
    /// Returns the height of the mesh acceleration tree.
    pub const fn tree_height(&self) -> i32 {
        self.data.treeHeight
    }

    #[inline]
    /// Returns the number of degenerate triangles found during cooking.
    pub const fn degenerate_count(&self) -> i32 {
        self.data.degenerateCount
    }

    #[inline]
    /// Returns the number of mesh vertices.
    pub const fn vertex_count(&self) -> i32 {
        self.data.vertexCount
    }

    #[inline]
    /// Returns the number of mesh triangles.
    pub const fn triangle_count(&self) -> i32 {
        self.data.triangleCount
    }

    #[inline]
    /// Returns the number of material slots referenced by the mesh.
    pub const fn material_count(&self) -> i32 {
        self.data.materialCount
    }
}

#[derive(Copy, Clone, Debug)]
/// Borrowed view of height-field data owned by another Box3D shape resource.
///
/// This type does not own native storage. It is tied to the `World` borrow that
/// produced it and becomes invalid if the owning shape or resource-backed
/// geometry is destroyed or replaced.
pub struct ShapeHeightField<'a> {
    raw: &'a ffi::b3HeightFieldData,
}

impl<'a> ShapeHeightField<'a> {
    #[inline]
    pub(crate) const fn from_raw(raw: &'a ffi::b3HeightFieldData) -> Self {
        Self { raw }
    }

    #[inline]
    /// Returns the native byte count of the height-field data.
    pub const fn byte_count(&self) -> i32 {
        self.raw.byteCount
    }

    #[inline]
    /// Returns Box3D's stable hash for the height-field data.
    pub const fn hash(&self) -> u32 {
        self.raw.hash
    }

    #[inline]
    /// Returns the height-field local-space AABB.
    pub const fn aabb(&self) -> Aabb {
        Aabb::from_raw(self.raw.aabb)
    }

    #[inline]
    /// Returns the minimum sample height.
    pub const fn min_height(&self) -> f32 {
        self.raw.minHeight
    }

    #[inline]
    /// Returns the maximum sample height.
    pub const fn max_height(&self) -> f32 {
        self.raw.maxHeight
    }

    #[inline]
    /// Returns the sample scale used by the height field.
    pub const fn scale(&self) -> Vec3 {
        Vec3::from_raw(self.raw.scale)
    }

    #[inline]
    /// Returns the number of sample columns.
    pub const fn column_count(&self) -> i32 {
        self.raw.columnCount
    }

    #[inline]
    /// Returns the number of sample rows.
    pub const fn row_count(&self) -> i32 {
        self.raw.rowCount
    }

    #[inline]
    /// Returns whether cells use clockwise triangle winding.
    pub const fn clockwise(&self) -> bool {
        self.raw.clockwise
    }
}

#[derive(Debug)]
/// Owned compound shape data allocated by Box3D.
///
/// A compound stores multiple primitive children and shared geometry resources
/// in a single native allocation.
pub struct Compound {
    inner: Option<CompoundInner>,
}

#[derive(Debug)]
struct CompoundInner {
    raw: NonNull<ffi::b3CompoundData>,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl CompoundInner {
    fn destroy(self) {
        let owner = callback_state::RetainOnUnwind::new(self);
        unsafe { ffi::b3DestroyCompound(owner.raw.as_ptr()) };
        #[cfg(test)]
        record_shape_drop(ShapeDropEvent::CompoundBacking);
        owner.finish();
    }
}

#[derive(Debug)]
/// Serialized bytes for a `Compound`.
///
/// The byte buffer is still owned by Box3D and can be converted back into an
/// owned `Compound`.
pub struct CompoundBytes {
    inner: Option<CompoundBytesInner>,
}

#[derive(Debug)]
struct CompoundBytesInner {
    raw: NonNull<u8>,
    byte_count: i32,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl CompoundBytesInner {
    fn destroy(self) {
        let owner = callback_state::RetainOnUnwind::new(self);
        unsafe { ffi::b3DestroyCompound(owner.raw.as_ptr().cast()) };
        owner.finish();
    }
}

impl Compound {
    #[inline]
    /// Starts a compound builder.
    pub fn builder<'a>() -> CompoundBuilder<'a> {
        CompoundBuilder::new()
    }

    /// Creates a compound containing exactly one sphere child.
    pub fn single_sphere(sphere: Sphere, material: SurfaceMaterial) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        Self::builder().with_sphere(sphere, material)?.build()
    }

    #[inline]
    /// Returns the native byte count of the compound data.
    pub fn byte_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().byteCount }
    }

    #[inline]
    /// Returns the total number of child shapes.
    pub fn child_count(&self) -> i32 {
        unsafe {
            let raw = self.inner().raw.as_ref();
            raw.capsuleCount + raw.hullCount + raw.meshCount + raw.sphereCount
        }
    }

    #[inline]
    /// Returns the number of capsule children.
    pub fn capsule_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().capsuleCount }
    }

    #[inline]
    /// Returns the number of hull children.
    pub fn hull_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().hullCount }
    }

    #[inline]
    /// Returns the number of mesh children.
    pub fn mesh_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().meshCount }
    }

    #[inline]
    /// Returns the number of sphere children.
    pub fn sphere_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().sphereCount }
    }

    #[inline]
    /// Returns the number of material records stored by the compound.
    pub fn material_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().materialCount }
    }

    #[inline]
    /// Returns the number of shared hull resources stored by the compound.
    pub fn shared_hull_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().sharedHullCount }
    }

    #[inline]
    /// Returns the number of shared mesh resources stored by the compound.
    pub fn shared_mesh_count(&self) -> i32 {
        unsafe { self.inner().raw.as_ref().sharedMeshCount }
    }

    /// Returns a material by compound material index.
    pub fn material(&self, index: i32) -> Result<SurfaceMaterial> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.material_count() {
            return Err(validation::invalid(
                "compound.material_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let _call = self.inner()._foundation_lease.enter_call()?;
        let materials = unsafe { ffi::b3GetCompoundMaterials(self.as_ptr()) };
        unsafe { materials.add(index as usize).as_ref() }
            .copied()
            .map(SurfaceMaterial::from_raw)
            .ok_or(Error::NativeFailure)
    }

    /// Returns a child by flattened child index.
    pub fn child(&self, index: i32) -> Result<CompoundChild<'_>> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.child_count() {
            return Err(validation::invalid(
                "compound.child_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let _call = self.inner()._foundation_lease.enter_call()?;
        CompoundChild::from_raw(unsafe { ffi::b3GetCompoundChild(self.as_ptr(), index) })
    }

    /// Returns a capsule child by capsule-child index.
    pub fn capsule_child(&self, index: i32) -> Result<CompoundCapsule> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.capsule_count() {
            return Err(validation::invalid(
                "compound.capsule_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let _call = self.inner()._foundation_lease.enter_call()?;
        let raw = unsafe { ffi::b3GetCompoundCapsule(self.as_ptr(), index) };
        Ok(CompoundCapsule::from_raw(raw))
    }

    /// Returns a hull child by hull-child index.
    pub fn hull_child(&self, index: i32) -> Result<CompoundHull<'_>> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.hull_count() {
            return Err(validation::invalid(
                "compound.hull_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let _call = self.inner()._foundation_lease.enter_call()?;
        let raw = unsafe { ffi::b3GetCompoundHull(self.as_ptr(), index) };
        CompoundHull::from_raw(raw)
    }

    /// Returns a mesh child by mesh-child index.
    pub fn mesh_child(&self, index: i32) -> Result<CompoundMesh<'_>> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.mesh_count() {
            return Err(validation::invalid(
                "compound.mesh_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let _call = self.inner()._foundation_lease.enter_call()?;
        let raw = unsafe { ffi::b3GetCompoundMesh(self.as_ptr(), index) };
        CompoundMesh::from_raw(raw)
    }

    /// Returns a sphere child by sphere-child index.
    pub fn sphere_child(&self, index: i32) -> Result<CompoundSphere> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.sphere_count() {
            return Err(validation::invalid(
                "compound.sphere_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let _call = self.inner()._foundation_lease.enter_call()?;
        let raw = unsafe { ffi::b3GetCompoundSphere(self.as_ptr(), index) };
        Ok(CompoundSphere::from_raw(raw))
    }

    /// Collects compound children whose bounds overlap an AABB.
    pub fn query_aabb(&self, aabb: Aabb) -> Result<Vec<CompoundQueryHit<'_>>> {
        let mut out = Vec::new();
        self.query_aabb_into(aabb, &mut out)?;
        Ok(out)
    }

    /// Writes compound children whose bounds overlap an AABB into `out`.
    pub fn query_aabb_into<'a>(
        &'a self,
        aabb: Aabb,
        out: &mut Vec<CompoundQueryHit<'a>>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_query_aabb(aabb, |hit| {
            out.push(hit);
            true
        })
    }

    /// Visits compound children whose bounds overlap an AABB.
    ///
    /// Return `false` from the visitor to stop traversal early.
    pub fn visit_query_aabb<'a, F>(&'a self, aabb: Aabb, visitor: F) -> Result<()>
    where
        F: FnMut(CompoundQueryHit<'a>) -> bool,
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (aabb, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let aabb = aabb.validate()?;
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let mut ctx = CompoundQueryContext {
                visitor,
                state: LocalCallbackState::new(),
                _lifetime: PhantomData,
            };
            {
                let _call = self.inner()._foundation_lease.enter_call()?;
                unsafe {
                    ffi::b3QueryCompound(
                        self.as_ptr(),
                        aabb.into_raw(),
                        Some(compound_query_trampoline::<F>),
                        (&mut ctx as *mut CompoundQueryContext<'a, F>).cast(),
                    );
                }
            }
            let result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            result
        }
    }

    /// Converts this compound into Box3D-owned serialized bytes.
    pub fn into_bytes(mut self) -> Result<CompoundBytes> {
        callback_state::check_not_in_callback()?;
        let byte_count = self.byte_count();
        if byte_count < 0 {
            return Err(Error::NativeFailure);
        }
        let raw = {
            let _call = self.inner()._foundation_lease.enter_call()?;
            unsafe { ffi::b3ConvertCompoundToBytes(self.inner().raw.as_ptr()) }
        };
        let raw = NonNull::new(raw).ok_or(Error::NativeFailure)?;
        let CompoundInner {
            raw: _transferred_raw,
            _foundation_lease,
            _not_send_sync,
        } = self.take_inner();
        Ok(CompoundBytes {
            inner: Some(CompoundBytesInner {
                raw,
                byte_count,
                _foundation_lease,
                _not_send_sync,
            }),
        })
    }

    #[inline]
    pub(crate) fn as_ptr(&self) -> *const ffi::b3CompoundData {
        self.inner().raw.as_ptr()
    }

    fn from_native(create: impl FnOnce() -> *mut ffi::b3CompoundData) -> Result<Self> {
        create_native_owner(create)
            .map(|(raw, foundation_lease)| CompoundInner {
                raw,
                _foundation_lease: foundation_lease,
                _not_send_sync: PhantomData,
            })
            .map(|inner| Self { inner: Some(inner) })
    }

    fn inner(&self) -> &CompoundInner {
        self.inner
            .as_ref()
            .expect("live compound always owns its complete inner")
    }

    fn take_inner(&mut self) -> CompoundInner {
        self.inner
            .take()
            .expect("live compound always owns its complete inner")
    }
}

impl Drop for Compound {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            cleanup_local_owner(inner, CompoundInner::destroy);
        }
    }
}

impl CompoundBytes {
    #[inline]
    /// Returns the number of serialized bytes.
    pub const fn byte_count(&self) -> i32 {
        match &self.inner {
            Some(inner) => inner.byte_count,
            None => panic!("live compound bytes always own their complete inner"),
        }
    }

    #[inline]
    /// Borrows the serialized byte buffer.
    pub fn as_slice(&self) -> &[u8] {
        let inner = self.inner();
        unsafe { slice::from_raw_parts(inner.raw.as_ptr(), inner.byte_count as usize) }
    }

    /// Converts the serialized bytes back into an owned compound.
    pub fn into_compound(mut self) -> Result<Compound> {
        callback_state::check_not_in_callback()?;
        let raw = {
            let _call = self.inner()._foundation_lease.enter_call()?;
            let inner = self.inner();
            unsafe { ffi::b3ConvertBytesToCompound(inner.raw.as_ptr(), inner.byte_count) }
        };
        let raw = NonNull::new(raw).ok_or(Error::NativeFailure)?;
        let CompoundBytesInner {
            raw: _transferred_raw,
            byte_count: _transferred_byte_count,
            _foundation_lease,
            _not_send_sync,
        } = self.take_inner();
        Ok(Compound {
            inner: Some(CompoundInner {
                raw,
                _foundation_lease,
                _not_send_sync,
            }),
        })
    }

    fn inner(&self) -> &CompoundBytesInner {
        self.inner
            .as_ref()
            .expect("live compound bytes always own their complete inner")
    }

    fn take_inner(&mut self) -> CompoundBytesInner {
        self.inner
            .take()
            .expect("live compound bytes always own their complete inner")
    }
}

impl Drop for CompoundBytes {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            cleanup_local_owner(inner, CompoundBytesInner::destroy);
        }
    }
}

#[derive(Debug)]
/// Builder for `Compound`.
///
/// Borrowed hull and mesh inputs must outlive the builder until `build` is
/// called because Box3D reads them during compound creation.
pub struct CompoundBuilder<'a> {
    capsules: Vec<ffi::b3CompoundCapsuleDef>,
    hulls: Vec<ffi::b3CompoundHullDef>,
    meshes: Vec<ffi::b3CompoundMeshDef>,
    mesh_materials: Vec<Box<[ffi::b3SurfaceMaterial]>>,
    spheres: Vec<ffi::b3CompoundSphereDef>,
    error: Option<Error>,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> CompoundBuilder<'a> {
    #[inline]
    /// Creates an empty compound builder.
    pub fn new() -> Self {
        Self {
            capsules: Vec::new(),
            hulls: Vec::new(),
            meshes: Vec::new(),
            mesh_materials: Vec::new(),
            spheres: Vec::new(),
            error: None,
            _lifetime: PhantomData,
        }
    }

    /// Adds a sphere child, storing any validation error until `build`.
    pub fn sphere(mut self, sphere: Sphere, material: SurfaceMaterial) -> Self {
        if let Err(error) = self.add_sphere(sphere, material) {
            self.error = Some(error);
        }
        self
    }

    /// Adds a sphere child and returns validation errors immediately.
    pub fn with_sphere(mut self, sphere: Sphere, material: SurfaceMaterial) -> Result<Self> {
        self.add_sphere(sphere, material)?;
        Ok(self)
    }

    /// Adds a sphere child to the builder in place.
    pub fn add_sphere(&mut self, sphere: Sphere, material: SurfaceMaterial) -> Result<&mut Self> {
        sphere.validate()?;
        material.validate()?;
        self.validate_additional_child()?;
        self.spheres.push(ffi::b3CompoundSphereDef {
            sphere: *sphere.raw(),
            material: material.into_raw(),
        });
        Ok(self)
    }

    /// Adds a capsule child, storing any validation error until `build`.
    pub fn capsule(mut self, capsule: Capsule, material: SurfaceMaterial) -> Self {
        if let Err(error) = self.add_capsule(capsule, material) {
            self.error = Some(error);
        }
        self
    }

    /// Adds a capsule child and returns validation errors immediately.
    pub fn with_capsule(mut self, capsule: Capsule, material: SurfaceMaterial) -> Result<Self> {
        self.add_capsule(capsule, material)?;
        Ok(self)
    }

    /// Adds a capsule child to the builder in place.
    pub fn add_capsule(
        &mut self,
        capsule: Capsule,
        material: SurfaceMaterial,
    ) -> Result<&mut Self> {
        capsule.validate()?;
        material.validate()?;
        self.validate_additional_child()?;
        self.capsules.push(ffi::b3CompoundCapsuleDef {
            capsule: *capsule.raw(),
            material: material.into_raw(),
        });
        Ok(self)
    }

    /// Adds a hull child, storing any validation error until `build`.
    pub fn hull(
        mut self,
        hull: &'a Hull,
        transform: impl Into<Transform>,
        material: SurfaceMaterial,
    ) -> Self {
        if let Err(error) = self.add_hull(hull, transform, material) {
            self.error = Some(error);
        }
        self
    }

    /// Adds a hull child and returns validation errors immediately.
    pub fn with_hull(
        mut self,
        hull: &'a Hull,
        transform: impl Into<Transform>,
        material: SurfaceMaterial,
    ) -> Result<Self> {
        self.add_hull(hull, transform, material)?;
        Ok(self)
    }

    /// Adds a hull child to the builder in place.
    pub fn add_hull(
        &mut self,
        hull: &'a Hull,
        transform: impl Into<Transform>,
        material: SurfaceMaterial,
    ) -> Result<&mut Self> {
        self.add_hull_ptr(hull.as_ptr(), transform.into(), material)
    }

    /// Adds a generated box hull child, storing any validation error until `build`.
    pub fn box_hull(
        mut self,
        hull: &'a BoxHull,
        transform: impl Into<Transform>,
        material: SurfaceMaterial,
    ) -> Self {
        if let Err(error) = self.add_box_hull(hull, transform, material) {
            self.error = Some(error);
        }
        self
    }

    /// Adds a generated box hull child and returns validation errors immediately.
    pub fn with_box_hull(
        mut self,
        hull: &'a BoxHull,
        transform: impl Into<Transform>,
        material: SurfaceMaterial,
    ) -> Result<Self> {
        self.add_box_hull(hull, transform, material)?;
        Ok(self)
    }

    /// Adds a generated box hull child to the builder in place.
    pub fn add_box_hull(
        &mut self,
        hull: &'a BoxHull,
        transform: impl Into<Transform>,
        material: SurfaceMaterial,
    ) -> Result<&mut Self> {
        self.add_hull_ptr(hull.hull_data(), transform.into(), material)
    }

    fn add_hull_ptr(
        &mut self,
        hull: *const ffi::b3HullData,
        transform: Transform,
        material: SurfaceMaterial,
    ) -> Result<&mut Self> {
        transform.validate()?;
        material.validate()?;
        if hull.is_null() {
            return Err(Error::NativeFailure);
        }
        self.validate_additional_child()?;
        self.hulls.push(ffi::b3CompoundHullDef {
            hull,
            transform: transform.into_raw(),
            material: material.into_raw(),
        });
        Ok(self)
    }

    /// Adds a mesh child, storing any validation error until `build`.
    pub fn mesh(
        mut self,
        mesh: &'a MeshData,
        transform: impl Into<Transform>,
        scale: impl Into<Vec3>,
        materials: impl AsRef<[SurfaceMaterial]>,
    ) -> Self {
        if let Err(error) = self.add_mesh(mesh, transform, scale, materials) {
            self.error = Some(error);
        }
        self
    }

    /// Adds a mesh child and returns validation errors immediately.
    pub fn with_mesh(
        mut self,
        mesh: &'a MeshData,
        transform: impl Into<Transform>,
        scale: impl Into<Vec3>,
        materials: impl AsRef<[SurfaceMaterial]>,
    ) -> Result<Self> {
        self.add_mesh(mesh, transform, scale, materials)?;
        Ok(self)
    }

    /// Adds a mesh child to the builder in place.
    ///
    /// `materials` must contain exactly `mesh.material_count()` entries and no
    /// more than `MAX_COMPOUND_MESH_MATERIALS` entries.
    pub fn add_mesh(
        &mut self,
        mesh: &'a MeshData,
        transform: impl Into<Transform>,
        scale: impl Into<Vec3>,
        materials: impl AsRef<[SurfaceMaterial]>,
    ) -> Result<&mut Self> {
        let transform = transform.into();
        let scale = validate_mesh_scale(scale.into())?;
        transform.validate()?;
        let materials = materials.as_ref();
        if materials.is_empty() || materials.len() > MAX_COMPOUND_MESH_MATERIALS {
            return Err(validation::invalid(
                "compound.mesh.materials",
                InvalidValueReason::OutOfRange,
            ));
        }
        if materials.len() != mesh.material_count() as usize {
            return Err(validation::invalid(
                "compound.mesh.materials",
                InvalidValueReason::InvalidCombination,
            ));
        }
        let raw_materials: Vec<_> = materials
            .iter()
            .copied()
            .map(|material| {
                material.validate()?;
                Ok(material.into_raw())
            })
            .collect::<Result<_>>()?;
        self.validate_additional_child()?;
        self.mesh_materials.push(raw_materials.into_boxed_slice());
        let material_ptr = self
            .mesh_materials
            .last()
            .expect("just pushed mesh materials")
            .as_ptr();
        self.meshes.push(ffi::b3CompoundMeshDef {
            meshData: mesh.as_ptr(),
            transform: transform.into_raw(),
            scale: scale.into_raw(),
            materials: material_ptr,
            materialCount: materials.len() as i32,
        });
        Ok(self)
    }

    /// Builds the compound data.
    pub fn build(mut self) -> Result<Compound> {
        callback_state::check_not_in_callback()?;
        if let Some(error) = self.error {
            return Err(error);
        }
        self.validate_child_capacity()?;
        if self.child_count() == 0 {
            return Err(validation::invalid(
                "compound.children",
                InvalidValueReason::OutOfRange,
            ));
        }
        let def = ffi::b3CompoundDef {
            capsules: self.capsules.as_mut_ptr(),
            capsuleCount: self.capsules.len() as i32,
            hulls: self.hulls.as_mut_ptr(),
            hullCount: self.hulls.len() as i32,
            meshes: self.meshes.as_mut_ptr(),
            meshCount: self.meshes.len() as i32,
            spheres: self.spheres.as_mut_ptr(),
            sphereCount: self.spheres.len() as i32,
        };
        Compound::from_native(|| unsafe { ffi::b3CreateCompound(&def) })
    }

    fn child_count(&self) -> usize {
        self.capsules.len() + self.hulls.len() + self.meshes.len() + self.spheres.len()
    }

    fn validate_child_capacity(&self) -> Result<()> {
        if self.child_count() < ffi::B3_MAX_CHILD_SHAPES as usize {
            Ok(())
        } else {
            Err(validation::invalid(
                "compound.children",
                InvalidValueReason::OutOfRange,
            ))
        }
    }

    fn validate_additional_child(&self) -> Result<()> {
        if self.child_count() + 1 < ffi::B3_MAX_CHILD_SHAPES as usize {
            Ok(())
        } else {
            Err(validation::invalid(
                "compound.children",
                InvalidValueReason::OutOfRange,
            ))
        }
    }
}

impl<'a> Default for CompoundBuilder<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Capsule child returned from a compound.
pub struct CompoundCapsule {
    /// Capsule geometry.
    pub capsule: Capsule,
    /// Index into the compound material table.
    pub material_index: i32,
}

impl CompoundCapsule {
    #[inline]
    const fn from_raw(raw: ffi::b3CompoundCapsule) -> Self {
        Self {
            capsule: Capsule::from_raw(raw.capsule),
            material_index: raw.materialIndex,
        }
    }
}

#[derive(Copy, Clone, Debug)]
/// Hull child returned from a compound.
pub struct CompoundHull<'a> {
    /// Borrowed hull geometry.
    pub hull: ShapeHull<'a>,
    /// Child transform in compound-local space.
    pub transform: Transform,
    /// Index into the compound material table.
    pub material_index: i32,
}

impl<'a> CompoundHull<'a> {
    fn from_raw(raw: ffi::b3CompoundHull) -> Result<Self> {
        unsafe { raw.hull.as_ref() }
            .map(|hull| Self {
                hull: ShapeHull::from_raw(hull),
                transform: Transform::from_raw(raw.transform),
                material_index: raw.materialIndex,
            })
            .ok_or(Error::NativeFailure)
    }
}

#[derive(Copy, Clone, Debug)]
/// Mesh child returned from a compound.
pub struct CompoundMesh<'a> {
    /// Borrowed mesh geometry and per-child scale.
    pub mesh: ShapeMesh<'a>,
    /// Child transform in compound-local space.
    pub transform: Transform,
    /// Material indices used by the mesh child's material slots.
    pub material_indices: [i32; MAX_COMPOUND_MESH_MATERIALS],
}

impl<'a> CompoundMesh<'a> {
    fn from_raw(raw: ffi::b3CompoundMesh) -> Result<Self> {
        ShapeMesh::from_raw(ffi::b3Mesh {
            data: raw.meshData,
            scale: raw.scale,
        })
        .map(|mesh| Self {
            mesh,
            transform: Transform::from_raw(raw.transform),
            material_indices: raw.materialIndices,
        })
        .ok_or(Error::NativeFailure)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Sphere child returned from a compound.
pub struct CompoundSphere {
    /// Sphere geometry.
    pub sphere: Sphere,
    /// Index into the compound material table.
    pub material_index: i32,
}

impl CompoundSphere {
    #[inline]
    const fn from_raw(raw: ffi::b3CompoundSphere) -> Self {
        Self {
            sphere: Sphere::from_raw(raw.sphere),
            material_index: raw.materialIndex,
        }
    }
}

#[derive(Copy, Clone, Debug)]
/// Flattened child returned from generic compound indexing and queries.
///
/// Hull and mesh children borrow native storage owned by the parent
/// [`Compound`]. Keep this value scoped to the compound borrow that produced it.
pub struct CompoundChild<'a> {
    /// Child geometry stored by a compound shape.
    pub shape: CompoundChildShape<'a>,
    /// Child transform in compound-local space.
    pub transform: Transform,
    /// Material indices used by the child.
    pub material_indices: [i32; MAX_COMPOUND_MESH_MATERIALS],
}

impl<'a> CompoundChild<'a> {
    fn from_raw(raw: ffi::b3ChildShape) -> Result<Self> {
        let shape = match ShapeType::from_raw(raw.type_) {
            Some(ShapeType::Capsule) => CompoundChildShape::Capsule(Capsule::from_raw(unsafe {
                raw.__bindgen_anon_1.capsule
            })),
            Some(ShapeType::Hull) => {
                let hull = unsafe { raw.__bindgen_anon_1.hull.as_ref() }
                    .map(ShapeHull::from_raw)
                    .ok_or(Error::NativeFailure)?;
                CompoundChildShape::Hull(hull)
            }
            Some(ShapeType::Mesh) => {
                let mesh = ShapeMesh::from_raw(unsafe { raw.__bindgen_anon_1.mesh })
                    .ok_or(Error::NativeFailure)?;
                CompoundChildShape::Mesh(mesh)
            }
            Some(ShapeType::Sphere) => {
                CompoundChildShape::Sphere(Sphere::from_raw(unsafe { raw.__bindgen_anon_1.sphere }))
            }
            _ => return Err(Error::NativeFailure),
        };
        Ok(Self {
            shape,
            transform: Transform::from_raw(raw.transform),
            material_indices: raw.materialIndices,
        })
    }

    #[inline]
    /// Returns the child shape type.
    pub const fn shape_type(&self) -> ShapeType {
        self.shape.shape_type()
    }

    #[inline]
    /// Returns the first material index for the child.
    pub const fn primary_material_index(&self) -> i32 {
        self.material_indices[0]
    }
}

#[derive(Copy, Clone, Debug)]
/// Hit returned by a compound AABB query.
///
/// The contained child borrows native storage owned by the queried
/// [`Compound`]. Do not retain it beyond the compound borrow.
pub struct CompoundQueryHit<'a> {
    /// Flattened child index hit by the query.
    pub child_index: i32,
    /// Borrowed child data for the hit.
    pub child: CompoundChild<'a>,
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
struct CompoundQueryContext<'a, F> {
    visitor: F,
    state: LocalCallbackState,
    _lifetime: PhantomData<&'a Compound>,
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
unsafe extern "C" fn compound_query_trampoline<'a, F>(
    compound: *const ffi::b3CompoundData,
    child_index: i32,
    context: *mut c_void,
) -> bool
where
    F: FnMut(CompoundQueryHit<'a>) -> bool,
{
    struct PanicFailureGuard<'a> {
        state: &'a mut LocalCallbackState,
        armed: bool,
    }

    impl PanicFailureGuard<'_> {
        fn fail<R>(&mut self, error: Error, fallback: R) -> R {
            let result = self.state.fail(error, fallback);
            self.armed = false;
            result
        }

        fn disarm(&mut self) {
            self.armed = false;
        }
    }

    impl Drop for PanicFailureGuard<'_> {
        fn drop(&mut self) {
            if self.armed {
                self.state.fail(Error::CallbackPanicked, ());
            }
        }
    }

    let ctx = unsafe { &mut *context.cast::<CompoundQueryContext<'a, F>>() };
    let mut boundary = LocalCallbackState::new();
    boundary.invoke(false, || {
        let CompoundQueryContext { visitor, state, .. } = ctx;
        let mut panic_failure = PanicFailureGuard { state, armed: true };
        if panic_failure.state.has_failed() {
            panic_failure.disarm();
            return false;
        }

        if compound.is_null() || child_index < 0 {
            return panic_failure.fail(Error::NativeFailure, false);
        }

        let raw_child = unsafe { ffi::b3GetCompoundChild(compound, child_index) };
        let child = match CompoundChild::from_raw(raw_child) {
            Ok(child) => child,
            Err(error) => return panic_failure.fail(error, false),
        };
        let hit = CompoundQueryHit { child_index, child };
        let result = visitor(hit);
        panic_failure.disarm();
        result
    })
}

#[derive(Copy, Clone, Debug)]
/// Shape variants that can appear inside a compound child.
pub enum CompoundChildShape<'a> {
    /// Capsule child geometry.
    Capsule(Capsule),
    /// Hull child geometry.
    Hull(ShapeHull<'a>),
    /// Mesh child geometry.
    Mesh(ShapeMesh<'a>),
    /// Sphere child geometry.
    Sphere(Sphere),
}

impl<'a> CompoundChildShape<'a> {
    #[inline]
    /// Returns the shape type represented by this variant.
    pub const fn shape_type(&self) -> ShapeType {
        match self {
            Self::Capsule(_) => ShapeType::Capsule,
            Self::Hull(_) => ShapeType::Hull,
            Self::Mesh(_) => ShapeType::Mesh,
            Self::Sphere(_) => ShapeType::Sphere,
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Box3D shape type tag.
pub enum ShapeType {
    /// Capsule shape.
    Capsule,
    /// Compound shape.
    Compound,
    /// Height-field shape.
    HeightField,
    /// Convex hull shape.
    Hull,
    /// Triangle mesh shape.
    Mesh,
    /// Sphere shape.
    Sphere,
}

impl ShapeType {
    /// Converts a raw Box3D shape type into the safe enum.
    pub const fn from_raw(raw: ffi::b3ShapeType) -> Option<Self> {
        match raw {
            ffi::b3ShapeType_b3_capsuleShape => Some(Self::Capsule),
            ffi::b3ShapeType_b3_compoundShape => Some(Self::Compound),
            ffi::b3ShapeType_b3_heightShape => Some(Self::HeightField),
            ffi::b3ShapeType_b3_hullShape => Some(Self::Hull),
            ffi::b3ShapeType_b3_meshShape => Some(Self::Mesh),
            ffi::b3ShapeType_b3_sphereShape => Some(Self::Sphere),
            _ => None,
        }
    }
}
