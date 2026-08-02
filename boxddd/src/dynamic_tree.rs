#![cfg_attr(all(target_arch = "wasm32", boxddd_wasm_provider), allow(dead_code))]

use crate::collision::{BoxCastInput, RayCastInput};
use crate::core::{
    callback_state,
    foundation::{Foundation, OrdinaryLease},
    provenance::{OwnerToken, ResourceToken, allocate_owner_token, allocate_resource_token},
    validation,
};
use crate::error::{Error, HandleKind, InvalidValueReason, Result};
use crate::query::TreeStats;
use crate::types::Aabb;
#[cfg(test)]
use crate::types::Vec3;
use crate::world::creation_transaction::{
    CreationStage, force_compensation_mismatch, inject_creation_failure,
};
use boxddd_sys::ffi;
use std::cell::Cell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

mod query;

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
/// nor `Sync` because callbacks enter Rust through raw context pointers and each tree requires
/// exclusive owner-thread access.
pub struct DynamicTree {
    inner: Option<DynamicTreeInner>,
}

struct DynamicTreeInner {
    raw: ffi::b3DynamicTree,
    owner: OwnerToken,
    proxies: HashMap<i32, ProxyEntry>,
    has_enlarged_nodes: bool,
    poisoned: Cell<bool>,
    _foundation_lease: OrdinaryLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl DynamicTree {
    /// Creates an empty dynamic tree.
    pub fn new() -> Result<Self> {
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
                poisoned: Cell::new(false),
                _foundation_lease: foundation_lease,
                _not_send_sync: PhantomData,
            }),
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
        self.check_owner_healthy()?;
        let aabb = aabb.validate()?;
        self.inner_mut()
            .proxies
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        let resource = allocate_resource_token()?;
        let entry = ProxyEntry {
            resource,
            proxy: DynamicTreeProxy {
                aabb,
                category_bits,
                user_data,
            },
        };
        let _call = self.enter_call()?;
        let DynamicTreeInner {
            raw,
            owner,
            proxies,
            poisoned,
            ..
        } = self.inner_mut();
        let baseline_proxy_count = unsafe { ffi::b3DynamicTree_GetProxyCount(raw) };
        if baseline_proxy_count < 0 || baseline_proxy_count as usize != proxies.len() {
            poisoned.set(true);
            return Err(Error::OwnerPoisoned);
        }

        let proxy_id = unsafe {
            ffi::b3DynamicTree_CreateProxy(raw, aabb.into_raw(), category_bits, user_data)
        };
        let observed_proxy_count = unsafe { ffi::b3DynamicTree_GetProxyCount(raw) };
        if proxy_id < 0 {
            if observed_proxy_count == baseline_proxy_count {
                return Err(Error::ObjectIdentityExhausted);
            }
            poisoned.set(true);
            return Err(Error::OwnerPoisoned);
        }
        if proxies.contains_key(&proxy_id) {
            poisoned.set(true);
            return Err(Error::OwnerPoisoned);
        }
        if !proxy_identity_is_compensable(raw, proxy_id) {
            if observed_proxy_count == baseline_proxy_count {
                return Err(Error::ObjectIdentityExhausted);
            }
            poisoned.set(true);
            return Err(Error::OwnerPoisoned);
        }
        if baseline_proxy_count.checked_add(1) != Some(observed_proxy_count) {
            poisoned.set(true);
            return Err(Error::OwnerPoisoned);
        }

        let candidate = ClaimedProxyCreation::new(raw, proxy_id, baseline_proxy_count, poisoned);
        inject_creation_failure(CreationStage::AfterClaim)?;
        let candidate = candidate.bind(entry);
        inject_creation_failure(CreationStage::AfterBind)?;
        if !candidate.verify_postflight(aabb, category_bits, user_data) {
            return Err(Error::NativeFailure);
        }
        inject_creation_failure(CreationStage::AfterPostflight)?;
        inject_creation_failure(CreationStage::BeforePublish)?;
        candidate.commit(proxies, *owner)
    }

    /// Removes a proxy from the tree.
    pub fn destroy_proxy(&mut self, proxy_id: DynamicTreeProxyId) -> Result<()> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _call = self.enter_call()?;
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
        self.check_owner_healthy()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let aabb = aabb.validate()?;
        let _call = self.enter_call()?;
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
        self.check_owner_healthy()?;
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
        let _call = self.enter_call()?;
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
        self.check_owner_healthy()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _call = self.enter_call()?;
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
        self.check_owner_healthy()?;
        let proxy_index = self.proxy_index(proxy_id)?;
        let _call = self.enter_call()?;
        Ok(unsafe { ffi::b3DynamicTree_GetCategoryBits(&mut self.inner_mut().raw, proxy_index) })
    }

    /// Returns the Rust-side snapshot for a live proxy.
    pub fn proxy(&self, proxy_id: DynamicTreeProxyId) -> Result<DynamicTreeProxy> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        Ok(self.proxy_entry(proxy_id)?.proxy)
    }

    /// Returns true when `proxy_id` still refers to a live proxy in this tree.
    ///
    /// Stale and foreign IDs return `Ok(false)`. Callback reentry and terminal
    /// owner poison remain visible as errors.
    pub fn contains_proxy(&self, proxy_id: DynamicTreeProxyId) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        match self.proxy_entry(proxy_id) {
            Ok(_) => Ok(true),
            Err(Error::ForeignHandle { .. } | Error::StaleHandle { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Returns the number of live proxies stored in the native tree.
    pub fn proxy_count(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let _call = self.enter_call()?;
        let count = unsafe { ffi::b3DynamicTree_GetProxyCount(&self.inner().raw) };
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Returns the native heap memory currently owned by the tree, in bytes.
    pub fn byte_count(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let _call = self.enter_call()?;
        let count = unsafe { ffi::b3DynamicTree_GetByteCount(&self.inner().raw) };
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Returns the current height of the native AABB tree.
    pub fn height(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let _call = self.enter_call()?;
        Ok(unsafe { ffi::b3DynamicTree_GetHeight(&self.inner().raw) })
    }

    /// Returns the tree area ratio reported by Box3D.
    pub fn area_ratio(&self) -> Result<f32> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let _call = self.enter_call()?;
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
        self.check_owner_healthy()?;
        if self.inner().proxies.is_empty() {
            return Ok(None);
        }
        let _call = self.enter_call()?;
        let aabb = Aabb::from_raw(unsafe { ffi::b3DynamicTree_GetRootBounds(&self.inner().raw) });
        Ok(Some(aabb.validate().map_err(|_| Error::NativeFailure)?))
    }

    /// Rebuilds the native tree and returns the number of boxes sorted by Box3D.
    pub fn rebuild(&mut self, full_build: bool) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let _call = self.enter_call()?;
        let inner = self.inner_mut();
        let count = unsafe { ffi::b3DynamicTree_Rebuild(&mut inner.raw, full_build) };
        inner.has_enlarged_nodes = false;
        usize::try_from(count).map_err(|_| Error::NativeFailure)
    }

    /// Runs Box3D's internal dynamic-tree validation checks.
    pub fn validate(&self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        let _call = self.enter_call()?;
        unsafe { ffi::b3DynamicTree_Validate(&self.inner().raw) };
        Ok(())
    }

    /// Runs Box3D validation that asserts no enlarged nodes remain.
    ///
    /// This returns [`Error::InvalidValue`] if [`Self::enlarge_proxy`] has been called and the
    /// tree has not subsequently been rebuilt.
    pub fn validate_no_enlarged(&self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        self.check_owner_healthy()?;
        if self.inner().has_enlarged_nodes {
            return Err(validation::invalid(
                "dynamic_tree.enlarged_nodes",
                InvalidValueReason::InvalidCombination,
            ));
        }
        let _call = self.enter_call()?;
        unsafe { ffi::b3DynamicTree_ValidateNoEnlarged(&self.inner().raw) };
        Ok(())
    }

    fn proxy_index(&self, proxy_id: DynamicTreeProxyId) -> Result<i32> {
        let index = proxy_id.into_raw();
        self.proxy_entry(proxy_id)?;
        Ok(index)
    }

    fn check_owner_healthy(&self) -> Result<()> {
        self.inner()
            ._foundation_lease
            .foundation()
            .ensure_healthy()?;
        if self.inner().poisoned.get() {
            Err(Error::OwnerPoisoned)
        } else {
            Ok(())
        }
    }

    fn enter_call(&self) -> Result<callback_state::OwnerCallFrame> {
        self.inner()._foundation_lease.enter_call()
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
        if inner.poisoned.get() {
            return Err(Error::OwnerPoisoned);
        }
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

struct ClaimedProxyCreation<'a> {
    transaction: ProxyCreationTransaction<'a>,
}

impl<'a> ClaimedProxyCreation<'a> {
    fn new(
        tree: *mut ffi::b3DynamicTree,
        proxy_id: i32,
        baseline_proxy_count: i32,
        poisoned: &'a Cell<bool>,
    ) -> Self {
        Self {
            transaction: ProxyCreationTransaction {
                tree,
                proxy_id,
                baseline_proxy_count,
                poisoned,
                armed: true,
                publishing: false,
            },
        }
    }

    fn bind(self, entry: ProxyEntry) -> BoundProxyCreation<'a> {
        BoundProxyCreation {
            transaction: self.transaction,
            entry,
        }
    }
}

struct BoundProxyCreation<'a> {
    transaction: ProxyCreationTransaction<'a>,
    entry: ProxyEntry,
}

impl BoundProxyCreation<'_> {
    fn verify_postflight(&self, aabb: Aabb, category_bits: u64, user_data: u64) -> bool {
        let tree = unsafe { &*self.transaction.tree };
        let expected_proxy_count = self.transaction.baseline_proxy_count.checked_add(1);
        if expected_proxy_count != Some(unsafe { ffi::b3DynamicTree_GetProxyCount(tree) }) {
            return false;
        }

        let Some(node) = trusted_proxy_node(tree, self.transaction.proxy_id) else {
            return false;
        };
        node.categoryBits == category_bits
            && unsafe { node.__bindgen_anon_1.userData } == user_data
            && Aabb::from_raw(node.aabb) == aabb
    }

    fn commit(
        mut self,
        proxies: &mut HashMap<i32, ProxyEntry>,
        owner: OwnerToken,
    ) -> Result<DynamicTreeProxyId> {
        let proxy_id = self.transaction.proxy_id;
        let resource = self.entry.resource;
        self.transaction.publishing = true;
        match proxies.entry(proxy_id) {
            Entry::Vacant(slot) => {
                slot.insert(self.entry);
            }
            Entry::Occupied(_) => {
                self.transaction.poisoned.set(true);
                return Err(Error::OwnerPoisoned);
            }
        }
        inject_creation_failure(CreationStage::BeforeCommitDisarm)?;
        self.transaction.armed = false;
        self.transaction.publishing = false;
        Ok(DynamicTreeProxyId::new(proxy_id, owner, resource))
    }
}

struct ProxyCreationTransaction<'a> {
    tree: *mut ffi::b3DynamicTree,
    proxy_id: i32,
    baseline_proxy_count: i32,
    poisoned: &'a Cell<bool>,
    armed: bool,
    publishing: bool,
}

impl Drop for ProxyCreationTransaction<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if self.publishing {
            self.poisoned.set(true);
        }

        let tree = unsafe { &mut *self.tree };
        unsafe { ffi::b3DynamicTree_DestroyProxy(tree, self.proxy_id) };
        let forced_mismatch = force_compensation_mismatch();
        if !proxy_compensation_is_verified(tree, self.proxy_id, self.baseline_proxy_count)
            || forced_mismatch
        {
            self.poisoned.set(true);
        }
    }
}

fn proxy_compensation_is_verified(
    tree: &ffi::b3DynamicTree,
    proxy_id: i32,
    baseline_proxy_count: i32,
) -> bool {
    (unsafe { ffi::b3DynamicTree_GetProxyCount(tree) }) == baseline_proxy_count
        && trusted_proxy_node(tree, proxy_id).is_none()
}

fn proxy_identity_is_compensable(tree: &ffi::b3DynamicTree, proxy_id: i32) -> bool {
    trusted_proxy_node(tree, proxy_id).is_some()
}

fn trusted_proxy_node(tree: &ffi::b3DynamicTree, proxy_id: i32) -> Option<&ffi::b3TreeNode> {
    if proxy_id < 0 || proxy_id >= tree.nodeCapacity || tree.nodes.is_null() {
        return None;
    }
    let node = unsafe { &*tree.nodes.add(proxy_id as usize) };
    let required_flags =
        (ffi::b3TreeNodeFlags_b3_allocatedNode | ffi::b3TreeNodeFlags_b3_leafNode) as u16;
    (node.flags & required_flags == required_flags && node.height == 0).then_some(node)
}

impl DynamicTreeInner {
    fn destroy(self) {
        let mut owner = callback_state::RetainOnUnwind::new(self);
        if !owner.raw.nodes.is_null() {
            unsafe { ffi::b3DynamicTree_Destroy(&mut owner.raw) };
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
    use crate::world::creation_transaction::{
        force_creation_failure, force_next_compensation_mismatch,
    };

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

    #[test]
    fn creation_transaction_proxy_compensates_every_fallible_stage() -> Result<()> {
        crate::Foundation::initialize_default()?;
        let stages = [
            CreationStage::AfterClaim,
            CreationStage::AfterBind,
            CreationStage::AfterPostflight,
            CreationStage::BeforePublish,
        ];

        for (index, stage) in stages.into_iter().enumerate() {
            let mut tree = DynamicTree::new()?;
            let retained = tree.create_proxy(aabb(-1.0, 1.0), index as u64)?;
            force_creation_failure(stage);

            assert_eq!(
                tree.create_proxy_with_category_bits(
                    aabb(2.0, 3.0),
                    1_u64 << index,
                    100 + index as u64,
                ),
                Err(Error::NativeFailure)
            );
            assert_eq!(tree.proxy_count()?, 1);
            assert_eq!(tree.inner().proxies.len(), 1);
            assert_eq!(tree.contains_proxy(retained), Ok(true));

            let replacement = tree.create_proxy(aabb(2.0, 3.0), 200 + index as u64)?;
            assert_eq!(tree.proxy_count()?, 2);
            assert_eq!(tree.contains_proxy(replacement), Ok(true));
        }
        Ok(())
    }

    #[test]
    fn creation_transaction_commit_disarm_fault_poisons_published_tree() -> Result<()> {
        crate::Foundation::initialize_default()?;
        let mut tree = DynamicTree::new()?;
        force_creation_failure(CreationStage::BeforeCommitDisarm);
        assert_eq!(
            tree.create_proxy(aabb(-1.0, 1.0), 1),
            Err(Error::NativeFailure)
        );
        assert!(tree.inner().poisoned.get());
        assert_eq!(tree.inner().proxies.len(), 1);
        assert_eq!(
            unsafe { ffi::b3DynamicTree_GetProxyCount(&tree.inner().raw) },
            0
        );
        assert_eq!(tree.proxy_count(), Err(Error::OwnerPoisoned));
        drop(tree);
        Ok(())
    }

    #[test]
    fn creation_transaction_mismatch_poisons_tree_but_drop_remains_finite() -> Result<()> {
        crate::Foundation::initialize_default()?;
        let mut tree = DynamicTree::new()?;
        let retained = tree.create_proxy(aabb(-1.0, 1.0), 1)?;
        force_creation_failure(CreationStage::AfterClaim);
        force_next_compensation_mismatch();

        assert_eq!(
            tree.create_proxy(aabb(2.0, 3.0), 2),
            Err(Error::NativeFailure)
        );
        assert!(tree.inner().poisoned.get());
        assert_eq!(tree.proxy_count(), Err(Error::OwnerPoisoned));
        assert_eq!(tree.proxy(retained), Err(Error::OwnerPoisoned));
        assert_eq!(tree.contains_proxy(retained), Err(Error::OwnerPoisoned));
        assert_eq!(tree.inner().proxies.len(), 1);

        drop(tree);
        Ok(())
    }

    #[test]
    fn creation_transaction_rejects_count_only_compensation_proof() -> Result<()> {
        crate::Foundation::initialize_default()?;
        let mut tree = DynamicTree::new()?;
        let candidate = tree.create_proxy(aabb(-1.0, 1.0), 1)?;
        let baseline = unsafe { ffi::b3DynamicTree_GetProxyCount(&tree.inner().raw) };

        assert_eq!(baseline, 1);
        assert!(trusted_proxy_node(&tree.inner().raw, candidate.into_raw()).is_some());
        assert!(!proxy_compensation_is_verified(
            &tree.inner().raw,
            candidate.into_raw(),
            baseline,
        ));
        Ok(())
    }
}
