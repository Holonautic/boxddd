#![cfg_attr(all(target_arch = "wasm32", boxddd_wasm_provider), allow(dead_code))]

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::core::callback_state::PendingCallback;
use crate::core::callback_state::{
    CallbackFailure, CallbackInvocationGuard, CallbackInvocationSlot, RegisteredCallback,
    SharedCallbackState,
};
use crate::core::{callback_state, material_mix_registry};
use crate::error::{Error, Result};
use crate::types::{Pos, ShapeId, Vec3};
use crate::world::World;
use crate::world::ledger::CallbackProvenanceIndex;
use boxddd_sys::ffi;
use std::ffi::c_void;
use std::fmt;
use std::sync::Arc;

type CustomFilterFn = dyn Fn(ShapeId, ShapeId) -> bool + Send + Sync + 'static;
type PreSolveFn = dyn Fn(ShapeId, ShapeId, Pos, Vec3) -> bool + Send + Sync + 'static;
type MaterialMixFn = dyn Fn(MaterialMixInput, MaterialMixInput) -> f32 + Send + Sync + 'static;

#[derive(Copy, Clone, Debug, PartialEq)]
/// Material data passed to a friction or restitution mixing callback.
pub struct MaterialMixInput {
    /// The coefficient currently supplied by Box3D for the material.
    pub coefficient: f32,
    /// User-defined material id stored on the source surface material.
    pub user_material_id: u64,
}
impl MaterialMixInput {
    /// Creates a material mixing callback input.
    #[inline]
    pub const fn new(coefficient: f32, user_material_id: u64) -> Self {
        Self {
            coefficient,
            user_material_id,
        }
    }
}

pub(crate) struct CustomFilterContext {
    callback: RegisteredCallback<CustomFilterFn>,
    provenance: CallbackProvenanceIndex,
    failures: CallbackInvocationSlot,
}

pub(crate) struct PreSolveContext {
    callback: RegisteredCallback<PreSolveFn>,
    provenance: CallbackProvenanceIndex,
    failures: CallbackInvocationSlot,
}

pub(crate) struct MaterialMixContext {
    pub(crate) callback: RegisteredCallback<MaterialMixFn>,
    pub(crate) failures: CallbackInvocationSlot,
}

pub(crate) struct WorldCallbacks {
    custom_filter: Box<CustomFilterContext>,
    pre_solve: Box<PreSolveContext>,
    friction: Box<MaterialMixContext>,
    restitution: Box<MaterialMixContext>,
    invocation: CallbackInvocationSlot,
    material_slot: Option<usize>,
}

pub(crate) struct RetiredWorldCallbacks {
    custom_filter: Option<Arc<CustomFilterFn>>,
    pre_solve: Option<Arc<PreSolveFn>>,
    friction: Option<Arc<MaterialMixFn>>,
    restitution: Option<Arc<MaterialMixFn>>,
}

impl RetiredWorldCallbacks {
    pub(crate) fn release_contained(self) -> Result<()> {
        let Self {
            custom_filter,
            pre_solve,
            friction,
            restitution,
        } = self;
        let mut panicked = false;
        for result in [
            callback_state::release_callback_capture(custom_filter),
            callback_state::release_callback_capture(pre_solve),
            callback_state::release_callback_capture(friction),
            callback_state::release_callback_capture(restitution),
        ] {
            panicked |= result.is_err();
        }
        if panicked {
            Err(Error::CallbackPanicked)
        } else {
            Ok(())
        }
    }
}

impl fmt::Debug for WorldCallbacks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorldCallbacks")
            .field(
                "custom_filter",
                &self.custom_filter.callback.snapshot().is_some(),
            )
            .field("pre_solve", &self.pre_solve.callback.snapshot().is_some())
            .field("friction", &self.friction.callback.snapshot().is_some())
            .field(
                "restitution",
                &self.restitution.callback.snapshot().is_some(),
            )
            .field("material_slot", &self.material_slot)
            .finish()
    }
}

impl WorldCallbacks {
    pub(crate) fn new(provenance: CallbackProvenanceIndex) -> Self {
        let invocation = CallbackInvocationSlot::default();
        Self {
            custom_filter: Box::new(CustomFilterContext {
                callback: RegisteredCallback::default(),
                provenance: provenance.clone(),
                failures: invocation.clone(),
            }),
            pre_solve: Box::new(PreSolveContext {
                callback: RegisteredCallback::default(),
                provenance,
                failures: invocation.clone(),
            }),
            friction: Box::new(MaterialMixContext {
                callback: RegisteredCallback::default(),
                failures: invocation.clone(),
            }),
            restitution: Box::new(MaterialMixContext {
                callback: RegisteredCallback::default(),
                failures: invocation.clone(),
            }),
            invocation,
            material_slot: None,
        }
    }

    pub(crate) fn install_raw_callbacks(&self, world: ffi::b3WorldId) {
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        unsafe {
            ffi::boxddd_provider_install_default_pre_solve(world);
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        unsafe {
            ffi::b3World_SetCustomFilterCallback(
                world,
                Some(custom_filter_trampoline),
                (&*self.custom_filter) as *const CustomFilterContext as *mut c_void,
            );
            ffi::b3World_SetPreSolveCallback(
                world,
                Some(pre_solve_trampoline),
                (&*self.pre_solve) as *const PreSolveContext as *mut c_void,
            );
        }
    }

    pub(crate) fn invocation_slot(&self) -> CallbackInvocationSlot {
        self.invocation.clone()
    }

    pub(crate) fn install_invocation(
        &self,
        state: SharedCallbackState,
    ) -> Result<CallbackInvocationGuard> {
        self.invocation.install(state)
    }

    pub(crate) fn clear_raw_callbacks(&mut self, world: ffi::b3WorldId) -> RetiredWorldCallbacks {
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = world;
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            unsafe {
                ffi::b3World_SetCustomFilterCallback(world, None, std::ptr::null_mut());
                ffi::b3World_SetPreSolveCallback(world, None, std::ptr::null_mut());
                ffi::b3World_SetFrictionCallback(world, None);
                ffi::b3World_SetRestitutionCallback(world, None);
            }
            if let Some(slot) = self.material_slot.take() {
                material_mix_registry::set_friction_ptr(slot, std::ptr::null_mut());
                material_mix_registry::set_restitution_ptr(slot, std::ptr::null_mut());
                material_mix_registry::release_slot(slot);
            }
        }
        self.retire_all()
    }

    pub(crate) fn retire_all(&mut self) -> RetiredWorldCallbacks {
        RetiredWorldCallbacks {
            custom_filter: self.custom_filter.callback.retire(),
            pre_solve: self.pre_solve.callback.retire(),
            friction: self.friction.callback.retire(),
            restitution: self.restitution.callback.retire(),
        }
    }

    fn ensure_material_slot(&mut self) -> Result<usize> {
        if let Some(slot) = self.material_slot {
            return Ok(slot);
        }
        let slot = material_mix_registry::acquire_slot().ok_or(Error::CallbackSlotsExhausted)?;
        self.material_slot = Some(slot);
        Ok(slot)
    }

    fn maybe_release_material_slot(&mut self) {
        let Some(slot) = self.material_slot else {
            return;
        };
        if !material_mix_registry::has_any_callback(slot) {
            material_mix_registry::release_slot(slot);
            self.material_slot = None;
        }
    }
}

unsafe extern "C" fn custom_filter_trampoline(
    shape_id_a: ffi::b3ShapeId,
    shape_id_b: ffi::b3ShapeId,
    context: *mut c_void,
) -> bool {
    if context.is_null() {
        return true;
    }
    let ctx = unsafe { &*(context as *const CustomFilterContext) };
    ctx.failures.invoke_while_clear(false, || {
        let Some(callback) = ctx.callback.snapshot() else {
            return true;
        };

        let Some(shape_id_a) = ctx.provenance.resolve_shape(shape_id_a) else {
            ctx.failures.record(CallbackFailure::InvalidHandle);
            return false;
        };
        let Some(shape_id_b) = ctx.provenance.resolve_shape(shape_id_b) else {
            ctx.failures.record(CallbackFailure::InvalidHandle);
            return false;
        };
        if !ctx.callback.is_current(&callback) {
            ctx.failures.record(CallbackFailure::InvalidNativeInput);
            return false;
        }
        callback(shape_id_a, shape_id_b)
    })
}

unsafe extern "C" fn pre_solve_trampoline(
    shape_id_a: ffi::b3ShapeId,
    shape_id_b: ffi::b3ShapeId,
    point: ffi::b3Pos,
    normal: ffi::b3Vec3,
    context: *mut c_void,
) -> bool {
    if context.is_null() {
        return true;
    }
    let ctx = unsafe { &*(context as *const PreSolveContext) };
    ctx.failures.invoke_while_clear(false, || {
        let Some(callback) = ctx.callback.snapshot() else {
            return true;
        };

        let Some(shape_id_a) = ctx.provenance.resolve_shape(shape_id_a) else {
            ctx.failures.record(CallbackFailure::InvalidHandle);
            return false;
        };
        let Some(shape_id_b) = ctx.provenance.resolve_shape(shape_id_b) else {
            ctx.failures.record(CallbackFailure::InvalidHandle);
            return false;
        };
        if !ctx.callback.is_current(&callback) {
            ctx.failures.record(CallbackFailure::InvalidNativeInput);
            return false;
        }
        callback(
            shape_id_a,
            shape_id_b,
            Pos::from_raw(point),
            Vec3::from_raw(normal),
        )
    })
}

impl World {
    /// Registers a custom contact filter callback.
    ///
    /// Box3D calls this when an awake dynamic contact pair is considered and at
    /// least one shape has custom filtering enabled. Return `true` to allow the
    /// collision, or `false` to disable it.
    ///
    /// The callback can run on Box3D worker threads, must be thread-safe, and
    /// must not mutate the world. The runtime callback guard rejects reentrant
    /// safe API calls as [`Error::InCallback`]. A callback panic is contained at
    /// the FFI boundary and reported by [`World::step`] as
    /// [`Error::CallbackPanicked`].
    ///
    /// On Emscripten provider builds, custom Rust callbacks return
    /// [`Error::UnsupportedOnWasm`].
    pub fn set_custom_filter<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn(ShapeId, ShapeId) -> bool + Send + Sync + 'static,
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = callback;
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let callback: Arc<CustomFilterFn> = Arc::new(callback);
            let pending = PendingCallback::new(callback)?;
            let previous = {
                let _call = self.enter_world_call()?;
                self.state()
                    .callbacks
                    .custom_filter
                    .callback
                    .publish(pending)
            };
            callback_state::release_callback_capture(previous)
        }
    }

    /// Clears the custom contact filter callback.
    ///
    /// Returns [`Error::InCallback`] if called from inside another Box3D callback.
    pub fn clear_custom_filter(&mut self) -> Result<()> {
        let previous = {
            let _call = self.enter_world_call()?;
            self.state().callbacks.custom_filter.callback.retire()
        };
        callback_state::release_callback_capture(previous)
    }

    /// Registers a pre-solve callback.
    ///
    /// Box3D calls this after contact update and before solving when a dynamic
    /// non-sensor shape has pre-solve events enabled. Return `true` to keep the
    /// contact enabled for the current step, or `false` to disable it for that
    /// step. The point and normal are the limited CCD-compatible contact data
    /// Box3D exposes here, not a full manifold.
    ///
    /// The callback can run on Box3D worker threads, must be thread-safe, and
    /// must not mutate the world. The runtime callback guard rejects reentrant
    /// safe API calls as [`Error::InCallback`]. A callback panic is contained at
    /// the FFI boundary and reported by [`World::step`] as
    /// [`Error::CallbackPanicked`].
    ///
    /// On Emscripten provider builds, custom Rust callbacks return
    /// [`Error::UnsupportedOnWasm`].
    pub fn set_pre_solve<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn(ShapeId, ShapeId, Pos, Vec3) -> bool + Send + Sync + 'static,
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = callback;
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let callback: Arc<PreSolveFn> = Arc::new(callback);
            let pending = PendingCallback::new(callback)?;
            let previous = {
                let _call = self.enter_world_call()?;
                self.state().callbacks.pre_solve.callback.publish(pending)
            };
            callback_state::release_callback_capture(previous)
        }
    }

    /// Clears the pre-solve callback.
    ///
    /// Returns [`Error::InCallback`] if called from inside another Box3D callback.
    pub fn clear_pre_solve(&mut self) -> Result<()> {
        let previous = {
            let _call = self.enter_world_call()?;
            self.state().callbacks.pre_solve.callback.retire()
        };
        callback_state::release_callback_capture(previous)
    }

    /// Registers a friction mixing callback.
    ///
    /// Box3D calls this from worker threads while mixing two shape materials.
    /// The default behavior is `sqrt(friction_a * friction_b)`. The callback
    /// receives only material inputs and must not mutate Box3D or application
    /// state. A panic is contained at the FFI boundary and reported by
    /// [`World::step`] as [`Error::CallbackPanicked`]. A non-finite result falls
    /// back to Box3D's default mix.
    ///
    /// On Emscripten provider builds, custom Rust callbacks return
    /// [`Error::UnsupportedOnWasm`].
    pub fn set_friction_callback<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn(MaterialMixInput, MaterialMixInput) -> f32 + Send + Sync + 'static,
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = callback;
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let callback: Arc<MaterialMixFn> = Arc::new(callback);
            let pending = PendingCallback::new(callback)?;
            let previous = {
                let _call = self.enter_world_call()?;
                let raw = self.raw();
                let callbacks = &mut self.state_mut().callbacks;
                let slot = callbacks.ensure_material_slot()?;
                let ptr =
                    (&*callbacks.friction) as *const MaterialMixContext as *mut MaterialMixContext;
                material_mix_registry::set_friction_ptr(slot, ptr);
                unsafe {
                    ffi::b3World_SetFrictionCallback(
                        raw,
                        material_mix_registry::friction_callback(slot),
                    )
                };
                callbacks.friction.callback.publish(pending)
            };
            callback_state::release_callback_capture(previous)
        }
    }

    /// Clears the friction mixing callback.
    ///
    /// Returns [`Error::InCallback`] if called from inside another Box3D callback.
    pub fn clear_friction_callback(&mut self) -> Result<()> {
        let previous = {
            let _call = self.enter_world_call()?;
            let raw = self.raw();
            let callbacks = &mut self.state_mut().callbacks;
            unsafe { ffi::b3World_SetFrictionCallback(raw, None) };
            if let Some(slot) = callbacks.material_slot {
                material_mix_registry::set_friction_ptr(slot, std::ptr::null_mut());
            }
            callbacks.maybe_release_material_slot();
            callbacks.friction.callback.retire()
        };
        callback_state::release_callback_capture(previous)
    }

    /// Registers a restitution mixing callback.
    ///
    /// Box3D calls this from worker threads while mixing two shape materials.
    /// The default behavior is `max(restitution_a, restitution_b)`. The callback
    /// receives only material inputs and must not mutate Box3D or application
    /// state. A panic is contained at the FFI boundary and reported by
    /// [`World::step`] as [`Error::CallbackPanicked`]. A non-finite result falls
    /// back to Box3D's default mix.
    ///
    /// On Emscripten provider builds, custom Rust callbacks return
    /// [`Error::UnsupportedOnWasm`].
    pub fn set_restitution_callback<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn(MaterialMixInput, MaterialMixInput) -> f32 + Send + Sync + 'static,
    {
        callback_state::check_not_in_callback()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = callback;
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let callback: Arc<MaterialMixFn> = Arc::new(callback);
            let pending = PendingCallback::new(callback)?;
            let previous = {
                let _call = self.enter_world_call()?;
                let raw = self.raw();
                let callbacks = &mut self.state_mut().callbacks;
                let slot = callbacks.ensure_material_slot()?;
                let ptr = (&*callbacks.restitution) as *const MaterialMixContext
                    as *mut MaterialMixContext;
                material_mix_registry::set_restitution_ptr(slot, ptr);
                unsafe {
                    ffi::b3World_SetRestitutionCallback(
                        raw,
                        material_mix_registry::restitution_callback(slot),
                    )
                };
                callbacks.restitution.callback.publish(pending)
            };
            callback_state::release_callback_capture(previous)
        }
    }

    /// Clears the restitution mixing callback.
    ///
    /// Returns [`Error::InCallback`] if called from inside another Box3D callback.
    pub fn clear_restitution_callback(&mut self) -> Result<()> {
        let previous = {
            let _call = self.enter_world_call()?;
            let raw = self.raw();
            let callbacks = &mut self.state_mut().callbacks;
            unsafe { ffi::b3World_SetRestitutionCallback(raw, None) };
            if let Some(slot) = callbacks.material_slot {
                material_mix_registry::set_restitution_ptr(slot, std::ptr::null_mut());
            }
            callbacks.maybe_release_material_slot();
            callbacks.restitution.callback.retire()
        };
        callback_state::release_callback_capture(previous)
    }
}
