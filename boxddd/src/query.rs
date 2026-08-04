#![cfg_attr(all(target_arch = "wasm32", boxddd_wasm_provider), allow(dead_code))]

use crate::collision::{ShapeCastInput, ShapeProxy};
use crate::core::callback_state;
use crate::core::callback_state::LocalCallbackState;
use crate::error::{Error, Result};
use crate::shapes::Capsule;
use crate::types::{Aabb, Plane, Pos, ShapeId, Vec3};
use crate::world::World;
use boxddd_sys::ffi;
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use std::cell::RefCell;
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use std::collections::HashMap;
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use std::ffi::c_void;
#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Broad-phase traversal statistics returned by Box3D queries.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TreeStats {
    /// Number of broad-phase tree nodes visited.
    pub node_visits: i32,
    /// Number of broad-phase tree leaves visited.
    pub leaf_visits: i32,
}

impl TreeStats {
    /// Converts raw Box3D data into the safe value type.
    #[inline]
    pub const fn from_raw(raw: ffi::b3TreeStats) -> Self {
        Self {
            node_visits: raw.nodeVisits,
            leaf_visits: raw.leafVisits,
        }
    }
}

/// Collision filter applied to world queries and casts.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct QueryFilter {
    /// Category bits assigned to the query.
    pub category_bits: u64,
    /// Mask bits used to accept candidate shapes.
    pub mask_bits: u64,
    /// User-defined query identifier recorded by Box3D replay.
    pub id: u64,
}

impl QueryFilter {
    /// Creates a new value with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the category bits assigned to the query.
    pub fn category_bits(mut self, category_bits: u64) -> Self {
        self.category_bits = category_bits;
        self
    }

    /// Sets the mask bits used to accept candidate shapes.
    pub fn mask_bits(mut self, mask_bits: u64) -> Self {
        self.mask_bits = mask_bits;
        self
    }

    /// Sets the user-defined query id used by recording and replay diagnostics.
    pub fn id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    #[inline]
    pub(crate) fn raw(self) -> ffi::b3QueryFilter {
        ffi::b3QueryFilter {
            categoryBits: self.category_bits,
            maskBits: self.mask_bits,
            id: self.id,
            name: std::ptr::null(),
        }
    }
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            category_bits: u64::MAX,
            mask_bits: u64::MAX,
            id: 0,
        }
    }
}

/// Shape reported by an overlap query.
///
/// The id is an owned, owner-scoped value rather than a borrow from Box3D. It may be retained after
/// the query, but becomes stale if the shape is later destroyed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct QueryHit {
    /// Shape that overlapped the query volume.
    pub shape_id: ShapeId,
}

/// Hit reported by a ray or shape cast callback.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RayHit {
    /// Shape hit by the cast.
    pub shape_id: ShapeId,
    /// World-space point of initial intersection.
    pub point: Pos,
    /// Surface normal at the intersection point.
    pub normal: Vec3,
    /// Fraction along the cast translation at the intersection point.
    pub fraction: f32,
    /// Shape or triangle material id reported by Box3D.
    pub user_material_id: u64,
    /// Triangle index for mesh or height-field hits, or `-1` otherwise.
    pub triangle_index: i32,
    /// Child shape index for compound hits, or `-1` otherwise.
    pub child_index: i32,
}

/// Closest hit reported by [`World::cast_ray_closest`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RayClosestHit {
    /// Shape hit by the ray.
    pub shape_id: ShapeId,
    /// World-space point of initial intersection.
    pub point: Pos,
    /// Surface normal at the intersection point.
    pub normal: Vec3,
    /// Fraction along the ray translation at the intersection point.
    pub fraction: f32,
    /// Shape or triangle material id reported by Box3D.
    pub user_material_id: u64,
    /// Triangle index for mesh or height-field hits, or `-1` otherwise.
    pub triangle_index: i32,
    /// Child shape index for compound hits, or `-1` otherwise.
    pub child_index: i32,
    /// Number of broad-phase tree nodes visited.
    pub node_visits: i32,
    /// Number of broad-phase tree leaves visited.
    pub leaf_visits: i32,
}

/// Closest point result for a body-scoped query.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BodyClosestPoint {
    /// Closest world-space point on the body.
    pub point: Vec3,
    /// Distance from the query point to the body.
    pub distance: f32,
}

/// Hit reported by a body-scoped cast.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BodyCastHit {
    /// Shape on the body hit by the cast.
    pub shape_id: ShapeId,
    /// World-space point of initial intersection.
    pub point: Pos,
    /// Surface normal at the intersection point.
    pub normal: Vec3,
    /// Fraction along the cast translation at the intersection point.
    pub fraction: f32,
    /// Shape or triangle material id reported by Box3D.
    pub user_material_id: u64,
    /// Triangle index for mesh or height-field hits, or `-1` otherwise.
    pub triangle_index: i32,
    /// Narrow-phase iteration count reported by Box3D.
    pub iterations: i32,
}

impl BodyCastHit {
    #[inline]
    pub(crate) fn from_raw(raw: ffi::b3BodyCastResult, shape_id: ShapeId) -> Option<Self> {
        raw.hit.then(|| Self {
            shape_id,
            point: Pos::from_raw(raw.point),
            normal: Vec3::from_raw(raw.normal),
            fraction: raw.fraction,
            user_material_id: raw.userMaterialId,
            triangle_index: raw.triangleIndex,
            iterations: raw.iterations,
        })
    }
}

/// Result returned by a ray cast against a single shape.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ShapeRayHit {
    /// World-space point of initial intersection.
    pub point: Pos,
    /// Surface normal at the intersection point.
    pub normal: Vec3,
    /// Fraction along the cast translation at the intersection point.
    pub fraction: f32,
    /// Triangle index for mesh or height-field hits, or `-1` otherwise.
    pub triangle_index: i32,
    /// Child shape index for compound hits, or `-1` otherwise.
    pub child_index: i32,
    /// Material slot index reported by the hit, when applicable.
    pub material_index: i32,
    /// Narrow-phase iteration count reported by Box3D.
    pub iterations: i32,
}

impl ShapeRayHit {
    #[inline]
    pub(crate) fn from_raw(raw: ffi::b3WorldCastOutput) -> Option<Self> {
        raw.hit.then(|| Self {
            point: Pos::from_raw(raw.point),
            normal: Vec3::from_raw(raw.normal),
            fraction: raw.fraction,
            triangle_index: raw.triangleIndex,
            child_index: raw.childIndex,
            material_index: raw.materialIndex,
            iterations: raw.iterations,
        })
    }
}

/// Contact plane gathered for a capsule character mover.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MoverPlane {
    /// Shape that produced the plane.
    pub shape_id: ShapeId,
    /// Plane that constrains the mover, expressed relative to the query origin.
    pub plane: Plane,
    /// Contact point expressed relative to the query origin.
    pub point: Vec3,
}

impl MoverPlane {
    #[inline]
    pub(crate) fn from_raw(shape_id: ShapeId, raw: ffi::b3PlaneResult) -> Self {
        Self {
            shape_id,
            plane: Plane::from_raw(raw.plane),
            point: Vec3::from_raw(raw.point),
        }
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
thread_local! {
    static PROVIDER_QUERY: RefCell<ProviderQueryRegistry> = RefCell::new(ProviderQueryRegistry::default());
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[derive(Default)]
struct ProviderQueryRegistry {
    calls: HashMap<u32, ProviderQueryCall>,
    next_token: u32,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[derive(Copy, Clone)]
struct ProviderQueryCall {
    kind: ProviderQueryKind,
    context: *mut c_void,
    state: *mut LocalCallbackState,
    overlap: Option<unsafe fn(*mut c_void, ffi::b3ShapeId) -> bool>,
    cast: Option<
        unsafe fn(*mut c_void, ffi::b3ShapeId, ffi::b3Pos, ffi::b3Vec3, f32, u64, i32, i32) -> f32,
    >,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[derive(Copy, Clone, Eq, PartialEq)]
enum ProviderQueryKind {
    Overlap,
    Cast,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
impl ProviderQueryRegistry {
    fn allocate_token_id(&mut self) -> Option<u32> {
        self.next_token = self.next_token.checked_add(1)?;
        Some(self.next_token)
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
struct ProviderQueryGuard {
    token: u32,
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
impl ProviderQueryGuard {
    fn overlap<F>(context: &mut OverlapContext<'_, F>) -> Result<Self>
    where
        F: FnMut(ShapeId) -> bool,
    {
        register_provider_query(ProviderQueryCall {
            kind: ProviderQueryKind::Overlap,
            context: (context as *mut OverlapContext<'_, F>).cast::<c_void>(),
            state: &mut context.state,
            overlap: Some(provider_overlap_dispatch::<F>),
            cast: None,
        })
    }

    fn cast<F>(context: &mut CastContext<'_, F>) -> Result<Self>
    where
        F: FnMut(RayHit) -> f32,
    {
        register_provider_query(ProviderQueryCall {
            kind: ProviderQueryKind::Cast,
            context: (context as *mut CastContext<'_, F>).cast::<c_void>(),
            state: &mut context.state,
            overlap: None,
            cast: Some(provider_cast_dispatch::<F>),
        })
    }

    fn token(&self) -> u32 {
        self.token
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
impl Drop for ProviderQueryGuard {
    fn drop(&mut self) {
        PROVIDER_QUERY.with(|state| {
            state.borrow_mut().calls.remove(&self.token);
        });
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn register_provider_query(call: ProviderQueryCall) -> Result<ProviderQueryGuard> {
    PROVIDER_QUERY.with(|state| {
        let mut state = state.borrow_mut();
        let token = state
            .allocate_token_id()
            .ok_or(Error::CallbackSlotsExhausted)?;
        state.calls.insert(token, call);
        Ok(ProviderQueryGuard { token })
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_query_call(token: u32) -> Option<ProviderQueryCall> {
    PROVIDER_QUERY.with(|state| state.borrow().calls.get(&token).copied())
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn provider_query_fail(token: u32, error: Error) {
    if token == 0 {
        return;
    }
    let state =
        PROVIDER_QUERY.with(|registry| registry.borrow().calls.get(&token).map(|call| call.state));
    if let Some(state) = state {
        unsafe { &mut *state }.fail(error, ());
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn poll_provider_query_bridge_error(token: u32) {
    let provider_error = unsafe { ffi::boxddd_provider_query_take_error(token) };
    if provider_error != 0 {
        provider_query_fail(token, Error::ProviderCallbackFailed);
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
unsafe fn provider_read<T: Copy>(ptr: *const T) -> Option<T> {
    unsafe { ptr.as_ref().copied() }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
fn run_provider_query_callback<R: Copy>(
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
                provider_query_fail(token, Error::CallbackPanicked);
            }));
            fallback
        }
    }
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
unsafe fn provider_overlap_dispatch<F>(context: *mut c_void, shape_id: ffi::b3ShapeId) -> bool
where
    F: FnMut(ShapeId) -> bool,
{
    let context = unsafe { &mut *context.cast::<OverlapContext<'_, F>>() };
    if context.state.has_failed() {
        return false;
    }
    let shape_id = match context.world.state().ledger.resolve_shape(shape_id) {
        Ok(shape_id) => shape_id,
        Err(error) => return context.state.fail(error, false),
    };
    context.state.invoke(false, || (context.visitor)(shape_id))
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
unsafe fn provider_cast_dispatch<F>(
    context: *mut c_void,
    shape_id: ffi::b3ShapeId,
    point: ffi::b3Pos,
    normal: ffi::b3Vec3,
    fraction: f32,
    user_material_id: u64,
    triangle_index: i32,
    child_index: i32,
) -> f32
where
    F: FnMut(RayHit) -> f32,
{
    let context = unsafe { &mut *context.cast::<CastContext<'_, F>>() };
    if context.state.has_failed() {
        return 0.0;
    }
    let shape_id = match context.world.state().ledger.resolve_shape(shape_id) {
        Ok(shape_id) => shape_id,
        Err(error) => return context.state.fail(error, 0.0),
    };
    let hit = RayHit {
        shape_id,
        point: Pos::from_raw(point),
        normal: Vec3::from_raw(normal),
        fraction,
        user_material_id,
        triangle_index,
        child_index,
    };
    context.state.invoke(0.0, || (context.visitor)(hit))
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// A non-null `shape_id` must point to a readable, properly aligned `b3ShapeId` for the duration
/// of this call.
pub unsafe extern "C" fn boxddd_query_overlap(token: u32, shape_id: *const ffi::b3ShapeId) -> i32 {
    run_provider_query_callback(token, 0, || {
        let Some(shape_id) = (unsafe { provider_read(shape_id) }) else {
            provider_query_fail(token, Error::ProviderCallbackFailed);
            return 0;
        };
        let Some(call) = provider_query_call(token) else {
            provider_query_fail(token, Error::ProviderCallbackFailed);
            return 0;
        };
        let Some(dispatch) = call
            .overlap
            .filter(|_| call.kind == ProviderQueryKind::Overlap)
        else {
            provider_query_fail(token, Error::ProviderCallbackFailed);
            return 0;
        };
        i32::from(unsafe { dispatch(call.context, shape_id) })
    })
}

#[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
#[unsafe(no_mangle)]
#[doc(hidden)]
/// # Safety
///
/// Every non-null pointer argument must point to a readable, properly aligned value of its
/// declared type for the duration of this call.
pub unsafe extern "C" fn boxddd_query_cast(
    token: u32,
    shape_id: *const ffi::b3ShapeId,
    point: *const ffi::b3Pos,
    normal: *const ffi::b3Vec3,
    fraction: f32,
    user_material_id: *const u64,
    triangle_index: i32,
    child_index: i32,
) -> f32 {
    run_provider_query_callback(token, 0.0, || {
        let (Some(shape_id), Some(point), Some(normal), Some(user_material_id)) = (
            unsafe { provider_read(shape_id) },
            unsafe { provider_read(point) },
            unsafe { provider_read(normal) },
            unsafe { provider_read(user_material_id) },
        ) else {
            provider_query_fail(token, Error::ProviderCallbackFailed);
            return 0.0;
        };
        let Some(call) = provider_query_call(token) else {
            provider_query_fail(token, Error::ProviderCallbackFailed);
            return 0.0;
        };
        let Some(dispatch) = call.cast.filter(|_| call.kind == ProviderQueryKind::Cast) else {
            provider_query_fail(token, Error::ProviderCallbackFailed);
            return 0.0;
        };
        let next_fraction = unsafe {
            dispatch(
                call.context,
                shape_id,
                point,
                normal,
                fraction,
                user_material_id,
                triangle_index,
                child_index,
            )
        };
        if next_fraction.is_finite() {
            next_fraction
        } else {
            0.0
        }
    })
}

impl World {
    /// Collects every broad-phase candidate whose bounds overlap the world-space `aabb`.
    ///
    /// Results are copied into owned values in Box3D traversal order, which is unspecified. See
    /// [`Self::visit_overlap_aabb`] for traversal and platform semantics.
    pub fn overlap_aabb(&self, aabb: Aabb, filter: QueryFilter) -> Result<Vec<QueryHit>> {
        let mut out = Vec::new();
        self.overlap_aabb_into(aabb, filter, &mut out)?;
        Ok(out)
    }

    /// Writes broad-phase candidates overlapping the world-space `aabb` into `out`.
    ///
    /// The output is cleared before traversal and follows the same unspecified order and platform
    /// semantics as [`Self::visit_overlap_aabb`]. If traversal fails, `out` may contain the prefix
    /// collected before the failure.
    pub fn overlap_aabb_into(
        &self,
        aabb: Aabb,
        filter: QueryFilter,
        out: &mut Vec<QueryHit>,
    ) -> Result<TreeStats> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_overlap_aabb(aabb, filter, |shape_id| {
            out.push(QueryHit { shape_id });
            true
        })
    }

    /// Visits broad-phase candidates whose bounds overlap the world-space `aabb`.
    ///
    /// Returning `false` from `visitor` terminates traversal early. Box3D may
    /// visit shapes in any order. The visitor is called synchronously and is
    /// never retained. It runs inside a Box3D callback context, so safe APIs
    /// called reentrantly return [`Error::InCallback`]. On unwind-capable
    /// targets, a visitor panic stops traversal and is returned as
    /// [`Error::CallbackPanicked`]; `panic=abort` targets cannot contain it.
    ///
    /// Provider-mode WASM supports this AABB visitor through its query bridge.
    pub fn visit_overlap_aabb<F>(
        &self,
        aabb: Aabb,
        filter: QueryFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(ShapeId) -> bool,
    {
        callback_state::check_not_in_callback()?;
        let owner_call_frame = callback_state::OwnerCallFrame::enter();
        let result = (move || {
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            {
                let aabb = aabb.validate()?;
                let mut context = OverlapContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let provider_query = ProviderQueryGuard::overlap(&mut context)?;
                let _call = self.enter_world_call()?;
                let stats = unsafe {
                    ffi::boxddd_provider_world_overlap_aabb(
                        self.raw(),
                        aabb.into_raw(),
                        filter.raw(),
                        provider_query.token(),
                    )
                };
                poll_provider_query_bridge_error(provider_query.token());
                context.state.drain()?;
                Ok(TreeStats::from_raw(stats))
            }
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                aabb.validate()?;
                let mut ctx = OverlapContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let _call = self.enter_world_call()?;
                let stats = unsafe {
                    ffi::b3World_OverlapAABB(
                        self.raw(),
                        aabb.into_raw(),
                        filter.raw(),
                        Some(overlap_trampoline::<F>),
                        (&mut ctx as *mut OverlapContext<_>).cast(),
                    )
                };
                ctx.state.drain()?;
                Ok(TreeStats::from_raw(stats))
            }
        })();
        drop(owner_call_frame);
        result
    }

    /// Collects shapes overlapping `proxy` placed at the world-space `origin`.
    ///
    /// Proxy points are relative to `origin`, which preserves precision far from the world origin.
    /// Results are owned and have unspecified traversal order. See [`Self::visit_overlap_shape`]
    /// for callback and platform semantics.
    pub fn overlap_shape(
        &self,
        origin: impl Into<Pos>,
        proxy: &ShapeProxy,
        filter: QueryFilter,
    ) -> Result<Vec<QueryHit>> {
        let mut out = Vec::new();
        self.overlap_shape_into(origin, proxy, filter, &mut out)?;
        Ok(out)
    }

    /// Writes shapes overlapping `proxy` at the world-space `origin` into `out`.
    ///
    /// The output is cleared before traversal and may contain a partial result if traversal fails.
    /// Proxy points are relative to `origin`; see [`Self::visit_overlap_shape`] for traversal and
    /// platform semantics.
    pub fn overlap_shape_into(
        &self,
        origin: impl Into<Pos>,
        proxy: &ShapeProxy,
        filter: QueryFilter,
        out: &mut Vec<QueryHit>,
    ) -> Result<TreeStats> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_overlap_shape(origin, proxy, filter, |shape_id| {
            out.push(QueryHit { shape_id });
            true
        })
    }

    /// Visits shapes overlapping `proxy` placed at the world-space `origin`.
    ///
    /// Proxy points are relative to `origin`, which preserves precision far from the world origin.
    /// Returning `false` from `visitor` terminates traversal early. Box3D may
    /// visit shapes in any order. The visitor is synchronous and is never
    /// retained. Safe APIs called from it return [`Error::InCallback`]. On
    /// unwind-capable targets, a panic stops traversal and is returned as
    /// [`Error::CallbackPanicked`]; `panic=abort` targets cannot contain it.
    ///
    /// Provider-mode WASM does not bridge shape-overlap visitors and returns
    /// [`Error::UnsupportedOnWasm`] before invoking `visitor`.
    pub fn visit_overlap_shape<F>(
        &self,
        origin: impl Into<Pos>,
        proxy: &ShapeProxy,
        filter: QueryFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(ShapeId) -> bool,
    {
        callback_state::check_not_in_callback()?;
        let owner_call_frame = callback_state::OwnerCallFrame::enter();
        let result = (move || {
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            {
                let _ = (origin, proxy, filter, visitor);
                Err(Error::UnsupportedOnWasm)
            }
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                let origin = origin.into().validate()?;
                let raw_proxy = proxy.raw();
                let mut ctx = OverlapContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let _call = self.enter_world_call()?;
                let stats = unsafe {
                    ffi::b3World_OverlapShape(
                        self.raw(),
                        origin.into_raw(),
                        &raw_proxy,
                        filter.raw(),
                        Some(overlap_trampoline::<F>),
                        (&mut ctx as *mut OverlapContext<_>).cast(),
                    )
                };
                ctx.state.drain()?;
                Ok(TreeStats::from_raw(stats))
            }
        })();
        drop(owner_call_frame);
        result
    }

    /// Collects hits from a ray cast through the world.
    ///
    /// `origin` is a world position and `translation` is the displacement from the ray start to its
    /// end. Hit points are world positions. Results are owned and follow Box3D's unspecified
    /// traversal order. See [`Self::visit_cast_ray`] for callback and platform semantics.
    pub fn cast_ray(
        &self,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
    ) -> Result<Vec<RayHit>> {
        let mut out = Vec::new();
        self.cast_ray_into(origin, translation, filter, &mut out)?;
        Ok(out)
    }

    /// Writes ray-cast hits into `out`, clearing it first.
    ///
    /// Hit points are world positions. The output follows Box3D's unspecified traversal order and
    /// may contain a partial result if traversal fails. See [`Self::visit_cast_ray`] for callback
    /// and platform semantics.
    pub fn cast_ray_into(
        &self,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
        out: &mut Vec<RayHit>,
    ) -> Result<TreeStats> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_cast_ray(origin, translation, filter, |hit| {
            out.push(hit);
            1.0
        })
    }

    /// Visits hits from a ray cast through the world.
    ///
    /// `origin` is a world position, `translation` is the displacement to the ray end, and reported
    /// hit points are world positions.
    ///
    /// The callback follows Box3D ray-cast semantics: return `-1.0` to ignore
    /// the hit and continue, `0.0` to terminate, the hit fraction to clip the
    /// ray for closest-hit behavior, or `1.0` to continue without clipping.
    /// Non-finite returns are treated as termination.
    ///
    /// Box3D may visit shapes in any order. The visitor is synchronous and is
    /// never retained. Safe APIs called from it return [`Error::InCallback`].
    /// On unwind-capable targets, a panic terminates the cast and is returned
    /// as [`Error::CallbackPanicked`]; `panic=abort` targets cannot contain it.
    ///
    /// Provider-mode WASM supports this visitor through its ray-cast bridge.
    pub fn visit_cast_ray<F>(
        &self,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(RayHit) -> f32,
    {
        callback_state::check_not_in_callback()?;
        let owner_call_frame = callback_state::OwnerCallFrame::enter();
        let result = (move || {
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            {
                let origin = origin.into().validate()?;
                let translation = translation.into().validate()?;
                let mut context = CastContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let provider_query = ProviderQueryGuard::cast(&mut context)?;
                let _call = self.enter_world_call()?;
                let stats = unsafe {
                    ffi::boxddd_provider_world_cast_ray(
                        self.raw(),
                        origin.into_raw(),
                        translation.into_raw(),
                        filter.raw(),
                        provider_query.token(),
                    )
                };
                poll_provider_query_bridge_error(provider_query.token());
                context.state.drain()?;
                Ok(TreeStats::from_raw(stats))
            }
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                let origin = origin.into().validate()?;
                let translation = translation.into().validate()?;
                let mut ctx = CastContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let _call = self.enter_world_call()?;
                let stats = unsafe {
                    ffi::b3World_CastRay(
                        self.raw(),
                        origin.into_raw(),
                        translation.into_raw(),
                        filter.raw(),
                        Some(cast_trampoline::<F>),
                        (&mut ctx as *mut CastContext<_>).cast(),
                    )
                };
                ctx.state.drain()?;
                Ok(TreeStats::from_raw(stats))
            }
        })();
        drop(owner_call_frame);
        result
    }

    /// Returns the closest hit from a ray cast through the world.
    ///
    /// This is the Box3D convenience path for closest-hit queries. It does not provide a callback
    /// for custom per-hit filtering and ignores shapes initially overlapping the ray origin.
    /// `origin` and the returned hit point are world positions; `translation` is the displacement
    /// to the ray end. The returned value owns its data and is supported in provider-mode WASM.
    pub fn cast_ray_closest(
        &self,
        origin: impl Into<Pos>,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
    ) -> Result<Option<RayClosestHit>> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        let translation = translation.into().validate()?;
        let _call = self.enter_world_call()?;
        let raw = unsafe {
            ffi::b3World_CastRayClosest(
                self.raw(),
                origin.into_raw(),
                translation.into_raw(),
                filter.raw(),
            )
        };
        if raw.hit {
            Ok(Some(RayClosestHit {
                shape_id: self.state().ledger.resolve_shape(raw.shapeId)?,
                point: Pos::from_raw(raw.point),
                normal: Vec3::from_raw(raw.normal),
                fraction: raw.fraction,
                user_material_id: raw.userMaterialId,
                triangle_index: raw.triangleIndex,
                child_index: raw.childIndex,
                node_visits: raw.nodeVisits,
                leaf_visits: raw.leafVisits,
            }))
        } else {
            Ok(None)
        }
    }

    /// Collects hits from sweeping a shape proxy through the world.
    ///
    /// The input proxy is relative to the world-space `origin`; reported hit points are world
    /// positions. Results have unspecified traversal order. See [`Self::visit_cast_shape`] for
    /// callback and platform semantics.
    pub fn cast_shape(
        &self,
        origin: impl Into<Pos>,
        input: ShapeCastInput,
        filter: QueryFilter,
    ) -> Result<Vec<RayHit>> {
        let mut out = Vec::new();
        self.cast_shape_into(origin, input, filter, &mut out)?;
        Ok(out)
    }

    /// Writes shape-cast hits into `out`, clearing it first.
    ///
    /// The input proxy is relative to the world-space `origin`; reported hit points are world
    /// positions. The output may contain a partial result if traversal fails. See
    /// [`Self::visit_cast_shape`] for callback and platform semantics.
    pub fn cast_shape_into(
        &self,
        origin: impl Into<Pos>,
        input: ShapeCastInput,
        filter: QueryFilter,
        out: &mut Vec<RayHit>,
    ) -> Result<TreeStats> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_cast_shape(origin, input, filter, |hit| {
            out.push(hit);
            1.0
        })
    }

    /// Visits hits from sweeping a shape proxy through the world.
    ///
    /// The input proxy is relative to the world-space `origin`, while reported hit points are world
    /// positions. This keeps the cast precise far from the world origin.
    ///
    /// The callback uses the same control return values as [`Self::visit_cast_ray`].
    /// Box3D may visit shapes in any order. The visitor is synchronous and is never retained; safe
    /// API reentry returns [`Error::InCallback`]. On unwind-capable targets, a panic stops the cast
    /// and is returned as [`Error::CallbackPanicked`]. `panic=abort` targets cannot contain a
    /// panic.
    ///
    /// Provider-mode WASM does not bridge shape-cast visitors and returns
    /// [`Error::UnsupportedOnWasm`] before invoking `visitor`.
    pub fn visit_cast_shape<F>(
        &self,
        origin: impl Into<Pos>,
        input: ShapeCastInput,
        filter: QueryFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(RayHit) -> f32,
    {
        callback_state::check_not_in_callback()?;
        let owner_call_frame = callback_state::OwnerCallFrame::enter();
        let result = (move || {
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            {
                let _ = (origin, input, filter, visitor);
                Err(Error::UnsupportedOnWasm)
            }
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                let origin = origin.into().validate()?;
                let input = input.validate()?;
                let raw_input = input.raw();
                let mut ctx = CastContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let _call = self.enter_world_call()?;
                let stats = unsafe {
                    ffi::b3World_CastShape(
                        self.raw(),
                        origin.into_raw(),
                        &raw_input.proxy,
                        raw_input.translation,
                        filter.raw(),
                        Some(cast_trampoline::<F>),
                        (&mut ctx as *mut CastContext<_>).cast(),
                    )
                };
                ctx.state.drain()?;
                Ok(TreeStats::from_raw(stats))
            }
        })();
        drop(owner_call_frame);
        result
    }

    /// Casts a capsule-shaped character mover through the world.
    ///
    /// `origin` is a world position, while `mover` is expressed relative to it. `translation` is
    /// the desired displacement.
    ///
    /// Returns the safe travel fraction in `[0, 1]`. Use [`Self::collide_mover`] at the final
    /// position to gather contact planes; Box3D's mover cast is for swept motion, not contact
    /// inspection. This non-callback path is supported in provider-mode WASM.
    pub fn cast_mover(
        &self,
        origin: impl Into<Pos>,
        mover: &Capsule,
        translation: impl Into<Vec3>,
        filter: QueryFilter,
    ) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let origin = origin.into().validate()?;
        let translation = translation.into().validate()?;
        mover.validate()?;
        let _call = self.enter_world_call()?;
        Ok(unsafe {
            ffi::b3World_CastMover(
                self.raw(),
                origin.into_raw(),
                mover.raw(),
                translation.into_raw(),
                filter.raw(),
                None,
                std::ptr::null_mut(),
            )
        })
    }

    /// Collects contact planes for a capsule mover at the world-space `origin`.
    ///
    /// The mover and returned plane geometry are expressed relative to `origin`. See
    /// [`Self::visit_collide_mover`] for callback and platform semantics.
    pub fn collide_mover(
        &self,
        origin: impl Into<Pos>,
        mover: &Capsule,
        filter: QueryFilter,
    ) -> Result<Vec<MoverPlane>> {
        let mut out = Vec::new();
        self.collide_mover_into(origin, mover, filter, &mut out)?;
        Ok(out)
    }

    /// Writes mover contact planes into `out`, clearing it first.
    ///
    /// The mover and returned plane geometry are relative to the world-space `origin`. The output
    /// may contain a partial result if traversal fails. See [`Self::visit_collide_mover`] for
    /// callback and platform semantics.
    pub fn collide_mover_into(
        &self,
        origin: impl Into<Pos>,
        mover: &Capsule,
        filter: QueryFilter,
        out: &mut Vec<MoverPlane>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        out.clear();
        self.visit_collide_mover(origin, mover, filter, |plane| {
            out.push(plane);
            true
        })
    }

    /// Visits contact planes for a capsule mover at the world-space `origin`.
    ///
    /// The mover and returned plane geometry are expressed relative to `origin`. Returning `false`
    /// from `visitor` stops plane collection early. Box3D may report shapes and planes in any order.
    /// The visitor is synchronous and is never retained; safe API reentry returns
    /// [`Error::InCallback`]. On unwind-capable targets, a panic stops collection and is returned as
    /// [`Error::CallbackPanicked`]. `panic=abort` targets cannot contain a panic.
    ///
    /// Provider-mode WASM does not bridge mover collision visitors and returns
    /// [`Error::UnsupportedOnWasm`] before invoking `visitor`.
    pub fn visit_collide_mover<F>(
        &self,
        origin: impl Into<Pos>,
        mover: &Capsule,
        filter: QueryFilter,
        visitor: F,
    ) -> Result<()>
    where
        F: FnMut(MoverPlane) -> bool,
    {
        callback_state::check_not_in_callback()?;
        let owner_call_frame = callback_state::OwnerCallFrame::enter();
        let result = (move || {
            #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
            {
                let _ = (origin, mover, filter, visitor);
                Err(Error::UnsupportedOnWasm)
            }
            #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
            {
                let origin = origin.into().validate()?;
                mover.validate()?;
                let mut ctx = MoverPlaneContext {
                    world: self,
                    visitor,
                    state: LocalCallbackState::new(),
                };
                let _call = self.enter_world_call()?;
                unsafe {
                    ffi::b3World_CollideMover(
                        self.raw(),
                        origin.into_raw(),
                        mover.raw(),
                        filter.raw(),
                        Some(mover_plane_trampoline::<F>),
                        (&mut ctx as *mut MoverPlaneContext<_>).cast(),
                    )
                };
                ctx.state.drain()
            }
        })();
        drop(owner_call_frame);
        result
    }
}

struct OverlapContext<'a, F> {
    world: &'a World,
    visitor: F,
    state: LocalCallbackState,
}

unsafe extern "C" fn overlap_trampoline<F>(
    shape_id: ffi::b3ShapeId,
    context: *mut std::ffi::c_void,
) -> bool
where
    F: FnMut(ShapeId) -> bool,
{
    let ctx = unsafe { &mut *context.cast::<OverlapContext<'_, F>>() };
    let OverlapContext {
        world,
        visitor,
        state,
    } = ctx;
    let mut pending_error = None;
    let keep_going = state.invoke(false, || {
        let shape_id = match world.state().ledger.resolve_shape(shape_id) {
            Ok(shape_id) => shape_id,
            Err(error) => {
                pending_error = Some(error);
                return false;
            }
        };
        visitor(shape_id)
    });
    match pending_error {
        Some(error) => state.fail(error, false),
        None => keep_going,
    }
}

struct CastContext<'a, F> {
    world: &'a World,
    visitor: F,
    state: LocalCallbackState,
}

unsafe extern "C" fn cast_trampoline<F>(
    shape_id: ffi::b3ShapeId,
    point: ffi::b3Pos,
    normal: ffi::b3Vec3,
    fraction: f32,
    user_material_id: u64,
    triangle_index: i32,
    child_index: i32,
    context: *mut std::ffi::c_void,
) -> f32
where
    F: FnMut(RayHit) -> f32,
{
    let ctx = unsafe { &mut *context.cast::<CastContext<'_, F>>() };
    let CastContext {
        world,
        visitor,
        state,
    } = ctx;
    let mut pending_error = None;
    let next_fraction = state.invoke(0.0, || {
        let shape_id = match world.state().ledger.resolve_shape(shape_id) {
            Ok(shape_id) => shape_id,
            Err(error) => {
                pending_error = Some(error);
                return 0.0;
            }
        };
        let hit = RayHit {
            shape_id,
            point: Pos::from_raw(point),
            normal: Vec3::from_raw(normal),
            fraction,
            user_material_id,
            triangle_index,
            child_index,
        };
        let next_fraction = visitor(hit);
        if next_fraction.is_finite() {
            next_fraction
        } else {
            0.0
        }
    });
    match pending_error {
        Some(error) => state.fail(error, 0.0),
        None => next_fraction,
    }
}

struct MoverPlaneContext<'a, F> {
    world: &'a World,
    visitor: F,
    state: LocalCallbackState,
}

const MAX_MOVER_PLANE_BATCH_SIZE: i32 = 64;

unsafe extern "C" fn mover_plane_trampoline<F>(
    shape_id: ffi::b3ShapeId,
    plane: *const ffi::b3PlaneResult,
    plane_count: i32,
    context: *mut std::ffi::c_void,
) -> bool
where
    F: FnMut(MoverPlane) -> bool,
{
    let ctx = unsafe { &mut *context.cast::<MoverPlaneContext<'_, F>>() };
    let MoverPlaneContext {
        world,
        visitor,
        state,
    } = ctx;
    let mut pending_error = None;
    let keep_going = state.invoke(false, || {
        if plane.is_null()
            || !plane.is_aligned()
            || !(1..=MAX_MOVER_PLANE_BATCH_SIZE).contains(&plane_count)
        {
            pending_error = Some(Error::NativeFailure);
            return false;
        }

        let planes = unsafe { std::slice::from_raw_parts(plane, plane_count as usize) };
        let shape_id = match world.state().ledger.resolve_shape(shape_id) {
            Ok(shape_id) => shape_id,
            Err(error) => {
                pending_error = Some(error);
                return false;
            }
        };
        for raw_plane in planes {
            let mover_plane = MoverPlane::from_raw(shape_id, *raw_plane);
            if !visitor(mover_plane) {
                return false;
            }
        }

        true
    });
    match pending_error {
        Some(error) => state.fail(error, false),
        None => keep_going,
    }
}
