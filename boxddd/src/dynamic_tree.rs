#![cfg_attr(all(target_arch = "wasm32", boxddd_wasm_provider), allow(dead_code))]

use crate::collision::{BoxCastInput, RayCastInput};
use crate::core::{
    box3d_lock,
    callback_state::{self, LocalCallbackState},
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
    raw: ffi::b3DynamicTree,
    owner: OwnerToken,
    proxies: HashMap<i32, ProxyEntry>,
    has_enlarged_nodes: bool,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl DynamicTree {
    /// Creates an empty dynamic tree.
    pub fn new() -> Result<Self> {
        Self::with_capacity(0)
    }

    /// Creates an empty dynamic tree with an initial proxy capacity hint.
    pub fn with_capacity(proxy_capacity: usize) -> Result<Self> {
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
        callback_state::check_not_in_callback()?;
        let owner = allocate_owner_token()?;
        let _guard = box3d_lock::lock();
        let raw = unsafe { ffi::b3DynamicTree_Create(proxy_capacity) };
        if raw.nodes.is_null() {
            return Err(Error::NativeFailure);
        }
        Ok(Self {
            raw,
            owner,
            proxies: HashMap::new(),
            has_enlarged_nodes: false,
            _not_send_sync: PhantomData,
        })
    }

    /// Inserts a proxy with default category bits and caller-owned `user_data`.
    pub fn create_proxy(&mut self, aabb: Aabb, user_data: u64) -> Result<DynamicTreeProxyId> {
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
        self.proxies
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let resource = allocate_resource_token()?;
        let _guard = box3d_lock::lock();
        let proxy_id = unsafe {
            ffi::b3DynamicTree_CreateProxy(&mut self.raw, aabb.into_raw(), category_bits, user_data)
        };
        if proxy_id < 0 {
            return Err(Error::NativeFailure);
        }
        let previous = self.proxies.insert(
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
        Ok(DynamicTreeProxyId::new(proxy_id, self.owner, resource))
    }

    /// Removes a proxy from the tree.
    pub fn destroy_proxy(&mut self, proxy_id: DynamicTreeProxyId) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_DestroyProxy(&mut self.raw, proxy_index) };
        self.proxies.remove(&proxy_index);
        if self.proxies.is_empty() {
            self.has_enlarged_nodes = false;
        }
        Ok(())
    }

    /// Moves an existing proxy to a new AABB.
    pub fn move_proxy(&mut self, proxy_id: DynamicTreeProxyId, aabb: Aabb) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let aabb = aabb.validate()?;
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_MoveProxy(&mut self.raw, proxy_index, aabb.into_raw()) };
        self.proxies
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
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_EnlargeProxy(&mut self.raw, proxy_index, aabb.into_raw()) };
        self.proxies
            .get_mut(&proxy_index)
            .expect("proxy index validated")
            .proxy
            .aabb = aabb;
        self.has_enlarged_nodes = true;
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
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_SetCategoryBits(&mut self.raw, proxy_index, category_bits) };
        self.proxies
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
        let _guard = box3d_lock::lock();
        Ok(unsafe { ffi::b3DynamicTree_GetCategoryBits(&mut self.raw, proxy_index) })
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
        let _guard = box3d_lock::lock();
        let count = unsafe { ffi::b3DynamicTree_GetProxyCount(&self.raw) };
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Returns the native heap memory currently owned by the tree, in bytes.
    pub fn byte_count(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        let count = unsafe { ffi::b3DynamicTree_GetByteCount(&self.raw) };
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Returns the current height of the native AABB tree.
    pub fn height(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        Ok(unsafe { ffi::b3DynamicTree_GetHeight(&self.raw) })
    }

    /// Returns the tree area ratio reported by Box3D.
    pub fn area_ratio(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        let ratio = unsafe { ffi::b3DynamicTree_GetAreaRatio(&self.raw) };
        if ratio.is_finite() && ratio >= 0.0 {
            Ok(ratio)
        } else {
            Err(Error::NativeFailure)
        }
    }

    /// Returns the root AABB, or `None` when the tree has no proxies.
    pub fn root_bounds(&self) -> Result<Option<Aabb>> {
        callback_state::check_not_in_callback()?;
        if self.proxies.is_empty() {
            return Ok(None);
        }
        let _guard = box3d_lock::lock();
        let aabb = Aabb::from_raw(unsafe { ffi::b3DynamicTree_GetRootBounds(&self.raw) });
        Ok(Some(aabb.validate().map_err(|_| Error::NativeFailure)?))
    }

    /// Rebuilds the native tree and returns the number of boxes sorted by Box3D.
    pub fn rebuild(&mut self, full_build: bool) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        let count = unsafe { ffi::b3DynamicTree_Rebuild(&mut self.raw, full_build) };
        self.has_enlarged_nodes = false;
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Runs Box3D's internal dynamic-tree validation checks.
    pub fn validate(&self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_Validate(&self.raw) };
        Ok(())
    }

    /// Runs Box3D validation that asserts no enlarged nodes remain.
    ///
    /// This returns [`Error::InvalidValue`] if [`Self::enlarge_proxy`] has been called and the
    /// tree has not subsequently been rebuilt.
    pub fn validate_no_enlarged(&self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        if self.has_enlarged_nodes {
            return Err(validation::invalid(
                "dynamic_tree.enlarged_nodes",
                InvalidValueReason::InvalidCombination,
            ));
        }
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_ValidateNoEnlarged(&self.raw) };
        Ok(())
    }

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
            let mut ctx = QueryContext {
                visitor,
                proxies: &self.proxies as *const HashMap<i32, ProxyEntry>,
                owner: self.owner,
                state: LocalCallbackState::new(),
            };
            let _guard = box3d_lock::lock();
            let stats = unsafe {
                ffi::b3DynamicTree_Query(
                    &self.raw,
                    aabb.into_raw(),
                    filter.mask_bits,
                    filter.require_all_bits,
                    Some(query_trampoline::<F>),
                    (&mut ctx as *mut QueryContext<_>).cast(),
                )
            };
            ctx.state.drain()?;
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
            let mut ctx = ClosestContext {
                visitor,
                proxies: &self.proxies as *const HashMap<i32, ProxyEntry>,
                owner: self.owner,
                state: LocalCallbackState::new(),
            };
            let mut min_distance_squared = min_distance_squared;
            let _guard = box3d_lock::lock();
            let stats = unsafe {
                ffi::b3DynamicTree_QueryClosest(
                    &self.raw,
                    point.into_raw(),
                    filter.mask_bits,
                    filter.require_all_bits,
                    Some(closest_trampoline::<F>),
                    (&mut ctx as *mut ClosestContext<_>).cast(),
                    &mut min_distance_squared,
                )
            };
            ctx.state.drain()?;
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
            let _guard = box3d_lock::lock();
            if !unsafe { ffi::b3IsValidRay(&raw_input) } {
                return Err(validation::invalid(
                    "ray_cast.max_fraction",
                    InvalidValueReason::OutOfRange,
                ));
            }
            let mut ctx = RayCastContext {
                visitor,
                proxies: &self.proxies as *const HashMap<i32, ProxyEntry>,
                owner: self.owner,
                state: LocalCallbackState::new(),
            };
            let stats = unsafe {
                ffi::b3DynamicTree_RayCast(
                    &self.raw,
                    &raw_input,
                    filter.mask_bits,
                    filter.require_all_bits,
                    Some(ray_cast_trampoline::<F>),
                    (&mut ctx as *mut RayCastContext<_>).cast(),
                )
            };
            ctx.state.drain()?;
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
            let mut ctx = BoxCastContext {
                visitor,
                proxies: &self.proxies as *const HashMap<i32, ProxyEntry>,
                owner: self.owner,
                state: LocalCallbackState::new(),
            };
            let _guard = box3d_lock::lock();
            let stats = unsafe {
                ffi::b3DynamicTree_BoxCast(
                    &self.raw,
                    &raw_input,
                    filter.mask_bits,
                    filter.require_all_bits,
                    Some(box_cast_trampoline::<F>),
                    (&mut ctx as *mut BoxCastContext<_>).cast(),
                )
            };
            ctx.state.drain()?;
            Ok(TreeStats::from_raw(stats))
        }
    }

    fn proxy_index(&self, proxy_id: DynamicTreeProxyId) -> Result<i32> {
        let index = proxy_id.into_raw();
        self.proxy_entry(proxy_id)?;
        Ok(index)
    }

    fn proxy_entry(&self, proxy_id: DynamicTreeProxyId) -> Result<&ProxyEntry> {
        if proxy_id.owner != self.owner {
            return Err(Error::ForeignHandle {
                kind: HandleKind::DynamicTreeProxy,
            });
        }
        let index = proxy_id.into_raw();
        let Some(entry) = self.proxies.get(&index) else {
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

impl Drop for DynamicTree {
    fn drop(&mut self) {
        if self.raw.nodes.is_null() {
            return;
        }
        let _guard = box3d_lock::lock();
        unsafe { ffi::b3DynamicTree_Destroy(&mut self.raw) };
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
    let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
        return ctx.state.fail(Error::NativeFailure, false);
    };
    let hit = DynamicTreeHit {
        proxy_id,
        user_data,
    };
    ctx.state.invoke(false, || (ctx.visitor)(hit))
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
    let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
        return ctx.state.fail(Error::NativeFailure, min_distance_squared);
    };
    let hit = DynamicTreeClosestHit {
        min_distance_squared,
        proxy_id,
        user_data,
    };
    let next_min = ctx
        .state
        .invoke(min_distance_squared, || (ctx.visitor)(hit));
    if next_min.is_finite() && next_min >= 0.0 {
        next_min
    } else {
        min_distance_squared
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
    if input.is_null() {
        return ctx.state.fail(Error::NativeFailure, 0.0);
    }
    let input = unsafe { *input };
    let Ok(input) = RayCastInput::with_max_fraction(
        Vec3::from_raw(input.origin),
        Vec3::from_raw(input.translation),
        input.maxFraction,
    ) else {
        return ctx.state.fail(Error::NativeFailure, 0.0);
    };
    let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
        return ctx.state.fail(Error::NativeFailure, 0.0);
    };
    let hit = DynamicTreeRayCastHit {
        input,
        proxy_id,
        user_data,
    };
    let control = ctx
        .state
        .invoke(DynamicTreeCastControl::Terminate, || (ctx.visitor)(hit));
    match control.into_raw(input.max_fraction) {
        Ok(next_fraction) => next_fraction,
        Err(error) => ctx.state.fail(error, 0.0),
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
    if input.is_null() {
        return ctx.state.fail(Error::NativeFailure, 0.0);
    }
    let input = unsafe { *input };
    let Ok(input) = BoxCastInput::with_max_fraction(
        Aabb::from_raw(input.box_),
        Vec3::from_raw(input.translation),
        input.maxFraction,
    ) else {
        return ctx.state.fail(Error::NativeFailure, 0.0);
    };
    let Some(proxy_id) = proxy_id_from_context(proxy_id, ctx.proxies, ctx.owner) else {
        return ctx.state.fail(Error::NativeFailure, 0.0);
    };
    let hit = DynamicTreeBoxCastHit {
        input,
        proxy_id,
        user_data,
    };
    let control = ctx
        .state
        .invoke(DynamicTreeCastControl::Terminate, || (ctx.visitor)(hit));
    match control.into_raw(input.max_fraction) {
        Ok(next_fraction) => next_fraction,
        Err(error) => ctx.state.fail(error, 0.0),
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
