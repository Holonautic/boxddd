use super::{DebugDrawCommand, DebugShapeHandle, DebugShapeRegistry, HexColor};
use crate::core::callback_state;
use crate::error::Error;
use crate::types::{Aabb, Pos, Vec3, WorldTransform};
use boxddd_sys::ffi;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
thread_local! {
    static PROVIDER_DEBUG: RefCell<ProviderDebugRegistry> = RefCell::new(ProviderDebugRegistry::default());
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[derive(Default)]
struct ProviderDebugRegistry {
    registries: HashMap<u32, ProviderDebugWorld>,
    shapes: HashMap<u32, ProviderDebugShape>,
    next_token: u32,
    next_shape: u32,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
struct ProviderDebugWorld {
    registry: *const DebugShapeRegistry,
    active_commands: Option<*mut Vec<DebugDrawCommand>>,
    first_error: Option<Error>,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[derive(Copy, Clone)]
struct ProviderDebugShape {
    token: u32,
    handle: DebugShapeHandle,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
impl ProviderDebugRegistry {
    fn allocate_token_id(&mut self) -> Option<u32> {
        self.next_token = self.next_token.checked_add(1)?;
        Some(self.next_token)
    }

    fn allocate_shape_id(&mut self) -> Option<u32> {
        self.next_shape = self.next_shape.checked_add(1)?;
        Some(self.next_shape)
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
pub(crate) struct ProviderDebugFrameGuard {
    token: u32,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
impl ProviderDebugFrameGuard {
    pub(crate) fn new(token: u32, commands: &mut Vec<DebugDrawCommand>) -> Self {
        set_provider_debug_frame(token, commands);
        Self { token }
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
impl Drop for ProviderDebugFrameGuard {
    fn drop(&mut self) {
        clear_provider_debug_frame(self.token);
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
pub(crate) fn register_provider_debug_registry(registry: &DebugShapeRegistry) -> Option<u32> {
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        let token = state.allocate_token_id()?;
        state.registries.insert(
            token,
            ProviderDebugWorld {
                registry: registry as *const DebugShapeRegistry,
                active_commands: None,
                first_error: None,
            },
        );
        Some(token)
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
pub(crate) fn unregister_provider_debug_registry(token: u32) {
    if token == 0 {
        return;
    }
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        state.registries.remove(&token);
        state.shapes.retain(|_, shape| shape.token != token);
    });
}

#[cfg(all(test, target_arch = "wasm32", boxddd_wasm_provider))]
pub(crate) fn provider_debug_registry_count() -> usize {
    PROVIDER_DEBUG.with(|state| state.borrow().registries.len())
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn set_provider_debug_frame(token: u32, commands: &mut Vec<DebugDrawCommand>) {
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(world) = state.registries.get_mut(&token) {
            world.active_commands = Some(commands as *mut Vec<DebugDrawCommand>);
            world.first_error = None;
        }
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn clear_provider_debug_frame(token: u32) {
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(world) = state.registries.get_mut(&token) {
            world.active_commands = None;
        }
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
pub(crate) fn take_provider_debug_error(token: u32) -> Option<Error> {
    if token == 0 {
        return None;
    }
    let provider_error = unsafe { ffi::boxddd_provider_debug_take_error(token) };
    if provider_error != 0 {
        boxddd_debug_report_error(token, provider_error as u32);
    }
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        state
            .registries
            .get_mut(&token)
            .and_then(|world| world.first_error.take())
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_registry(token: u32) -> Option<*const DebugShapeRegistry> {
    if token == 0 {
        return None;
    }
    PROVIDER_DEBUG.with(|state| {
        let state = state.borrow();
        state.registries.get(&token).map(|world| world.registry)
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_record_diagnostic(token: u32, message: impl Into<String>) {
    if let Some(registry) = provider_registry(token) {
        unsafe { &*registry }.push_diagnostic(message);
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_fail(token: u32, message: impl Into<String>) {
    provider_record_failure(
        token,
        Error::ProviderCallbackFailed,
        callback_state::CallbackFailure::Provider,
        message,
    );
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_record_failure(
    token: u32,
    error: Error,
    failure: callback_state::CallbackFailure,
    message: impl Into<String>,
) {
    let registry = if token == 0 {
        None
    } else {
        PROVIDER_DEBUG.with(|state| {
            let mut state = state.borrow_mut();
            let world = state.registries.get_mut(&token)?;
            if world.first_error.is_some() {
                None
            } else {
                world.first_error = Some(error);
                Some(world.registry)
            }
        })
    };
    if let Some(registry) = registry {
        unsafe { &*registry }.record_callback_failure(failure);
    }
    provider_record_diagnostic(token, message);
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_alloc_shape(token: u32, handle: DebugShapeHandle) -> Option<u32> {
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        let raw_shape = state.allocate_shape_id()?;
        state
            .shapes
            .insert(raw_shape, ProviderDebugShape { token, handle });
        Some(raw_shape)
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_can_alloc_shape() -> bool {
    PROVIDER_DEBUG.with(|state| state.borrow().next_shape < u32::MAX)
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_take_shape(token: u32, raw_shape: u32) -> Option<DebugShapeHandle> {
    if raw_shape == 0 {
        return None;
    }
    PROVIDER_DEBUG.with(|state| {
        let mut state = state.borrow_mut();
        let shape = state.shapes.get(&raw_shape).copied()?;
        if shape.token != token {
            return None;
        }
        state.shapes.remove(&raw_shape);
        Some(shape.handle)
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_shape_handle(token: u32, raw_shape: u32) -> Option<DebugShapeHandle> {
    if raw_shape == 0 {
        return None;
    }
    PROVIDER_DEBUG.with(|state| {
        let state = state.borrow();
        state
            .shapes
            .get(&raw_shape)
            .copied()
            .filter(|shape| shape.token == token)
            .map(|shape| shape.handle)
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_push_command(token: u32, command: DebugDrawCommand) -> bool {
    if token == 0 {
        return false;
    }
    let mut missing_frame = false;
    let pushed = PROVIDER_DEBUG.with(|state| {
        let state = state.borrow();
        let Some(world) = state.registries.get(&token) else {
            missing_frame = true;
            return false;
        };
        let Some(commands) = world.active_commands else {
            missing_frame = true;
            return false;
        };
        unsafe { (*commands).push(command) };
        true
    });
    if !pushed && missing_frame {
        provider_fail(
            token,
            "debug draw provider callback arrived without an active frame",
        );
    }
    pushed
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
unsafe fn provider_read<T: Copy>(ptr: *const T) -> Option<T> {
    unsafe { ptr.as_ref().copied() }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn run_provider_debug_callback<R: Copy>(
    token: u32,
    fallback: R,
    callback: impl FnOnce() -> R,
) -> R {
    match catch_unwind(AssertUnwindSafe(|| {
        let _guard = callback_state::CallbackGuard::enter();
        callback()
    })) {
        Ok(value) => value,
        Err(_) => {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let _guard = callback_state::CallbackGuard::enter();
                provider_record_failure(
                    token,
                    Error::CallbackPanicked,
                    callback_state::CallbackFailure::Panicked,
                    "debug draw provider callback panicked",
                );
            }));
            fallback
        }
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
pub extern "C" fn boxddd_debug_report_error(token: u32, code: u32) {
    run_provider_debug_callback(token, (), || {
        let message = match code {
            1 => "debug draw provider exports were not registered or were incomplete",
            2 => "debug draw provider dispatcher threw an exception",
            _ => "debug draw provider dispatcher failed",
        };
        provider_fail(token, message);
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `debug_shape` must point to a readable, properly aligned `b3DebugShape` for the
/// duration of this call.
pub unsafe extern "C" fn boxddd_debug_shape_create(
    token: u32,
    debug_shape: *const ffi::b3DebugShape,
) -> u32 {
    run_provider_debug_callback(token, 0, || {
        let Some(registry) = provider_registry(token) else {
            return 0;
        };
        let Some(raw) = (unsafe { debug_shape.as_ref() }) else {
            provider_fail(token, "debug shape create callback received a null shape");
            return 0;
        };
        if !provider_can_alloc_shape() {
            provider_fail(token, "debug draw provider shape handle table is full");
            return 0;
        }
        let Some(handle) = (unsafe { &*registry }).create_asset_handle(raw) else {
            provider_fail(token, "debug shape creation bookkeeping failed");
            return 0;
        };
        match provider_alloc_shape(token, handle) {
            Some(raw_shape) => raw_shape,
            None => {
                provider_fail(token, "debug draw provider shape handle table is full");
                0
            }
        }
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
pub extern "C" fn boxddd_debug_shape_destroy(token: u32, raw_shape: u32) {
    run_provider_debug_callback(token, (), || {
        let Some(handle) = provider_take_shape(token, raw_shape) else {
            provider_fail(
                token,
                "debug shape destroy referenced an unknown provider handle",
            );
            return;
        };
        if let Some(registry) = provider_registry(token) {
            if !unsafe { &*registry }.destroy_handle(handle) {
                provider_fail(token, "debug shape destruction bookkeeping failed");
            }
        } else {
            provider_fail(token, "debug shape destroy referenced an unknown world");
        }
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `transform` must point to a readable, properly aligned `b3WorldTransform` for the
/// duration of this call.
pub unsafe extern "C" fn boxddd_debug_draw_shape(
    token: u32,
    raw_shape: u32,
    transform: *const ffi::b3WorldTransform,
    color: u32,
) -> i32 {
    run_provider_debug_callback(token, 0, || {
        let Some(transform) = (unsafe { provider_read(transform) }) else {
            provider_fail(token, "debug draw shape callback received a null transform");
            return 0;
        };
        let handle = provider_shape_handle(token, raw_shape);
        provider_push_command(
            token,
            DebugDrawCommand::Shape {
                handle,
                transform: WorldTransform::from_raw(transform),
                color: HexColor::from_raw(color),
            },
        ) as i32
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// Every non-null endpoint must point to a readable, properly aligned `b3Pos` for the duration of
/// this call.
pub unsafe extern "C" fn boxddd_debug_draw_segment(
    token: u32,
    p1: *const ffi::b3Pos,
    p2: *const ffi::b3Pos,
    color: u32,
) {
    run_provider_debug_callback(token, (), || {
        let (Some(p1), Some(p2)) = (unsafe { provider_read(p1) }, unsafe { provider_read(p2) })
        else {
            provider_fail(
                token,
                "debug draw segment callback received a null endpoint",
            );
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Segment {
                p1: Pos::from_raw(p1),
                p2: Pos::from_raw(p2),
                color: HexColor::from_raw(color),
            },
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `transform` must point to a readable, properly aligned `b3WorldTransform` for the
/// duration of this call.
pub unsafe extern "C" fn boxddd_debug_draw_transform(
    token: u32,
    transform: *const ffi::b3WorldTransform,
) {
    run_provider_debug_callback(token, (), || {
        let Some(transform) = (unsafe { provider_read(transform) }) else {
            provider_fail(
                token,
                "debug draw transform callback received a null transform",
            );
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Transform(WorldTransform::from_raw(transform)),
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `position` must point to a readable, properly aligned `b3Pos` for the duration of
/// this call.
pub unsafe extern "C" fn boxddd_debug_draw_point(
    token: u32,
    position: *const ffi::b3Pos,
    size: f32,
    color: u32,
) {
    run_provider_debug_callback(token, (), || {
        let Some(position) = (unsafe { provider_read(position) }) else {
            provider_fail(token, "debug draw point callback received a null position");
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Point {
                position: Pos::from_raw(position),
                size,
                color: HexColor::from_raw(color),
            },
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `center` must point to a readable, properly aligned `b3Pos` for the duration of this
/// call.
pub unsafe extern "C" fn boxddd_debug_draw_sphere(
    token: u32,
    center: *const ffi::b3Pos,
    radius: f32,
    color: u32,
    alpha: f32,
) {
    run_provider_debug_callback(token, (), || {
        let Some(center) = (unsafe { provider_read(center) }) else {
            provider_fail(token, "debug draw sphere callback received a null center");
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Sphere {
                center: Pos::from_raw(center),
                radius,
                color: HexColor::from_raw(color),
                alpha,
            },
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// Every non-null endpoint must point to a readable, properly aligned `b3Pos` for the duration of
/// this call.
pub unsafe extern "C" fn boxddd_debug_draw_capsule(
    token: u32,
    p1: *const ffi::b3Pos,
    p2: *const ffi::b3Pos,
    radius: f32,
    color: u32,
    alpha: f32,
) {
    run_provider_debug_callback(token, (), || {
        let (Some(p1), Some(p2)) = (unsafe { provider_read(p1) }, unsafe { provider_read(p2) })
        else {
            provider_fail(
                token,
                "debug draw capsule callback received a null endpoint",
            );
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Capsule {
                p1: Pos::from_raw(p1),
                p2: Pos::from_raw(p2),
                radius,
                color: HexColor::from_raw(color),
                alpha,
            },
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `aabb` must point to a readable, properly aligned `b3AABB` for the duration of this
/// call.
pub unsafe extern "C" fn boxddd_debug_draw_bounds(
    token: u32,
    aabb: *const ffi::b3AABB,
    color: u32,
) {
    run_provider_debug_callback(token, (), || {
        let Some(aabb) = (unsafe { provider_read(aabb) }) else {
            provider_fail(token, "debug draw bounds callback received a null AABB");
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Bounds {
                aabb: Aabb::from_raw(aabb),
                color: HexColor::from_raw(color),
            },
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// Every non-null pointer argument must point to a readable, properly aligned value of its
/// declared type for the duration of this call.
pub unsafe extern "C" fn boxddd_debug_draw_box(
    token: u32,
    extents: *const ffi::b3Vec3,
    transform: *const ffi::b3WorldTransform,
    color: u32,
) {
    run_provider_debug_callback(token, (), || {
        let (Some(extents), Some(transform)) = (unsafe { provider_read(extents) }, unsafe {
            provider_read(transform)
        }) else {
            provider_fail(token, "debug draw box callback received a null argument");
            return;
        };
        provider_push_command(
            token,
            DebugDrawCommand::Box {
                extents: Vec3::from_raw(extents),
                transform: WorldTransform::from_raw(transform),
                color: HexColor::from_raw(color),
            },
        );
    });
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `position` must point to a readable, properly aligned `b3Pos`. A non-null `text`
/// must point to a valid NUL-terminated C string. Both pointees must remain readable for the
/// duration of this call.
pub unsafe extern "C" fn boxddd_debug_draw_string(
    token: u32,
    position: *const ffi::b3Pos,
    text: *const std::ffi::c_char,
    color: u32,
) {
    run_provider_debug_callback(token, (), || {
        let Some(position) = (unsafe { provider_read(position) }) else {
            provider_fail(token, "debug draw string callback received a null position");
            return;
        };
        if text.is_null() {
            provider_fail(
                token,
                "debug draw string callback received a null text pointer",
            );
            return;
        }
        let text = unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned();
        provider_push_command(
            token,
            DebugDrawCommand::String {
                position: Pos::from_raw(position),
                text,
                color: HexColor::from_raw(color),
            },
        );
    });
}
