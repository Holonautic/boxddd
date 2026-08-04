use crate::TaskSystem;
use crate::body::{BodyDef, BodyType};
use crate::callbacks::WorldCallbacks;
use crate::core::foundation::{Foundation, OrdinaryLease};
use crate::core::{callback_state, debug_checks, ffi_vec, task_system, validation};
use crate::debug_draw::DebugShapeRegistry;
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use crate::debug_draw::take_provider_debug_error;
use crate::error::{Error, Result};
use crate::events::EventScratch;
use crate::recording::RecordingActivity;
use crate::shapes::{
    BoxHull, Capsule, Compound, HeightField, Hull, MeshData, PreparedShapeDef, ShapeDef,
    ShapeHeightField, ShapeHull, ShapeMaterialUsage, ShapeMesh, ShapeType, Sphere, SurfaceMaterial,
    validate_mesh_scale,
};
use crate::types::{
    Aabb, BodyId, Capacity, ContactData, ContactId, Counters, Filter, JointId, MassData, Matrix3,
    MotionLocks, Pos, Profile, Quat, ShapeId, Vec3, Version, WorldTransform,
};
use boxddd_sys::ffi;
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::rc::Rc;

/// Configuration used when creating a Box3D world.
#[derive(Clone, Debug)]
pub struct WorldDef {
    /// World gravity vector.
    pub gravity: Vec3,
    /// Minimum relative speed that enables restitution.
    pub restitution_threshold: f32,
    /// Minimum relative speed that produces hit events.
    pub hit_event_threshold: f32,
    /// Contact constraint frequency in hertz.
    pub contact_hertz: f32,
    /// Non-negative contact damping ratio.
    pub contact_damping_ratio: f32,
    /// Maximum contact overlap recovery speed.
    pub contact_speed: f32,
    /// Positive maximum body linear speed.
    pub maximum_linear_speed: f32,
    /// Whether bodies may sleep.
    pub enable_sleep: bool,
    /// Whether continuous collision detection is enabled.
    pub enable_continuous: bool,
    /// Number of worker slots; zero selects Box3D's default single worker.
    pub worker_count: u32,
    /// Optional initial native allocation capacities.
    pub capacity: Capacity,
    /// Optional Rust task-system adapter retained by the world.
    pub task_system: Option<TaskSystem>,
}

impl WorldDef {
    pub(crate) fn with_length_units_per_meter(length_units: f32) -> Self {
        Self {
            gravity: Vec3::new(0.0, -10.0, 0.0),
            restitution_threshold: length_units,
            hit_event_threshold: length_units,
            contact_hertz: 30.0,
            contact_damping_ratio: 10.0,
            contact_speed: 3.0 * length_units,
            maximum_linear_speed: 400.0 * length_units,
            enable_sleep: true,
            enable_continuous: true,
            worker_count: 0,
            capacity: Capacity::default(),
            task_system: None,
        }
    }

    /// Validates the value before it is passed to Box3D.
    pub fn validate(&self) -> Result<()> {
        validation::vec3("world.gravity", self.gravity)?;
        validation::nonnegative("world.restitution_threshold", self.restitution_threshold)?;
        validation::nonnegative("world.hit_event_threshold", self.hit_event_threshold)?;
        validation::nonnegative("world.contact_hertz", self.contact_hertz)?;
        validation::nonnegative("world.contact_damping_ratio", self.contact_damping_ratio)?;
        validation::nonnegative("world.contact_speed", self.contact_speed)?;
        validation::positive("world.maximum_linear_speed", self.maximum_linear_speed)?;
        if self.worker_count > ffi::B3_MAX_WORKERS {
            return Err(validation::invalid(
                "world.worker_count",
                crate::error::InvalidValueReason::OutOfRange,
            ));
        }
        if self.task_system.is_some() && self.worker_count == 0 {
            return Err(validation::invalid(
                "world.task_system",
                crate::error::InvalidValueReason::InvalidCombination,
            ));
        }
        for (context, value) in [
            (
                "world.capacity.static_shape_count",
                self.capacity.static_shape_count,
            ),
            (
                "world.capacity.dynamic_shape_count",
                self.capacity.dynamic_shape_count,
            ),
            (
                "world.capacity.static_body_count",
                self.capacity.static_body_count,
            ),
            (
                "world.capacity.dynamic_body_count",
                self.capacity.dynamic_body_count,
            ),
            ("world.capacity.contact_count", self.capacity.contact_count),
        ] {
            if value < 0 {
                return Err(validation::invalid(
                    context,
                    crate::error::InvalidValueReason::OutOfRange,
                ));
            }
        }
        self.capacity
            .static_body_count
            .checked_add(self.capacity.dynamic_body_count)
            .ok_or_else(|| {
                validation::invalid(
                    "world.capacity.body_count",
                    crate::error::InvalidValueReason::OutOfRange,
                )
            })?;
        self.capacity
            .static_shape_count
            .checked_add(self.capacity.dynamic_shape_count)
            .ok_or_else(|| {
                validation::invalid(
                    "world.capacity.shape_count",
                    crate::error::InvalidValueReason::OutOfRange,
                )
            })?;
        self.capacity.contact_count.checked_mul(2).ok_or_else(|| {
            validation::invalid(
                "world.capacity.contact_count",
                crate::error::InvalidValueReason::OutOfRange,
            )
        })?;
        Ok(())
    }

    fn validate_platform(&self) -> Result<()> {
        #[cfg(target_arch = "wasm32")]
        if self.worker_count > 1 || self.task_system.is_some() {
            return Err(Error::UnsupportedOnWasm);
        }
        Ok(())
    }
}

/// Builder for `WorldDef`.
#[derive(Clone, Debug)]
pub struct WorldDefBuilder {
    def: WorldDef,
}

impl WorldDefBuilder {
    pub(crate) fn from_def(def: WorldDef) -> Self {
        Self { def }
    }

    /// Sets the gravity vector used by the world, usually in meters per second squared.
    #[inline]
    pub fn gravity(mut self, gravity: impl Into<Vec3>) -> Self {
        self.def.gravity = gravity.into();
        self
    }

    /// Sets the number of worker slots Box3D may use while stepping the world.
    #[inline]
    pub fn worker_count(mut self, worker_count: u32) -> Self {
        self.def.worker_count = if self.def.task_system.is_some() {
            worker_count.max(1)
        } else {
            worker_count
        };
        self
    }

    /// Installs the task-system adapter used by Box3D during `World::step`.
    #[inline]
    pub fn task_system(mut self, task_system: TaskSystem) -> Self {
        if self.def.worker_count == 0 {
            self.def.worker_count = 1;
        }
        self.def.task_system = Some(task_system);
        self
    }

    /// Sets the restitution speed threshold.
    pub fn restitution_threshold(mut self, threshold: f32) -> Self {
        self.def.restitution_threshold = threshold;
        self
    }

    /// Sets the hit-event speed threshold.
    pub fn hit_event_threshold(mut self, threshold: f32) -> Self {
        self.def.hit_event_threshold = threshold;
        self
    }

    /// Sets contact solver tuning and maximum recovery speed.
    pub fn contact_tuning(mut self, hertz: f32, damping_ratio: f32, speed: f32) -> Self {
        self.def.contact_hertz = hertz;
        self.def.contact_damping_ratio = damping_ratio;
        self.def.contact_speed = speed;
        self
    }

    /// Sets the maximum linear speed for bodies.
    pub fn maximum_linear_speed(mut self, speed: f32) -> Self {
        self.def.maximum_linear_speed = speed;
        self
    }

    /// Enables or disables body sleeping.
    pub fn enable_sleep(mut self, enabled: bool) -> Self {
        self.def.enable_sleep = enabled;
        self
    }

    /// Enables or disables continuous collision detection.
    pub fn enable_continuous(mut self, enabled: bool) -> Self {
        self.def.enable_continuous = enabled;
        self
    }

    /// Sets optional initial allocation capacities.
    pub fn capacity(mut self, capacity: Capacity) -> Self {
        self.def.capacity = capacity;
        self
    }

    /// Validates and finishes the world definition.
    #[inline]
    pub fn build(self) -> Result<WorldDef> {
        self.def.validate()?;
        Ok(self.def)
    }
}

/// Configuration for applying an explosion impulse in a world.
#[derive(Clone, Debug)]
pub struct ExplosionDef {
    /// Collision mask selecting affected shapes.
    pub mask_bits: u64,
    /// Explosion center in world space.
    pub position: Pos,
    /// Non-negative inner radius receiving full impulse.
    pub radius: f32,
    /// Non-negative falloff distance beyond the radius.
    pub falloff: f32,
    /// Impulse per unit facing area; negative values produce implosions.
    pub impulse_per_area: f32,
}

impl Default for ExplosionDef {
    fn default() -> Self {
        Self {
            mask_bits: u64::MAX,
            position: Pos::ZERO,
            radius: 0.0,
            falloff: 0.0,
            impulse_per_area: 0.0,
        }
    }
}

impl ExplosionDef {
    /// Starts a builder with Box3D defaults.
    #[inline]
    pub fn builder() -> ExplosionDefBuilder {
        ExplosionDefBuilder::new()
    }

    /// Validates the value before it is passed to Box3D.
    pub fn validate(&self) -> Result<()> {
        validation::position("explosion.position", self.position)?;
        validation::nonnegative("explosion.radius", self.radius)?;
        validation::nonnegative("explosion.falloff", self.falloff)?;
        validation::finite("explosion.extent", self.radius + self.falloff)?;
        validation::finite("explosion.impulse_per_area", self.impulse_per_area)
    }

    pub(crate) fn lower(&self) -> Result<ffi::b3ExplosionDef> {
        self.validate()?;
        Ok(ffi::b3ExplosionDef {
            maskBits: self.mask_bits,
            position: self.position.into_raw(),
            radius: self.radius,
            falloff: self.falloff,
            impulsePerArea: self.impulse_per_area,
        })
    }
}

/// Builder for `ExplosionDef`.
#[derive(Clone, Debug)]
pub struct ExplosionDefBuilder {
    def: ExplosionDef,
}

impl ExplosionDefBuilder {
    /// Creates a new value with default settings.
    #[inline]
    pub fn new() -> Self {
        Self {
            def: ExplosionDef::default(),
        }
    }

    /// Sets the collision mask bits matched by the query.
    #[inline]
    pub fn mask_bits(mut self, mask_bits: u64) -> Self {
        self.def.mask_bits = mask_bits;
        self
    }

    /// Sets the world-space center of the explosion.
    #[inline]
    pub fn position(mut self, position: impl Into<Pos>) -> Self {
        self.def.position = position.into();
        self
    }

    /// Sets the explosion radius.
    #[inline]
    pub fn radius(mut self, radius: f32) -> Self {
        self.def.radius = radius;
        self
    }

    /// Sets how quickly the explosion impulse decays over distance.
    #[inline]
    pub fn falloff(mut self, falloff: f32) -> Self {
        self.def.falloff = falloff;
        self
    }

    /// Sets the impulse applied per affected surface area.
    #[inline]
    pub fn impulse_per_area(mut self, impulse_per_area: f32) -> Self {
        self.def.impulse_per_area = impulse_per_area;
        self
    }

    /// Validates and finishes the explosion definition.
    #[inline]
    pub fn build(self) -> Result<ExplosionDef> {
        self.def.validate()?;
        Ok(self.def)
    }
}

impl Default for ExplosionDefBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a World step that reached and advanced the native simulation.
///
/// A value of this type proves that Box3D was called and Rust-side post-step
/// finalizers committed. Callback or task failures discovered after native
/// advancement are retained separately and can be inspected or consumed.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutcome {
    post_step_error: Option<Error>,
}

impl StepOutcome {
    pub(crate) fn from_post_step_result(result: Result<()>) -> Self {
        Self {
            post_step_error: result.err(),
        }
    }

    /// Returns the callback or task failure reported after native advancement.
    #[inline]
    pub const fn post_step_error(&self) -> Option<&Error> {
        self.post_step_error.as_ref()
    }

    /// Projects the advanced outcome to the traditional result-first contract.
    #[inline]
    pub fn into_result(self) -> Result<()> {
        match self.post_step_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Owning handle for a Box3D world.
#[derive(Debug)]
pub struct World {
    owner: Option<Box<WorldOwner>>,
}

#[derive(Debug)]
struct WorldOwner {
    raw: ffi::b3WorldId,
    pub(crate) state: WorldState,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

#[derive(Debug)]
pub(crate) struct WorldState {
    phase: WorldPhase,
    pub(crate) poisoned: Cell<bool>,
    pub(crate) ledger: WorldLedger,
    pub(crate) backing_quarantine: Vec<ShapeResource>,
    pub(crate) callbacks: WorldCallbacks,
    pub(crate) event_scratch: EventScratch,
    _task_context: Option<Box<task_system::InstalledTaskContext>>,
    pub(crate) active_recording: Option<Rc<RecordingActivity>>,
    pub(crate) debug_shapes: Box<DebugShapeRegistry>,
    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
    pub(crate) provider_debug_shapes_token: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum WorldPhase {
    Live,
    Dropping,
    Destroyed,
}

#[derive(Debug)]
pub(crate) enum ShapeResource {
    Mesh { _data: MeshData },
    HeightField { _data: HeightField },
    Compound { _data: Compound },
}

mod body_api;
mod creation;
pub(crate) mod creation_transaction;
pub(crate) mod ledger;
mod lifecycle;
mod runtime;
mod shape_api;

use ledger::WorldLedger;

impl World {
    #[inline]
    fn owner(&self) -> &WorldOwner {
        self.owner
            .as_deref()
            .expect("World owner is present outside destruction")
    }

    #[inline]
    fn owner_mut(&mut self) -> &mut WorldOwner {
        self.owner
            .as_deref_mut()
            .expect("World owner is present outside destruction")
    }

    #[inline]
    pub(crate) fn state(&self) -> &WorldState {
        &self.owner().state
    }

    #[inline]
    pub(crate) fn state_mut(&mut self) -> &mut WorldState {
        &mut self.owner_mut().state
    }

    #[inline]
    pub(crate) fn raw(&self) -> ffi::b3WorldId {
        self.owner().raw
    }

    /// Returns whether `body_id` currently belongs to this world.
    ///
    /// Stale and foreign IDs return `Ok(false)`. Callback reentry and terminal
    /// owner poison remain visible as errors.
    pub fn contains_body(&self, body_id: BodyId) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        match self.state().ledger.authorize_body(body_id) {
            Ok(_) => Ok(true),
            Err(Error::ForeignHandle { .. } | Error::StaleHandle { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Returns whether `shape_id` currently belongs to this world.
    ///
    /// Stale and foreign IDs return `Ok(false)`. Callback reentry and terminal
    /// owner poison remain visible as errors.
    pub fn contains_shape(&self, shape_id: ShapeId) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        match self.state().ledger.authorize_shape(shape_id) {
            Ok(_) => Ok(true),
            Err(Error::ForeignHandle { .. } | Error::StaleHandle { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Returns whether `joint_id` currently belongs to this world.
    ///
    /// Stale and foreign IDs return `Ok(false)`. Callback reentry and terminal
    /// owner poison remain visible as errors.
    pub fn contains_joint(&self, joint_id: JointId) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        match self.state().ledger.authorize_joint(joint_id) {
            Ok(_) => Ok(true),
            Err(Error::ForeignHandle { .. } | Error::StaleHandle { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Returns whether `contact_id` is live in the current contact epoch.
    pub fn contains_contact(&self, contact_id: ContactId) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let Ok(raw) = self.state().ledger.authorize_contact(contact_id) else {
            return Ok(false);
        };
        let _call = self.enter_call()?;
        self.check_world_valid()?;
        Ok(unsafe { ffi::b3Contact_IsValid(raw) })
    }

    pub(crate) fn enter_call(&self) -> Result<callback_state::OwnerCallFrame> {
        self.owner()._foundation_lease.enter_call()
    }

    pub(crate) fn check_world_valid(&self) -> Result<()> {
        self.check_owner_healthy()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        if let Some(error) = take_provider_debug_error(self.state().provider_debug_shapes_token) {
            self.state().poisoned.set(true);
            return Err(error);
        }
        if self.state().phase == WorldPhase::Live && unsafe { ffi::b3World_IsValid(self.raw()) } {
            Ok(())
        } else {
            Err(Error::NativeFailure)
        }
    }

    pub(crate) fn enter_world_call(&self) -> Result<callback_state::OwnerCallFrame> {
        let call = self.enter_call()?;
        self.check_world_valid()?;
        Ok(call)
    }

    pub(crate) fn check_owner_healthy(&self) -> Result<()> {
        self.owner()
            ._foundation_lease
            .foundation()
            .ensure_healthy()?;
        if self.state().poisoned.get() || self.state().debug_shapes.is_poisoned() {
            Err(Error::OwnerPoisoned)
        } else {
            Ok(())
        }
    }

    #[inline]
    pub(crate) fn enter_body_call(
        &self,
        body_id: BodyId,
    ) -> Result<callback_state::OwnerCallFrame> {
        callback_state::check_not_in_callback()?;
        let raw = self.state().ledger.authorize_body(body_id)?;
        let call = self.enter_call()?;
        self.check_world_valid()?;
        debug_checks::check_body_valid_raw(raw)?;
        Ok(call)
    }

    #[inline]
    pub(crate) fn enter_shape_call(
        &self,
        shape_id: ShapeId,
    ) -> Result<callback_state::OwnerCallFrame> {
        callback_state::check_not_in_callback()?;
        let raw = self.state().ledger.authorize_shape(shape_id)?;
        let call = self.enter_call()?;
        self.check_world_valid()?;
        debug_checks::check_shape_valid_raw(raw)?;
        Ok(call)
    }

    #[inline]
    pub(crate) fn enter_joint_call(
        &self,
        joint_id: JointId,
    ) -> Result<callback_state::OwnerCallFrame> {
        callback_state::check_not_in_callback()?;
        let raw = self.state().ledger.authorize_joint(joint_id)?;
        let call = self.enter_call()?;
        self.check_world_valid()?;
        debug_checks::check_joint_valid_raw(raw)?;
        Ok(call)
    }
}

/// Returns the Box3D runtime version.
#[inline]
pub fn version() -> Result<Version> {
    let _call = Foundation::enter_transient_call()?;
    Ok(Version::from_raw(unsafe { ffi::b3GetVersion() }))
}

/// Returns the number of bytes currently allocated by Box3D.
#[inline]
pub fn allocated_byte_count() -> Result<i32> {
    let _call = Foundation::enter_transient_call()?;
    Ok(unsafe { ffi::b3GetByteCount() })
}

/// Returns whether the linked Box3D library uses double precision.
#[inline]
pub const fn is_double_precision() -> bool {
    cfg!(feature = "double-precision")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DistanceJointDef;
    use crate::shapes::{ShapeDropEvent, begin_shape_drop_trace, take_shape_drop_trace};
    use crate::world::creation_transaction::{
        BodyNative, CreationStage, NativeClaim, UnclaimedNative, force_creation_failure,
        force_next_compensation_mismatch, force_next_joint_endpoints, force_next_shape_parent,
    };
    use crate::world::ledger::IdentityClassification;

    fn foundation() -> &'static Foundation {
        Foundation::initialize_default().unwrap()
    }

    fn static_body(world: &mut World) -> BodyId {
        world
            .create_body(
                foundation()
                    .body_def_builder()
                    .body_type(BodyType::Static)
                    .build()
                    .unwrap(),
            )
            .unwrap()
    }

    fn dynamic_body(world: &mut World, x: f32) -> BodyId {
        world
            .create_body(
                foundation()
                    .body_def_builder()
                    .body_type(BodyType::Dynamic)
                    .position([x, 0.0, 0.0])
                    .build()
                    .unwrap(),
            )
            .unwrap()
    }

    #[derive(Clone, Copy)]
    enum BackingFamily {
        Mesh,
        HeightField,
        Compound,
    }

    impl BackingFamily {
        fn drop_event(self) -> ShapeDropEvent {
            match self {
                Self::Mesh => ShapeDropEvent::MeshBacking,
                Self::HeightField => ShapeDropEvent::HeightFieldBacking,
                Self::Compound => ShapeDropEvent::CompoundBacking,
            }
        }
    }

    fn create_backed_shape(
        world: &mut World,
        body: BodyId,
        family: BackingFamily,
    ) -> Result<ShapeId> {
        match family {
            BackingFamily::Mesh => world.create_mesh_shape(
                body,
                &foundation().shape_def(),
                MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true)?,
                [1.0, 1.0, 1.0],
            ),
            BackingFamily::HeightField => world.create_height_field_shape(
                body,
                &foundation().shape_def(),
                HeightField::grid(2, 2, [1.0, 1.0, 1.0], false)?,
            ),
            BackingFamily::Compound => world.create_compound_shape(
                body,
                &foundation().shape_def(),
                Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.25), SurfaceMaterial::default())?,
            ),
        }
    }

    fn native_counters(world: &World) -> Counters {
        let _call = world.enter_call().unwrap();
        Counters::from_raw(unsafe { ffi::b3World_GetCounters(world.raw()) })
    }

    #[test]
    fn shape_resources_are_removed_on_shape_destroy_and_mesh_replace() {
        let mut world = foundation().create_world(foundation().world_def()).unwrap();
        let body = static_body(&mut world);
        let shape = world
            .create_mesh_shape(
                body,
                &foundation().shape_def(),
                MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true).unwrap(),
                [1.0, 1.0, 1.0],
            )
            .unwrap();
        assert_eq!(world.state().ledger.shape_resource_count(), 1);

        world
            .set_shape_mesh(
                shape,
                MeshData::box_mesh(Vec3::ZERO, [0.5, 0.5, 0.5], true).unwrap(),
                [1.0, 1.0, 1.0],
            )
            .unwrap();
        assert_eq!(world.state().ledger.shape_resource_count(), 1);

        world
            .set_shape_sphere(shape, &Sphere::new(Vec3::ZERO, 0.25))
            .unwrap();
        assert_eq!(world.state().ledger.shape_resource_count(), 0);

        world.destroy_shape(shape, true).unwrap();
        assert_eq!(world.state().ledger.shape_resource_count(), 0);
    }

    #[test]
    fn resource_backed_shapes_keep_body_static() {
        let mut world = foundation().create_world(foundation().world_def()).unwrap();
        let body = static_body(&mut world);
        world
            .create_height_field_shape(
                body,
                &foundation().shape_def(),
                HeightField::grid(2, 2, [1.0, 1.0, 1.0], false).unwrap(),
            )
            .unwrap();

        assert_eq!(
            world.set_body_type(body, BodyType::Dynamic).unwrap_err(),
            Error::InvalidValue {
                context: "body.type",
                reason: crate::error::InvalidValueReason::InvalidCombination,
            }
        );
        assert_eq!(world.body_type(body).unwrap(), BodyType::Static);

        world
            .create_sphere_shape(
                body,
                &foundation().shape_def(),
                &Sphere::new(Vec3::new(2.0, 0.0, 0.0), 0.25),
            )
            .unwrap();
    }

    #[test]
    fn shape_resources_are_removed_on_body_destroy() {
        let mut world = foundation().create_world(foundation().world_def()).unwrap();
        let body = static_body(&mut world);
        world
            .create_mesh_shape(
                body,
                &foundation().shape_def(),
                MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true).unwrap(),
                [1.0, 1.0, 1.0],
            )
            .unwrap();
        world
            .create_height_field_shape(
                body,
                &foundation().shape_def(),
                HeightField::grid(2, 2, [1.0, 1.0, 1.0], false).unwrap(),
            )
            .unwrap();
        world
            .create_compound_shape(
                body,
                &foundation().shape_def(),
                Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.25), SurfaceMaterial::default())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(world.state().ledger.shape_resource_count(), 3);

        world.destroy_body(body).unwrap();
        assert_eq!(world.state().ledger.shape_resource_count(), 0);
    }

    #[test]
    fn creation_transaction_body_and_shape_compensate_every_fallible_stage() {
        let stages = [
            CreationStage::AfterClaim,
            CreationStage::AfterBind,
            CreationStage::AfterPostflight,
            CreationStage::BeforePublish,
        ];

        for stage in stages {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let before = world.counters().unwrap();
            force_creation_failure(stage);
            assert_eq!(
                world.create_body(foundation().body_def()).unwrap_err(),
                Error::NativeFailure
            );
            assert_eq!(world.counters().unwrap().body_count, before.body_count);
            assert!(world.create_body(foundation().body_def()).is_ok());
        }

        for family in [
            BackingFamily::Mesh,
            BackingFamily::HeightField,
            BackingFamily::Compound,
        ] {
            for stage in stages {
                let mut world = foundation().create_world(foundation().world_def()).unwrap();
                let body = static_body(&mut world);
                let before = world.counters().unwrap();
                begin_shape_drop_trace();
                force_creation_failure(stage);
                assert_eq!(
                    create_backed_shape(&mut world, body, family).unwrap_err(),
                    Error::NativeFailure
                );
                assert_eq!(world.counters().unwrap().shape_count, before.shape_count);
                assert_eq!(world.state().ledger.shape_resource_count(), 0);
                assert!(world.state().backing_quarantine.is_empty());
                assert_eq!(
                    take_shape_drop_trace(),
                    vec![ShapeDropEvent::NativeShape, family.drop_event(),]
                );
                assert!(create_backed_shape(&mut world, body, family).is_ok());
                assert_eq!(world.state().ledger.shape_resource_count(), 1);
            }
        }
    }

    #[test]
    fn creation_transaction_joint_rollback_poisons_owner() {
        for stage in [
            CreationStage::AfterClaim,
            CreationStage::AfterBind,
            CreationStage::AfterPostflight,
            CreationStage::BeforePublish,
        ] {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let body_a = dynamic_body(&mut world, -1.0);
            let body_b = dynamic_body(&mut world, 1.0);
            let shape = world
                .create_sphere_shape(
                    body_a,
                    &foundation().shape_def(),
                    &Sphere::new(Vec3::ZERO, 0.5),
                )
                .unwrap();
            let retained_joint = world
                .create_distance_joint(DistanceJointDef::new(body_a, body_b).length(2.0))
                .unwrap();
            let before = native_counters(&world);

            force_creation_failure(stage);
            assert_eq!(
                world
                    .create_distance_joint(DistanceJointDef::new(body_a, body_b).length(3.0))
                    .unwrap_err(),
                Error::NativeFailure
            );
            assert_eq!(native_counters(&world).joint_count, before.joint_count);
            assert_eq!(world.body_type(body_a), Err(Error::OwnerPoisoned));
            assert_eq!(world.contains_body(body_a), Err(Error::OwnerPoisoned));
            assert_eq!(world.contains_shape(shape), Err(Error::OwnerPoisoned));
            assert_eq!(
                world.contains_joint(retained_joint),
                Err(Error::OwnerPoisoned)
            );
        }
    }

    #[test]
    fn creation_transaction_commit_disarm_fault_poisons_published_owner() {
        {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let before = native_counters(&world);
            force_creation_failure(CreationStage::BeforeCommitDisarm);
            assert_eq!(
                world.create_body(foundation().body_def()).unwrap_err(),
                Error::NativeFailure
            );
            assert!(world.state().poisoned.get());
            assert_eq!(native_counters(&world).body_count, before.body_count);
            assert_eq!(
                world.create_body(foundation().body_def()).unwrap_err(),
                Error::OwnerPoisoned
            );
        }

        {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let body = static_body(&mut world);
            let before = native_counters(&world);
            begin_shape_drop_trace();
            force_creation_failure(CreationStage::BeforeCommitDisarm);
            assert_eq!(
                create_backed_shape(&mut world, body, BackingFamily::Mesh).unwrap_err(),
                Error::NativeFailure
            );
            assert!(world.state().poisoned.get());
            assert_eq!(native_counters(&world).shape_count, before.shape_count);
            assert_eq!(world.state().ledger.shape_resource_count(), 1);
            assert!(world.state().backing_quarantine.is_empty());
            assert_eq!(world.contains_body(body), Err(Error::OwnerPoisoned));
        }
        assert_eq!(
            take_shape_drop_trace(),
            vec![ShapeDropEvent::NativeShape, ShapeDropEvent::MeshBacking,]
        );

        {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let body_a = dynamic_body(&mut world, -1.0);
            let body_b = dynamic_body(&mut world, 1.0);
            let before = native_counters(&world);
            force_creation_failure(CreationStage::BeforeCommitDisarm);
            assert_eq!(
                world
                    .create_distance_joint(DistanceJointDef::new(body_a, body_b).length(2.0))
                    .unwrap_err(),
                Error::NativeFailure
            );
            assert!(world.state().poisoned.get());
            assert_eq!(native_counters(&world).joint_count, before.joint_count);
            assert_eq!(world.contains_body(body_a), Err(Error::OwnerPoisoned));
        }
    }

    #[test]
    fn creation_transaction_wrong_shape_relation_compensates_before_backing_drop() {
        let mut world = foundation().create_world(foundation().world_def()).unwrap();
        let body_a = static_body(&mut world);
        let body_b = static_body(&mut world);
        let before = world.counters().unwrap();
        begin_shape_drop_trace();
        force_next_shape_parent(body_b.into_raw());
        assert_eq!(
            create_backed_shape(&mut world, body_a, BackingFamily::Mesh).unwrap_err(),
            Error::NativeFailure
        );
        assert_eq!(world.counters().unwrap().shape_count, before.shape_count);
        assert_eq!(world.state().ledger.shape_resource_count(), 0);
        assert!(world.state().backing_quarantine.is_empty());
        assert_eq!(
            take_shape_drop_trace(),
            vec![ShapeDropEvent::NativeShape, ShapeDropEvent::MeshBacking,]
        );
        assert!(create_backed_shape(&mut world, body_a, BackingFamily::Mesh).is_ok());
    }

    #[test]
    fn creation_transaction_wrong_joint_relation_poisons_owner() {
        let mut world = foundation().create_world(foundation().world_def()).unwrap();
        let body_c = dynamic_body(&mut world, -1.0);
        let body_d = dynamic_body(&mut world, 1.0);
        let before = native_counters(&world);
        force_next_joint_endpoints(body_d.into_raw(), body_c.into_raw());
        assert_eq!(
            world
                .create_distance_joint(DistanceJointDef::new(body_c, body_d).length(2.0))
                .unwrap_err(),
            Error::NativeFailure
        );
        assert!(world.state().poisoned.get());
        assert_eq!(native_counters(&world).joint_count, before.joint_count);
        assert_eq!(world.body_joints(body_c), Err(Error::OwnerPoisoned));
        assert_eq!(world.body_joints(body_d), Err(Error::OwnerPoisoned));
    }

    #[test]
    fn creation_transaction_rejects_empty_invalid_active_and_foreign_outputs() {
        let mut world = foundation().create_world(foundation().world_def()).unwrap();
        let raw_world = world.raw();
        let empty = ffi::b3BodyId {
            index1: 0,
            world0: raw_world.index1 - 1,
            generation: 0,
        };
        let empty_identity = world.state().ledger.classify_body(empty);
        assert!(matches!(
            UnclaimedNative::<BodyNative>::new(empty, (), raw_world, &world.state().poisoned)
                .claim(empty_identity),
            Err(Error::ObjectIdentityExhausted)
        ));
        assert!(!world.state().poisoned.get());

        let invalid = ffi::b3BodyId {
            index1: i32::MAX,
            world0: raw_world.index1 - 1,
            generation: u16::MAX,
        };
        let invalid_identity = world.state().ledger.classify_body(invalid);
        let _call = world.enter_call().unwrap();
        assert!(matches!(
            UnclaimedNative::<BodyNative>::new(invalid, (), raw_world, &world.state().poisoned)
                .claim(invalid_identity),
            Err(Error::ObjectIdentityExhausted)
        ));
        drop(_call);
        assert!(!world.state().poisoned.get());

        let invalid_wrong_world = ffi::b3BodyId {
            index1: i32::MAX,
            world0: u16::MAX,
            generation: u16::MAX,
        };
        let invalid_identity = world.state().ledger.classify_body(invalid_wrong_world);
        let _call = world.enter_call().unwrap();
        assert!(matches!(
            UnclaimedNative::<BodyNative>::new(
                invalid_wrong_world,
                (),
                raw_world,
                &world.state().poisoned,
            )
            .claim(invalid_identity),
            Err(Error::ObjectIdentityExhausted)
        ));
        drop(_call);
        assert!(!world.state().poisoned.get());

        let body = world.create_body(foundation().body_def()).unwrap();
        let raw_body = body.into_raw();
        let active_identity = world.state().ledger.classify_body(raw_body);
        let _call = world.enter_call().unwrap();
        assert!(matches!(
            UnclaimedNative::<BodyNative>::new(raw_body, (), raw_world, &world.state().poisoned)
                .claim(active_identity),
            Err(Error::OwnerPoisoned)
        ));
        assert!(unsafe { ffi::b3Body_IsValid(raw_body) });
        drop(_call);
        drop(world);

        let mut source = foundation().create_world(foundation().world_def()).unwrap();
        let raw_body = source
            .create_body(foundation().body_def())
            .unwrap()
            .into_raw();
        let target = foundation().create_world(foundation().world_def()).unwrap();
        let identity = target.state().ledger.classify_body(raw_body);
        let _call = target.enter_call().unwrap();
        assert!(matches!(
            UnclaimedNative::<BodyNative>::new(
                raw_body,
                (),
                target.raw(),
                &target.state().poisoned,
            )
            .claim(identity),
            Err(Error::OwnerPoisoned)
        ));
        assert!(unsafe { ffi::b3Body_IsValid(raw_body) });
    }

    #[test]
    fn creation_transaction_compensates_retired_candidate_and_poisons_on_mismatch() {
        fn check_retired_collision(rotate_to_visible: bool) {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let target_world = world.raw();
            let pending = world.state_mut().ledger.reserve_body().unwrap();
            let _call = world.enter_call().unwrap();
            let raw = foundation()
                .body_def()
                .prepare()
                .unwrap()
                .create(target_world);
            let WorldState {
                poisoned, ledger, ..
            } = world.state_mut();
            let IdentityClassification::Available(available) = ledger.classify_body(raw) else {
                panic!("fresh test body identity is available")
            };
            let NativeClaim {
                candidate,
                available: claimed,
            } = UnclaimedNative::<BodyNative>::new(raw, (), target_world, poisoned)
                .claim(IdentityClassification::Available(available))
                .unwrap();
            let bound = ledger.bind_body(raw, claimed, pending).unwrap();
            let retired = candidate
                .bind()
                .commit(|_| ledger.publish_body(bound))
                .unwrap();
            let cascade = ledger.prepare_body_cascade(retired).unwrap();
            ledger.finish_body_cascade(retired, cascade);
            if rotate_to_visible {
                let next = ledger.prepare_contact_turnover().unwrap();
                ledger.finish_step(next);
            }
            let retired_identity = ledger.classify_body(raw);
            assert!(matches!(
                &retired_identity,
                IdentityClassification::ObservableRetired
            ));
            assert_eq!(ledger.resolve_observed_body(raw).unwrap(), retired);
            assert!(matches!(
                UnclaimedNative::<BodyNative>::new(raw, (), target_world, poisoned)
                    .claim(retired_identity),
                Err(Error::ObjectIdentityExhausted)
            ));
            assert!(!unsafe { ffi::b3Body_IsValid(raw) });
            assert_eq!(ledger.resolve_observed_body(raw).unwrap(), retired);

            let next = ledger.prepare_contact_turnover().unwrap();
            ledger.finish_step(next);
            if !rotate_to_visible {
                assert_eq!(ledger.resolve_observed_body(raw).unwrap(), retired);
                let next = ledger.prepare_contact_turnover().unwrap();
                ledger.finish_step(next);
            }
            assert!(matches!(
                ledger.resolve_observed_body(raw),
                Err(Error::StaleHandle {
                    kind: crate::error::HandleKind::Body
                })
            ));
            drop(_call);
            assert!(!world.state().poisoned.get());
        }

        check_retired_collision(false);
        check_retired_collision(true);

        let mut world = foundation().create_world(foundation().world_def()).unwrap();

        force_creation_failure(CreationStage::AfterClaim);
        force_next_compensation_mismatch();
        assert_eq!(
            world.create_body(foundation().body_def()).unwrap_err(),
            Error::NativeFailure
        );
        assert!(world.state().poisoned.get());
        assert_eq!(
            world.create_body(foundation().body_def()).unwrap_err(),
            Error::OwnerPoisoned
        );
        drop(world);
    }

    #[test]
    fn creation_transaction_compensation_mismatch_poisons_shape_and_joint_owners() {
        begin_shape_drop_trace();
        {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let body = static_body(&mut world);
            let before = native_counters(&world);
            force_creation_failure(CreationStage::AfterClaim);
            force_next_compensation_mismatch();

            assert_eq!(
                create_backed_shape(&mut world, body, BackingFamily::Mesh).unwrap_err(),
                Error::NativeFailure
            );
            assert!(world.state().poisoned.get());
            assert_eq!(native_counters(&world).shape_count, before.shape_count);
            assert_eq!(world.state().ledger.shape_resource_count(), 0);
            assert_eq!(world.state().backing_quarantine.len(), 1);
            assert_eq!(world.contains_body(body), Err(Error::OwnerPoisoned));
            assert_eq!(
                world.create_body(foundation().body_def()).unwrap_err(),
                Error::OwnerPoisoned
            );
        }
        assert_eq!(
            take_shape_drop_trace(),
            vec![ShapeDropEvent::NativeShape, ShapeDropEvent::MeshBacking,]
        );

        {
            let mut world = foundation().create_world(foundation().world_def()).unwrap();
            let body_a = dynamic_body(&mut world, -1.0);
            let body_b = dynamic_body(&mut world, 1.0);
            let before = native_counters(&world);
            force_creation_failure(CreationStage::AfterClaim);
            force_next_compensation_mismatch();

            assert_eq!(
                world
                    .create_distance_joint(DistanceJointDef::new(body_a, body_b).length(2.0))
                    .unwrap_err(),
                Error::NativeFailure
            );
            assert!(world.state().poisoned.get());
            assert_eq!(native_counters(&world).joint_count, before.joint_count);
            assert_eq!(
                world.create_body(foundation().body_def()).unwrap_err(),
                Error::OwnerPoisoned
            );
        }
    }
}
