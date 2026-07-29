use crate::TaskSystem;
use crate::body::{BodyDef, BodyType};
use crate::callbacks::WorldCallbacks;
use crate::core::provenance::allocate_owner_token;
use crate::core::{
    box3d_lock, callback_state, debug_checks, ffi_vec, task_system, units, validation, wasm,
};
use crate::debug_draw::DebugShapeRegistry;
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::debug_draw::{create_debug_shape, destroy_debug_shape};
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use crate::debug_draw::{
    register_provider_debug_registry, take_provider_debug_error, unregister_provider_debug_registry,
};
use crate::error::{Error, Result};
use crate::events::EventScratch;
use crate::recording::RecordingActivity;
use crate::shapes::{
    BoxHull, Capsule, Compound, HeightField, Hull, MeshData, ShapeDef, ShapeHeightField, ShapeHull,
    ShapeMaterialUsage, ShapeMesh, ShapeType, Sphere, SurfaceMaterial, validate_mesh_scale,
};
use crate::types::{
    Aabb, BodyId, Capacity, ContactData, ContactId, Counters, Filter, JointId, MassData, Matrix3,
    MotionLocks, Pos, Profile, Quat, ShapeId, Vec3, Version, WorldTransform,
};
use boxddd_sys::ffi;
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

impl Default for WorldDef {
    fn default() -> Self {
        let length_units = units::current_length_units();
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
}

impl WorldDef {
    /// Starts a builder with Box3D defaults.
    #[inline]
    pub fn builder() -> WorldDefBuilder {
        WorldDefBuilder::new()
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

    fn lower_locked(&self) -> ffi::b3WorldDef {
        let mut raw = unsafe { ffi::b3DefaultWorldDef() };
        raw.gravity = self.gravity.into_raw();
        raw.restitutionThreshold = self.restitution_threshold;
        raw.hitEventThreshold = self.hit_event_threshold;
        raw.contactHertz = self.contact_hertz;
        raw.contactDampingRatio = self.contact_damping_ratio;
        raw.contactSpeed = self.contact_speed;
        raw.maximumLinearSpeed = self.maximum_linear_speed;
        raw.enableSleep = self.enable_sleep;
        raw.enableContinuous = self.enable_continuous;
        raw.workerCount = self.worker_count;
        raw.capacity = self.capacity.into_raw();
        raw.frictionCallback = None;
        raw.restitutionCallback = None;
        raw.enqueueTask = None;
        raw.finishTask = None;
        raw.userTaskContext = std::ptr::null_mut();
        raw.userData = std::ptr::null_mut();
        raw.createDebugShape = None;
        raw.destroyDebugShape = None;
        raw.userDebugShapeContext = std::ptr::null_mut();
        raw
    }
}

/// Builder for `WorldDef`.
#[derive(Clone, Debug)]
pub struct WorldDefBuilder {
    def: WorldDef,
}

impl WorldDefBuilder {
    /// Creates a new value with default settings.
    #[inline]
    pub fn new() -> Self {
        Self {
            def: WorldDef::default(),
        }
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

impl Default for WorldDefBuilder {
    fn default() -> Self {
        Self::new()
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
    raw: ffi::b3WorldId,
    pub(crate) state: WorldState,
    _not_send_sync: PhantomData<Rc<()>>,
}

#[derive(Debug)]
pub(crate) struct WorldState {
    phase: WorldPhase,
    pub(crate) ledger: WorldLedger,
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
pub(crate) mod ledger;
mod runtime;
mod shape_api;

use ledger::WorldLedger;

impl World {
    /// Creates a new value with default settings.
    pub fn new(def: WorldDef) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        def.validate()?;
        def.validate_platform()?;
        let owner = allocate_owner_token()?;

        let ledger = WorldLedger::new(owner);
        let callback_index = ledger.callback_index();
        let callbacks = WorldCallbacks::new(callback_index.clone());
        let debug_shapes = Box::new(DebugShapeRegistry::new(callback_index));
        let task_system = def.task_system.clone();
        let task_context = task_system.as_ref().map(|task_system| {
            task_system::InstalledTaskContext::new(task_system, callbacks.invocation_slot())
        });
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        let provider_debug_shapes_token = if wasm::is_provider_mode() {
            register_provider_debug_registry(&debug_shapes).ok_or(Error::ProviderCallbackFailed)?
        } else {
            0
        };

        let _guard = box3d_lock::lock();
        let mut raw_def = def.lower_locked();
        if let Some(task_context) = task_context.as_deref() {
            task_system::install_callbacks(&mut raw_def, task_context);
        }
        if !wasm::is_provider_mode() {
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                raw_def.createDebugShape = Some(create_debug_shape);
                raw_def.destroyDebugShape = Some(destroy_debug_shape);
                raw_def.userDebugShapeContext =
                    (&*debug_shapes) as *const DebugShapeRegistry as *mut std::ffi::c_void;
            }
        }
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        if provider_debug_shapes_token != 0 {
            unsafe {
                ffi::boxddd_provider_debug_install_world_def(
                    &mut raw_def,
                    provider_debug_shapes_token,
                )
            };
        }

        let raw = unsafe { create_world_raw(&raw_def) };
        if unsafe { ffi::b3World_IsValid(raw) } {
            callbacks.install_raw_callbacks(raw);
            Ok(Self {
                raw,
                state: WorldState {
                    phase: WorldPhase::Live,
                    ledger,
                    callbacks,
                    event_scratch: EventScratch::default(),
                    _task_context: task_context,
                    active_recording: None,
                    debug_shapes,
                    #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
                    provider_debug_shapes_token,
                },
                _not_send_sync: PhantomData,
            })
        } else {
            drop(_guard);
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            if provider_debug_shapes_token != 0 {
                let _ = take_provider_debug_error(provider_debug_shapes_token);
                unregister_provider_debug_registry(provider_debug_shapes_token);
            }
            Err(Error::NativeFailure)
        }
    }

    #[inline]
    pub(crate) const fn raw(&self) -> ffi::b3WorldId {
        self.raw
    }

    /// Returns whether `body_id` currently belongs to this world.
    pub fn contains_body(&self, body_id: BodyId) -> bool {
        self.state.ledger.authorize_body(body_id).is_ok()
    }

    /// Returns whether `shape_id` currently belongs to this world.
    pub fn contains_shape(&self, shape_id: ShapeId) -> bool {
        self.state.ledger.authorize_shape(shape_id).is_ok()
    }

    /// Returns whether `joint_id` currently belongs to this world.
    pub fn contains_joint(&self, joint_id: JointId) -> bool {
        self.state.ledger.authorize_joint(joint_id).is_ok()
    }

    /// Returns whether `contact_id` is live in the current contact epoch.
    pub fn contains_contact(&self, contact_id: ContactId) -> bool {
        let Ok(raw) = self.state.ledger.authorize_contact(contact_id) else {
            return false;
        };
        if callback_state::check_not_in_callback().is_err() {
            return false;
        }
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked().is_ok() && unsafe { ffi::b3Contact_IsValid(raw) }
    }

    pub(crate) fn check_world_valid_locked(&self) -> Result<()> {
        if self.state.phase == WorldPhase::Live && unsafe { ffi::b3World_IsValid(self.raw) } {
            Ok(())
        } else {
            Err(Error::NativeFailure)
        }
    }

    #[inline]
    pub(crate) fn lock_body_checked(
        &self,
        body_id: BodyId,
    ) -> Result<std::sync::MutexGuard<'static, ()>> {
        callback_state::check_not_in_callback()?;
        let raw = self.state.ledger.authorize_body(body_id)?;
        let guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        debug_checks::check_body_valid_raw(raw)?;
        Ok(guard)
    }

    #[inline]
    pub(crate) fn lock_shape_checked(
        &self,
        shape_id: ShapeId,
    ) -> Result<std::sync::MutexGuard<'static, ()>> {
        callback_state::check_not_in_callback()?;
        let raw = self.state.ledger.authorize_shape(shape_id)?;
        let guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        debug_checks::check_shape_valid_raw(raw)?;
        Ok(guard)
    }

    #[inline]
    pub(crate) fn lock_joint_checked(
        &self,
        joint_id: JointId,
    ) -> Result<std::sync::MutexGuard<'static, ()>> {
        callback_state::check_not_in_callback()?;
        let raw = self.state.ledger.authorize_joint(joint_id)?;
        let guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        debug_checks::check_joint_valid_raw(raw)?;
        Ok(guard)
    }
}

#[cfg(not(feature = "double-precision"))]
#[inline]
unsafe fn create_world_raw(def: *const ffi::b3WorldDef) -> ffi::b3WorldId {
    unsafe { ffi::b3CreateWorld(def) }
}

#[cfg(feature = "double-precision")]
#[inline]
unsafe fn create_world_raw(def: *const ffi::b3WorldDef) -> ffi::b3WorldId {
    unsafe { ffi::b3CreateWorldDoublePrecision(def) }
}

impl Drop for World {
    fn drop(&mut self) {
        if self.state.phase != WorldPhase::Live {
            return;
        }
        self.state.phase = WorldPhase::Dropping;
        let _guard = box3d_lock::lock();
        self.stop_recording_locked();
        if unsafe { ffi::b3World_IsValid(self.raw) } {
            self.state.callbacks.clear_raw_callbacks(self.raw);
            unsafe { ffi::b3DestroyWorld(self.raw) };
            self.state.debug_shapes.clear_all();
        }
        self.state.ledger.finish_drop();
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        if self.state.provider_debug_shapes_token != 0 {
            let _ = take_provider_debug_error(self.state.provider_debug_shapes_token);
            unregister_provider_debug_registry(self.state.provider_debug_shapes_token);
            self.state.provider_debug_shapes_token = 0;
        }
        self.state.phase = WorldPhase::Destroyed;
    }
}

/// Returns the Box3D runtime version.
#[inline]
pub fn version() -> Version {
    Version::from_raw(unsafe { ffi::b3GetVersion() })
}

/// Returns the number of bytes currently allocated by Box3D.
#[inline]
pub fn allocated_byte_count() -> Result<i32> {
    callback_state::check_not_in_callback()?;
    let _guard = box3d_lock::lock();
    Ok(unsafe { ffi::b3GetByteCount() })
}

/// Returns whether the linked Box3D library uses double precision.
#[inline]
pub fn is_double_precision() -> bool {
    unsafe { ffi::b3IsDoublePrecision() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn static_body(world: &mut World) -> BodyId {
        world
            .create_body(
                BodyDef::builder()
                    .body_type(BodyType::Static)
                    .build()
                    .unwrap(),
            )
            .unwrap()
    }

    #[test]
    fn shape_resources_are_removed_on_shape_destroy_and_mesh_replace() {
        let mut world = World::new(WorldDef::default()).unwrap();
        let body = static_body(&mut world);
        let shape = world
            .create_mesh_shape(
                body,
                &ShapeDef::default(),
                MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true).unwrap(),
                [1.0, 1.0, 1.0],
            )
            .unwrap();
        assert_eq!(world.state.ledger.shape_resource_count(), 1);

        world
            .set_shape_mesh(
                shape,
                MeshData::box_mesh(Vec3::ZERO, [0.5, 0.5, 0.5], true).unwrap(),
                [1.0, 1.0, 1.0],
            )
            .unwrap();
        assert_eq!(world.state.ledger.shape_resource_count(), 1);

        world
            .set_shape_sphere(shape, &Sphere::new(Vec3::ZERO, 0.25))
            .unwrap();
        assert_eq!(world.state.ledger.shape_resource_count(), 0);

        world.destroy_shape(shape, true).unwrap();
        assert_eq!(world.state.ledger.shape_resource_count(), 0);
    }

    #[test]
    fn resource_backed_shapes_keep_body_static() {
        let mut world = World::new(WorldDef::default()).unwrap();
        let body = static_body(&mut world);
        world
            .create_height_field_shape(
                body,
                &ShapeDef::default(),
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
                &ShapeDef::default(),
                &Sphere::new(Vec3::new(2.0, 0.0, 0.0), 0.25),
            )
            .unwrap();
    }

    #[test]
    fn shape_resources_are_removed_on_body_destroy() {
        let mut world = World::new(WorldDef::default()).unwrap();
        let body = static_body(&mut world);
        world
            .create_mesh_shape(
                body,
                &ShapeDef::default(),
                MeshData::box_mesh(Vec3::ZERO, [1.0, 1.0, 1.0], true).unwrap(),
                [1.0, 1.0, 1.0],
            )
            .unwrap();
        world
            .create_height_field_shape(
                body,
                &ShapeDef::default(),
                HeightField::grid(2, 2, [1.0, 1.0, 1.0], false).unwrap(),
            )
            .unwrap();
        world
            .create_compound_shape(
                body,
                &ShapeDef::default(),
                Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.25), SurfaceMaterial::default())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(world.state.ledger.shape_resource_count(), 3);

        world.destroy_body(body).unwrap();
        assert_eq!(world.state.ledger.shape_resource_count(), 0);
    }
}
