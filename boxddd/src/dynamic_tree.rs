#![cfg_attr(all(target_arch = "wasm32", boxddd_wasm_provider), allow(dead_code))]

use crate::collision::{BoxCastInput, RayCastInput};
use crate::core::{
    box3d_lock,
    callback_state::{self, LocalCallbackState},
    foundation::{Foundation, OrdinaryLease},
    provenance::{OwnerToken, ResourceToken, allocate_owner_token, allocate_resource_token},
    validation,
};
use crate::error::{Error, HandleKind, InvalidValueReason, Result};
use crate::query::TreeStats;
use crate::types::{Aabb, Vec3};
use boxddd_sys::ffi;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
/// Stable handle for a proxy stored in a [`DynamicTree`].
///
/// This is an opaque, inert value. Its private owner and resource tokens prevent both cross-tree
/// use and reuse of a native proxy slot after destruction.
pub struct DynamicTreeProxyId {
    index: i32,
    owner: OwnerToken,
    resource: ResourceToken,
}

impl DynamicTreeProxyId {
    #[inline]
    const fn new(index: i32, owner: OwnerToken, resource: ResourceToken) -> Self {
        Self {
            index,
            owner,
            resource,
        }
    }

    #[inline]
    const fn into_raw(self) -> i32 {
        self.index
    }
}

impl fmt::Debug for DynamicTreeProxyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DynamicTreeProxyId(..)")
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
/// Snapshot of a proxy tracked by a [`DynamicTree`].
pub struct DynamicTreeProxy {
    /// Current axis-aligned bounds for the proxy.
    pub aabb: Aabb,
    /// Category bits used by query and cast filters.
    pub category_bits: u64,
    /// Caller-owned payload returned by query and cast callbacks.
    pub user_data: u64,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Category filter used by dynamic tree queries and casts.
pub struct DynamicTreeFilter {
    /// Category bit mask to test against candidate proxies.
    pub mask_bits: u64,
    /// When true, every bit in `mask_bits` must be present on the proxy category.
    pub require_all_bits: bool,
}

impl DynamicTreeFilter {
    /// Creates a filter that accepts proxies sharing any bit in `mask_bits`.
    #[inline]
    pub const fn new(mask_bits: u64) -> Self {
        Self {
            mask_bits,
            require_all_bits: false,
        }
    }

    /// Sets whether matching requires all mask bits instead of any shared bit.
    #[inline]
    pub const fn require_all_bits(mut self, require_all_bits: bool) -> Self {
        self.require_all_bits = require_all_bits;
        self
    }
}

impl Default for DynamicTreeFilter {
    fn default() -> Self {
        Self {
            mask_bits: u64::MAX,
            require_all_bits: false,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
/// Result item produced by an AABB query.
pub struct DynamicTreeHit {
    /// Proxy that matched the query.
    pub proxy_id: DynamicTreeProxyId,
    /// Caller-owned payload stored on the proxy.
    pub user_data: u64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Candidate passed to a closest-point query callback.
pub struct DynamicTreeClosestHit {
    /// Current squared distance bound reported by Box3D for this candidate.
    pub min_distance_squared: f32,
    /// Proxy being visited.
    pub proxy_id: DynamicTreeProxyId,
    /// Caller-owned payload stored on the proxy.
    pub user_data: u64,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
/// Summary returned by a closest-point query.
pub struct DynamicTreeClosestResult {
    /// Traversal statistics reported by Box3D.
    pub stats: TreeStats,
    /// Final squared distance bound after all callback updates.
    pub min_distance_squared: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Candidate passed to a dynamic-tree ray-cast callback.
pub struct DynamicTreeRayCastHit {
    /// Ray input clipped to the current candidate interval.
    pub input: RayCastInput,
    /// Proxy being visited.
    pub proxy_id: DynamicTreeProxyId,
    /// Caller-owned payload stored on the proxy.
    pub user_data: u64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
/// Candidate passed to a dynamic-tree box-cast callback.
pub struct DynamicTreeBoxCastHit {
    /// Box-cast input clipped to the current candidate interval.
    pub input: BoxCastInput,
    /// Proxy being visited.
    pub proxy_id: DynamicTreeProxyId,
    /// Caller-owned payload stored on the proxy.
    pub user_data: u64,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq)]
/// Callback control value for dynamic-tree ray and box casts.
pub enum DynamicTreeCastControl {
    /// Continue traversal without changing the current maximum fraction.
    Continue,
    /// Clip future traversal to the supplied fraction.
    ///
    /// The fraction must be finite and within the current cast interval. An
    /// invalid clip fraction makes the visit method return
    /// [`Error::InvalidValue`].
    Clip(f32),
    /// Skip the current hit while continuing traversal.
    Skip,
    /// Stop traversal immediately.
    Terminate,
}

impl DynamicTreeCastControl {
    fn into_raw(self, max_fraction: f32) -> Result<f32> {
        match self {
            Self::Continue => Ok(max_fraction),
            Self::Clip(fraction) => {
                validation::finite("dynamic_tree.cast.clip_fraction", fraction)?;
                if (0.0..=max_fraction).contains(&fraction) {
                    Ok(fraction)
                } else {
                    Err(validation::invalid(
                        "dynamic_tree.cast.clip_fraction",
                        InvalidValueReason::OutOfRange,
                    ))
                }
            }
            Self::Skip => Ok(-1.0),
            Self::Terminate => Ok(0.0),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct ProxyEntry {
    resource: ResourceToken,
    proxy: DynamicTreeProxy,
}

/// Standalone Box3D dynamic AABB tree.
///
/// `DynamicTree` owns the native tree and releases it on drop. It is intentionally neither `Send`
/// nor `Sync` because callbacks enter Rust through raw context pointers and Box3D access is
/// serialized by the crate-wide Box3D lock.
pub struct DynamicTree {
    inner: Option<DynamicTreeInner>,
}

struct DynamicTreeInner {
    raw: ffi::b3DynamicTree,
    owner: OwnerToken,
    proxies: HashMap<i32, ProxyEntry>,
    has_enlarged_nodes: bool,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl DynamicTree {
    /// Creates an empty dynamic tree.
    pub fn new() -> Result<Self> {
        callback_state::check_not_in_callback()?;
        Self::with_capacity(0)
    }

    /// Creates an empty dynamic tree with an initial proxy capacity hint.
    pub fn with_capacity(proxy_capacity: usize) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        if proxy_capacity > i32::MAX as usize / 2 {
            return Err(validation::invalid(
                "dynamic_tree.proxy_capacity",
                InvalidValueReason::OutOfRange,
            ));
        }
        let proxy_capacity = i32::try_from(proxy_capacity).map_err(|_| {
            validation::invalid(
                "dynamic_tree.proxy_capacity",
                InvalidValueReason::OutOfRange,
            )
        })?;
        let foundation_lease = Foundation::get()?.acquire_ordinary()?;
        let owner = allocate_owner_token()?;
        let _guard = box3d_lock::lock()?;
        let raw = unsafe { ffi::b3DynamicTree_Create(proxy_capacity) };
        if raw.nodes.is_null() {
            return Err(Error::NativeFailure);
        }
        Ok(Self {
            inner: Some(DynamicTreeInner {
                raw,
                owner,
                proxies: HashMap::new(),
                has_enlarged_nodes: false,
                _foundation_lease: foundation_lease,
                _not_send_sync: PhantomData,
            }),
        })
    }

    /// Inserts a proxy with default category bits and caller-owned `user_data`.
    pub fn create_proxy(&mut self, aabb: Aabb, user_data: u64) -> Result<DynamicTreeProxyId> {
        callback_state::check_not_in_callback()?;
        self.create_proxy_with_category_bits(aabb, u64::MAX, user_data)
    }

    /// Inserts a proxy with explicit category bits and caller-owned `user_data`.
    pub fn create_proxy_with_category_bits(
        &mut self,
        aabb: Aabb,
        category_bits: u64,
        user_data: u64,
    ) -> Result<DynamicTreeProxyId> {
        callback_state::check_not_in_callback()?;
        let aabb = aabb.validate()?;
        self.inner_mut()
            .proxies
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let resource = allocate_resource_token()?;
        let _guard = box3d_lock::lock()?;
        let proxy_id = unsafe {
            ffi::b3DynamicTree_CreateProxy(
                &mut self.inner_mut().raw,
                aabb.into_raw(),
                category_bits,
                user_data,
            )
        };
        if proxy_id < 0 {
            return Err(Error::NativeFailure);
        }
        let inner = self.inner_mut();
        let previous = inner.proxies.insert(
            proxy_id,
            ProxyEntry {
                resource,
                proxy: DynamicTreeProxy {
                    aabb,
                    category_bits,
                    user_data,
                },
            },
        );
        debug_assert!(previous.is_none(), "native proxy index reused while active");
        Ok(DynamicTreeProxyId::new(proxy_id, inner.owner, resource))
    }

    /// Removes a proxy from the tree.
    pub fn destroy_proxy(&mut self, proxy_id: DynamicTreeProxyId) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _guard = box3d_lock::lock()?;
        let inner = self.inner_mut();
        unsafe { ffi::b3DynamicTree_DestroyProxy(&mut inner.raw, proxy_index) };
        inner.proxies.remove(&proxy_index);
        if inner.proxies.is_empty() {
            inner.has_enlarged_nodes = false;
        }
        Ok(())
    }

    /// Moves an existing proxy to a new AABB.
    pub fn move_proxy(&mut self, proxy_id: DynamicTreeProxyId, aabb: Aabb) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let aabb = aabb.validate()?;
        let _guard = box3d_lock::lock()?;
        let inner = self.inner_mut();
        unsafe { ffi::b3DynamicTree_MoveProxy(&mut inner.raw, proxy_index, aabb.into_raw()) };
        inner
            .proxies
            .get_mut(&proxy_index)
            .expect("proxy index validated")
            .proxy
            .aabb = aabb;
        Ok(())
    }

    /// Enlarges an existing proxy AABB without rebuilding the tree.
    ///
    /// The new AABB must strictly contain the current one; call [`Self::rebuild`] before
    /// [`Self::validate_no_enlarged`] if enlarged nodes should be eliminated.
    pub fn enlarge_proxy(&mut self, proxy_id: DynamicTreeProxyId, aabb: Aabb) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let aabb = aabb.validate()?;
        let current = self
            .inner()
            .proxies
            .get(&proxy_index)
            .expect("proxy index validated")
            .proxy
            .aabb;
        if !aabb_contains(aabb, current) || aabb_contains(current, aabb) {
            return Err(validation::invalid(
                "dynamic_tree.enlarge_proxy.aabb",
                InvalidValueReason::InvalidCombination,
            ));
        }
        let _guard = box3d_lock::lock()?;
        let inner = self.inner_mut();
        unsafe { ffi::b3DynamicTree_EnlargeProxy(&mut inner.raw, proxy_index, aabb.into_raw()) };
        inner
            .proxies
            .get_mut(&proxy_index)
            .expect("proxy index validated")
            .proxy
            .aabb = aabb;
        inner.has_enlarged_nodes = true;
        Ok(())
    }

    /// Replaces the category bits for a proxy.
    pub fn set_category_bits(
        &mut self,
        proxy_id: DynamicTreeProxyId,
        category_bits: u64,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _guard = box3d_lock::lock()?;
        let inner = self.inner_mut();
        unsafe { ffi::b3DynamicTree_SetCategoryBits(&mut inner.raw, proxy_index, category_bits) };
        inner
            .proxies
            .get_mut(&proxy_index)
            .expect("proxy index validated")
            .proxy
            .category_bits = category_bits;
        Ok(())
    }

    /// Returns the category bits currently stored by Box3D for a proxy.
    pub fn category_bits(&mut self, proxy_id: DynamicTreeProxyId) -> Result<u64> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _guard = box3d_lock::lock()?;
        Ok(unsafe { ffi::b3DynamicTree_GetCategoryBits(&mut self.inner_mut().raw, proxy_index) })
    }

    /// Returns the Rust-side snapshot for a live proxy.
    pub fn proxy(&self, proxy_id: DynamicTreeProxyId) -> Result<DynamicTreeProxy> {
        callback_state::check_not_in_callback()?;
        Ok(self.proxy_entry(proxy_id)?.proxy)
    }

    /// Returns true when `proxy_id` still refers to a live proxy in this tree.
    pub fn contains_proxy(&self, proxy_id: DynamicTreeProxyId) -> bool {
        self.proxy_entry(proxy_id).is_ok()
    }

    /// Returns the number of live proxies stored in the native tree.
    pub fn proxy_count(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock()?;
        let count = unsafe { ffi::b3DynamicTree_GetProxyCount(&self.inner().raw) };
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Returns the native heap memory currently owned by the tree, in bytes.
    pub fn byte_count(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock()?;
        let count = unsafe { ffi::b3DynamicTree_GetByteCount(&self.inner().raw) };
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Returns the current height of the native AABB tree.
    pub fn height(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock()?;
        Ok(unsafe { ffi::b3DynamicTree_GetHeight(&self.inner().raw) })
    }

    /// Returns the tree area ratio reported by Box3D.
    pub fn area_ratio(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock()?;
        let ratio = unsafe { ffi::b3DynamicTree_GetAreaRatio(&self.inner().raw) };
        if ratio.is_finite() && ratio >= 0.0 {
            Ok(ratio)
        } else {
            Err(Error::NativeFailure)
        }
    }

    /// Returns the root AABB, or `None` when the tree has no proxies.
    pub fn root_bounds(&self) -> Result<Option<Aabb>> {
        callback_state::check_not_in_callback()?;
        if self.inner().proxies.is_empty() {
            return Ok(None);
        }
        let _guard = box3d_lock::lock()?;
        let aabb = Aabb::from_raw(unsafe { ffi::b3DynamicTree_GetRootBounds(&self.inner().raw) });
        Ok(Some(aabb.validate().map_err(|_| Error::NativeFailure)?))
    }

    /// Rebuilds the native tree and returns the number of boxes sorted by Box3D.
    pub fn rebuild(&mut self, full_build: bool) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock()?;
        let inner = self.inner_mut();
        let count = unsafe { ffi::b3DynamicTree_Rebuild(&mut inner.raw, full_build) };
        inner.has_enlarged_nodes = false;
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Runs Box3D's internal dynamic-tree validation checks.
    pub fn validate(&self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock()?;
        unsafe { ffi::b3DynamicTree_Validate(&self.inner().raw) };
        Ok(())
    }

    /// Runs Box3D validation that asserts no enlarged nodes remain.
    ///
    /// This returns [`Error::InvalidValue`] if [`Self::enlarge_proxy`] has been called and the
    /// tree has not subsequently been rebuilt.
    pub fn validate_no_enlarged(&self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        if self.inner().has_enlarged_nodes {
            return Err(validation::invalid(
                "dynamic_tree.enlarged_nodes",
                InvalidValueReason::InvalidCombination,
            ));
        }
        let _guard = box3d_lock::lock()?;
        unsafe { ffi::b3DynamicTree_ValidateNoEnlarged(&self.inner().raw) };
        Ok(())
    }

    /// Collects all proxies whose bounds overlap `aabb` and pass `filter`.
    pub fn query(&self, aabb: Aabb, filter: DynamicTreeFilter) -> Result<Vec<DynamicTreeHit>> {
        callback_state::check_not_in_callback()?;
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
                let _guard = box3d_lock::lock()?;
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
                let _guard = box3d_lock::lock()?;
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
                let _guard = box3d_lock::lock()?;
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
                let _guard = box3d_lock::lock()?;
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

    fn proxy_index(&self, proxy_id: DynamicTreeProxyId) -> Result<i32> {
        let index = proxy_id.into_raw();
        self.proxy_entry(proxy_id)?;
        Ok(index)
    }

    fn inner(&self) -> &DynamicTreeInner {
        self.inner
            .as_ref()
            .expect("dynamic tree inner is present outside drop")
    }

    fn inner_mut(&mut self) -> &mut DynamicTreeInner {
        self.inner
            .as_mut()
            .expect("dynamic tree inner is present outside drop")
    }

    fn proxy_entry(&self, proxy_id: DynamicTreeProxyId) -> Result<&ProxyEntry> {
        let inner = self.inner();
        if proxy_id.owner != inner.owner {
            return Err(Error::ForeignHandle {
                kind: HandleKind::DynamicTreeProxy,
            });
        }
        let index = proxy_id.into_raw();
        let Some(entry) = inner.proxies.get(&index) else {
            return Err(Error::StaleHandle {
                kind: HandleKind::DynamicTreeProxy,
            });
        };
        if proxy_id.resource == entry.resource {
            Ok(entry)
        } else {
            Err(Error::StaleHandle {
                kind: HandleKind::DynamicTreeProxy,
            })
        }
    }
}

impl DynamicTreeInner {
    fn destroy(self) {
        let mut owner = callback_state::RetainOnUnwind::new(self);
        if !owner.raw.nodes.is_null() {
            {
                let _guard = box3d_lock::lock_for_cleanup();
                unsafe { ffi::b3DynamicTree_Destroy(&mut owner.raw) };
            }
        }
        owner.finish();
    }
}

impl Drop for DynamicTree {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };

        if callback_state::in_callback() {
            callback_state::defer_local_cleanup_or_retain(move || inner.destroy());
        } else {
            inner.destroy();
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

fn aabb_contains(outer: Aabb, inner: Aabb) -> bool {
    outer.lower_bound.x <= inner.lower_bound.x
        && outer.lower_bound.y <= inner.lower_bound.y
        && outer.lower_bound.z <= inner.lower_bound.z
        && inner.upper_bound.x <= outer.upper_bound.x
        && inner.upper_bound.y <= outer.upper_bound.y
        && inner.upper_bound.z <= outer.upper_bound.z
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb(lower: f32, upper: f32) -> Aabb {
        Aabb {
            lower_bound: Vec3::new(lower, lower, lower),
            upper_bound: Vec3::new(upper, upper, upper),
        }
    }

    #[test]
    fn recycled_native_proxy_slot_receives_a_new_resource_token() -> Result<()> {
        crate::Foundation::initialize_default()?;
        let mut tree = DynamicTree::new()?;
        let first = tree.create_proxy(aabb(-1.0, 1.0), 1)?;
        tree.destroy_proxy(first)?;

        let replacement = tree.create_proxy(aabb(-1.0, 1.0), 2)?;
        assert_eq!(replacement.index, first.index);
        assert_ne!(replacement.resource, first.resource);
        assert_eq!(
            tree.proxy(first),
            Err(Error::StaleHandle {
                kind: HandleKind::DynamicTreeProxy,
            })
        );
        assert_eq!(tree.proxy(replacement)?.user_data, 2);
        Ok(())
    }

    #[test]
    fn invalid_cast_clip_reports_precise_validation_error() {
        assert_eq!(
            DynamicTreeCastControl::Clip(f32::NAN).into_raw(1.0),
            Err(Error::InvalidValue {
                context: "dynamic_tree.cast.clip_fraction",
                reason: InvalidValueReason::NonFinite,
            })
        );
        assert_eq!(
            DynamicTreeCastControl::Clip(2.0).into_raw(1.0),
            Err(Error::InvalidValue {
                context: "dynamic_tree.cast.clip_fraction",
                reason: InvalidValueReason::OutOfRange,
            })
        );
    }

    #[test]
    fn excessive_capacity_is_a_caller_input_error() {
        assert!(matches!(
            DynamicTree::with_capacity(usize::MAX),
            Err(Error::InvalidValue {
                context: "dynamic_tree.proxy_capacity",
                reason: InvalidValueReason::OutOfRange,
            })
        ));
    }
}
