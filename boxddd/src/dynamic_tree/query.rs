use super::{
    DynamicTree, DynamicTreeBoxCastHit, DynamicTreeCastControl, DynamicTreeClosestHit,
    DynamicTreeClosestResult, DynamicTreeFilter, DynamicTreeHit, DynamicTreeProxyId,
    DynamicTreeRayCastHit, ProxyEntry,
};
use crate::collision::{BoxCastInput, RayCastInput};
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::core::validation;
use crate::core::{
    callback_state::{self, LocalCallbackState},
    provenance::OwnerToken,
};
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::error::InvalidValueReason;
use crate::error::{Error, Result};
use crate::query::TreeStats;
use crate::types::{Aabb, Vec3};
use boxddd_sys::ffi;
use std::collections::HashMap;

impl DynamicTree {
    /// Collects all proxies whose bounds overlap `aabb` and pass `filter`.
    pub fn query(&self, aabb: Aabb, filter: DynamicTreeFilter) -> Result<Vec<DynamicTreeHit>> {
        let mut out = Vec::new();
        self.query_into(aabb, filter, &mut out)?;
        Ok(out)
    }

    /// Writes all AABB query hits into `out`, clearing it first.
    pub fn query_into(
        &self,
        aabb: Aabb,
        filter: DynamicTreeFilter,
        out: &mut Vec<DynamicTreeHit>,
    ) -> Result<TreeStats> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        out.clear();
        self.visit_query(aabb, filter, |hit| {
            out.push(hit);
            true
        })
    }

    /// Visits proxies whose bounds overlap `aabb` and pass `filter`.
    ///
    /// Returning `false` from `visitor` stops traversal early. The visitor runs
    /// inside a Box3D callback context, so reentrant safe APIs return
    /// [`Error::InCallback`], and a panic is caught and reported as
    /// [`Error::CallbackPanicked`].
    pub fn visit_query<F>(
        &self,
        aabb: Aabb,
        filter: DynamicTreeFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(DynamicTreeHit) -> bool,
    {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (aabb, filter, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let aabb = aabb.validate()?;
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let inner = self.inner();
            let mut ctx = QueryContext {
                visitor,
                proxies: &inner.proxies as *const HashMap<i32, ProxyEntry>,
                owner: inner.owner,
                state: LocalCallbackState::new(),
            };
            let stats = {
                let _call = self.enter_call()?;
                unsafe {
                    ffi::b3DynamicTree_Query(
                        &inner.raw,
                        aabb.into_raw(),
                        filter.mask_bits,
                        filter.require_all_bits,
                        Some(query_trampoline::<F>),
                        (&mut ctx as *mut QueryContext<_>).cast(),
                    )
                }
            };
            let callback_result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            callback_result?;
            Ok(TreeStats::from_raw(stats))
        }
    }

    /// Visits closest-query candidates near `point`.
    ///
    /// The callback returns the next squared distance bound. Non-finite or
    /// negative callback results are ignored and leave the existing bound
    /// unchanged. The visitor runs inside a Box3D callback context, so reentrant
    /// safe APIs return [`Error::InCallback`], and a panic is caught and reported
    /// as [`Error::CallbackPanicked`].
    pub fn visit_query_closest<F>(
        &self,
        point: impl Into<Vec3>,
        filter: DynamicTreeFilter,
        min_distance_squared: f32,
        visitor: F,
    ) -> Result<DynamicTreeClosestResult>
    where
        F: FnMut(DynamicTreeClosestHit) -> f32,
    {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (point, filter, min_distance_squared, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let point = point.into();
            validation::vec3("dynamic_tree.query_closest.point", point)?;
            validation::nonnegative(
                "dynamic_tree.query_closest.min_distance_squared",
                min_distance_squared,
            )?;
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let inner = self.inner();
            let mut ctx = ClosestContext {
                visitor,
                proxies: &inner.proxies as *const HashMap<i32, ProxyEntry>,
                owner: inner.owner,
                state: LocalCallbackState::new(),
            };
            let mut min_distance_squared = min_distance_squared;
            let stats = {
                let _call = self.enter_call()?;
                unsafe {
                    ffi::b3DynamicTree_QueryClosest(
                        &inner.raw,
                        point.into_raw(),
                        filter.mask_bits,
                        filter.require_all_bits,
                        Some(closest_trampoline::<F>),
                        (&mut ctx as *mut ClosestContext<_>).cast(),
                        &mut min_distance_squared,
                    )
                }
            };
            let callback_result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            callback_result?;
            if !min_distance_squared.is_finite() || min_distance_squared < 0.0 {
                return Err(Error::NativeFailure);
            }
            Ok(DynamicTreeClosestResult {
                stats: TreeStats::from_raw(stats),
                min_distance_squared,
            })
        }
    }

    /// Visits proxies intersected by a ray cast through the tree.
    ///
    /// The callback controls traversal by returning [`DynamicTreeCastControl`].
    /// The visitor runs inside a Box3D callback context, so reentrant safe APIs
    /// return [`Error::InCallback`], and a panic is caught and reported as
    /// [`Error::CallbackPanicked`].
    pub fn visit_ray_cast<F>(
        &self,
        input: RayCastInput,
        filter: DynamicTreeFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(DynamicTreeRayCastHit) -> DynamicTreeCastControl,
    {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (input, filter, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let input = input.validate()?;
            let raw_input = input.raw();
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let inner = self.inner();
            let mut ctx = RayCastContext {
                visitor,
                proxies: &inner.proxies as *const HashMap<i32, ProxyEntry>,
                owner: inner.owner,
                state: LocalCallbackState::new(),
            };
            let stats = {
                let _call = self.enter_call()?;
                if !unsafe { ffi::b3IsValidRay(&raw_input) } {
                    None
                } else {
                    Some(unsafe {
                        ffi::b3DynamicTree_RayCast(
                            &inner.raw,
                            &raw_input,
                            filter.mask_bits,
                            filter.require_all_bits,
                            Some(ray_cast_trampoline::<F>),
                            (&mut ctx as *mut RayCastContext<_>).cast(),
                        )
                    })
                }
            };
            let callback_result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            let Some(stats) = stats else {
                return Err(validation::invalid(
                    "ray_cast.max_fraction",
                    InvalidValueReason::OutOfRange,
                ));
            };
            callback_result?;
            Ok(TreeStats::from_raw(stats))
        }
    }

    /// Visits proxies intersected by a swept AABB cast through the tree.
    ///
    /// The callback controls traversal by returning [`DynamicTreeCastControl`].
    /// The visitor runs inside a Box3D callback context, so reentrant safe APIs
    /// return [`Error::InCallback`], and a panic is caught and reported as
    /// [`Error::CallbackPanicked`].
    pub fn visit_box_cast<F>(
        &self,
        input: BoxCastInput,
        filter: DynamicTreeFilter,
        visitor: F,
    ) -> Result<TreeStats>
    where
        F: FnMut(DynamicTreeBoxCastHit) -> DynamicTreeCastControl,
    {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        #[cfg(all(target_arch = "wasm32", boxddd_wasm_provider))]
        {
            let _ = (input, filter, visitor);
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
        {
            let raw_input = input.validate()?.raw();
            let owner_call_frame = callback_state::OwnerCallFrame::enter();
            let inner = self.inner();
            let mut ctx = BoxCastContext {
                visitor,
                proxies: &inner.proxies as *const HashMap<i32, ProxyEntry>,
                owner: inner.owner,
                state: LocalCallbackState::new(),
            };
            let stats = {
                let _call = self.enter_call()?;
                unsafe {
                    ffi::b3DynamicTree_BoxCast(
                        &inner.raw,
                        &raw_input,
                        filter.mask_bits,
                        filter.require_all_bits,
                        Some(box_cast_trampoline::<F>),
                        (&mut ctx as *mut BoxCastContext<_>).cast(),
                    )
                }
            };
            let callback_result = ctx.state.drain();
            drop(ctx);
            drop(owner_call_frame);
            callback_result?;
            Ok(TreeStats::from_raw(stats))
        }
    }
}

struct QueryContext<F> {
    visitor: F,
    proxies: *const HashMap<i32, ProxyEntry>,
    owner: OwnerToken,
    state: LocalCallbackState,
}

unsafe extern "C" fn query_trampoline<F>(
    proxy_id: i32,
    user_data: u64,
    context: *mut std::ffi::c_void,
) -> bool
where
    F: FnMut(DynamicTreeHit) -> bool,
{
    let ctx = unsafe { &mut *context.cast::<QueryContext<F>>() };
    let mut failure = None;
    let keep_going = ctx.state.invoke(false, || {
        let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
            failure = Some(Error::NativeFailure);
            return false;
        };
        let hit = DynamicTreeHit {
            proxy_id,
            user_data,
        };
        (ctx.visitor)(hit)
    });
    match failure {
        Some(error) => ctx.state.fail(error, false),
        None => keep_going,
    }
}

struct ClosestContext<F> {
    visitor: F,
    proxies: *const HashMap<i32, ProxyEntry>,
    owner: OwnerToken,
    state: LocalCallbackState,
}

unsafe extern "C" fn closest_trampoline<F>(
    min_distance_squared: f32,
    proxy_id: i32,
    user_data: u64,
    context: *mut std::ffi::c_void,
) -> f32
where
    F: FnMut(DynamicTreeClosestHit) -> f32,
{
    let ctx = unsafe { &mut *context.cast::<ClosestContext<F>>() };
    let mut failure = None;
    let next_min = ctx.state.invoke(min_distance_squared, || {
        let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
            failure = Some(Error::NativeFailure);
            return min_distance_squared;
        };
        let hit = DynamicTreeClosestHit {
            min_distance_squared,
            proxy_id,
            user_data,
        };
        let next_min = (ctx.visitor)(hit);
        if next_min.is_finite() && next_min >= 0.0 {
            next_min
        } else {
            min_distance_squared
        }
    });
    match failure {
        Some(error) => ctx.state.fail(error, min_distance_squared),
        None => next_min,
    }
}

struct RayCastContext<F> {
    visitor: F,
    proxies: *const HashMap<i32, ProxyEntry>,
    owner: OwnerToken,
    state: LocalCallbackState,
}

unsafe extern "C" fn ray_cast_trampoline<F>(
    input: *const ffi::b3RayCastInput,
    proxy_id: i32,
    user_data: u64,
    context: *mut std::ffi::c_void,
) -> f32
where
    F: FnMut(DynamicTreeRayCastHit) -> DynamicTreeCastControl,
{
    let ctx = unsafe { &mut *context.cast::<RayCastContext<F>>() };
    let mut failure = None;
    let next_fraction = ctx.state.invoke(0.0, || {
        if input.is_null() {
            failure = Some(Error::NativeFailure);
            return 0.0;
        }
        let input = unsafe { *input };
        let Ok(input) = RayCastInput::with_max_fraction(
            Vec3::from_raw(input.origin),
            Vec3::from_raw(input.translation),
            input.maxFraction,
        ) else {
            failure = Some(Error::NativeFailure);
            return 0.0;
        };
        let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
            failure = Some(Error::NativeFailure);
            return 0.0;
        };
        let max_fraction = input.max_fraction;
        let hit = DynamicTreeRayCastHit {
            input,
            proxy_id,
            user_data,
        };
        match (ctx.visitor)(hit).into_raw(max_fraction) {
            Ok(next_fraction) => next_fraction,
            Err(error) => {
                failure = Some(error);
                0.0
            }
        }
    });
    match failure {
        Some(error) => ctx.state.fail(error, 0.0),
        None => next_fraction,
    }
}

struct BoxCastContext<F> {
    visitor: F,
    proxies: *const HashMap<i32, ProxyEntry>,
    owner: OwnerToken,
    state: LocalCallbackState,
}

unsafe extern "C" fn box_cast_trampoline<F>(
    input: *const ffi::b3BoxCastInput,
    proxy_id: i32,
    user_data: u64,
    context: *mut std::ffi::c_void,
) -> f32
where
    F: FnMut(DynamicTreeBoxCastHit) -> DynamicTreeCastControl,
{
    let ctx = unsafe { &mut *context.cast::<BoxCastContext<F>>() };
    let mut failure = None;
    let next_fraction = ctx.state.invoke(0.0, || {
        if input.is_null() {
            failure = Some(Error::NativeFailure);
            return 0.0;
        }
        let input = unsafe { *input };
        let Ok(input) = BoxCastInput::with_max_fraction(
            Aabb::from_raw(input.box_),
            Vec3::from_raw(input.translation),
            input.maxFraction,
        ) else {
            failure = Some(Error::NativeFailure);
            return 0.0;
        };
        let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
            failure = Some(Error::NativeFailure);
            return 0.0;
        };
        let max_fraction = input.max_fraction;
        let hit = DynamicTreeBoxCastHit {
            input,
            proxy_id,
            user_data,
        };
        match (ctx.visitor)(hit).into_raw(max_fraction) {
            Ok(next_fraction) => next_fraction,
            Err(error) => {
                failure = Some(error);
                0.0
            }
        }
    });
    match failure {
        Some(error) => ctx.state.fail(error, 0.0),
        None => next_fraction,
    }
}

fn proxy_id_from_context(
    proxy_id: i32,
    proxies: *const HashMap<i32, ProxyEntry>,
    owner: OwnerToken,
) -> Option<DynamicTreeProxyId> {
    unsafe { proxies.as_ref() }
        .and_then(|proxies| proxies.get(&proxy_id))
        .map(|entry| DynamicTreeProxyId::new(proxy_id, owner, entry.resource))
}
