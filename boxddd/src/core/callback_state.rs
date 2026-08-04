use crate::core::provenance::ResourceToken;
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::core::provenance::allocate_resource_token;
use crate::error::{Error, Result};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static OWNER_CALL_FRAMES: RefCell<OwnerCallStack> = const {
        RefCell::new(OwnerCallStack::new())
    };
    static SHARED_INVOCATIONS: RefCell<Vec<Weak<SharedCallbackInner>>> = const {
        RefCell::new(Vec::new())
    };
}

type LocalCleanup = Box<dyn FnOnce() + 'static>;
type SendCleanup = Box<dyn FnOnce() + Send + 'static>;

/// Keeps a complete cleanup owner intact until its cleanup path explicitly finishes.
pub(crate) struct RetainOnUnwind<T> {
    value: Option<T>,
}

impl<T> RetainOnUnwind<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self { value: Some(value) }
    }

    pub(crate) fn finish(mut self) {
        drop(
            self.value
                .take()
                .expect("cleanup owner finishes exactly once"),
        );
    }
}

impl<T> Deref for RetainOnUnwind<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value
            .as_ref()
            .expect("cleanup owner is present until wrapper destruction")
    }
}

impl<T> DerefMut for RetainOnUnwind<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value
            .as_mut()
            .expect("cleanup owner is present until wrapper destruction")
    }
}

impl<T> Drop for RetainOnUnwind<T> {
    fn drop(&mut self) {
        let Some(value) = self.value.take() else {
            return;
        };
        std::mem::forget(value);
    }
}

#[derive(Default)]
struct LocalCleanupQueue {
    pending: VecDeque<LocalCleanup>,
}

struct OwnerCallStack {
    active: usize,
    frames: Vec<LocalCleanupQueue>,
}

impl OwnerCallStack {
    const fn new() -> Self {
        Self {
            active: 0,
            frames: Vec::new(),
        }
    }
}

/// Outermost safe-call boundary for callback-time owner destruction.
///
/// Frames are thread-local because the complete owners retained here may be
/// intentionally `!Send`. Nested frames append to their parent; only the
/// outermost frame executes cleanup.
pub(crate) struct OwnerCallFrame {
    index: usize,
    _not_send: PhantomData<Rc<()>>,
}

impl OwnerCallFrame {
    pub(crate) fn enter() -> Self {
        let index = OWNER_CALL_FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            let index = frames.active;
            if index == frames.frames.len() {
                frames.frames.push(LocalCleanupQueue::default());
            } else {
                debug_assert!(frames.frames[index].pending.is_empty());
            }
            frames.active += 1;
            index
        });
        Self {
            index,
            _not_send: PhantomData,
        }
    }
}

impl Drop for OwnerCallFrame {
    fn drop(&mut self) {
        let merged_into_parent = OWNER_CALL_FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            debug_assert_eq!(frames.active, self.index + 1);
            if self.index == 0 {
                return false;
            }
            let (parents, current) = frames.frames.split_at_mut(self.index);
            parents[self.index - 1]
                .pending
                .append(&mut current[0].pending);
            frames.active -= 1;
            true
        });

        if merged_into_parent {
            return;
        }

        loop {
            let cleanup = OWNER_CALL_FRAMES.with(|frames| {
                let mut frames = frames.borrow_mut();
                debug_assert_eq!(frames.active, 1);
                frames.frames[0].pending.pop_front()
            });
            let Some(cleanup) = cleanup else {
                break;
            };
            if catch_unwind(AssertUnwindSafe(cleanup)).is_err()
                && let Ok(foundation) = crate::core::foundation::Foundation::get()
            {
                foundation.poison();
            }
        }

        OWNER_CALL_FRAMES.with(|frames| {
            let mut frames = frames.borrow_mut();
            debug_assert_eq!(frames.active, 1);
            debug_assert!(frames.frames[0].pending.is_empty());
            frames.active = 0;
        });
    }
}

/// Transfers a complete owner cleanup to the current thread's call frame.
///
/// The caller retains ownership when no frame exists, so it can deliberately
/// retain the complete capsule instead of running callback-time destruction.
pub(crate) fn try_defer_local_cleanup<C>(cleanup: C) -> std::result::Result<(), C>
where
    C: FnOnce() + 'static,
{
    let mut cleanup = Some(cleanup);
    OWNER_CALL_FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        let active = frames.active;
        if active > 0 {
            frames.frames[active - 1].pending.push_back(Box::new(
                cleanup.take().expect("cleanup is queued at most once"),
            ));
        }
    });
    match cleanup {
        None => Ok(()),
        Some(cleanup) => Err(cleanup),
    }
}

/// Defers cleanup when possible and otherwise intentionally retains it whole.
pub(crate) fn defer_local_cleanup_or_retain<C>(cleanup: C)
where
    C: FnOnce() + 'static,
{
    if let Err(cleanup) = try_defer_local_cleanup(cleanup) {
        std::mem::forget(cleanup);
    }
}

pub struct CallbackGuard;

impl CallbackGuard {
    pub fn enter() -> Self {
        DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        Self
    }
}

impl Drop for CallbackGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

#[inline]
pub(crate) fn in_callback() -> bool {
    DEPTH.with(|depth| depth.get() > 0)
}

#[inline]
pub(crate) fn check_not_in_callback() -> crate::error::Result<()> {
    if in_callback() {
        Err(crate::error::Error::InCallback)
    } else {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct LocalCallbackState {
    failure: Option<Error>,
}

impl LocalCallbackState {
    pub(crate) const fn new() -> Self {
        Self { failure: None }
    }

    #[inline]
    pub(crate) const fn has_failed(&self) -> bool {
        self.failure.is_some()
    }

    pub(crate) fn fail<R>(&mut self, error: Error, fallback: R) -> R {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        fallback
    }

    pub(crate) fn invoke<R: Copy>(&mut self, fallback: R, callback: impl FnOnce() -> R) -> R {
        if self.has_failed() {
            return fallback;
        }
        match catch_unwind(AssertUnwindSafe(|| {
            let _guard = CallbackGuard::enter();
            callback()
        })) {
            Ok(value) => value,
            Err(_) => self.fail(Error::CallbackPanicked, fallback),
        }
    }

    pub(crate) fn drain(&mut self) -> Result<()> {
        match self.failure.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum CallbackFailure {
    Panicked = 1,
    InvalidHandle = 2,
    InvalidNativeInput = 3,
    Provider = 4,
    DeferredCleanup = 5,
}

impl CallbackFailure {
    fn into_error(self) -> Error {
        match self {
            Self::Panicked => Error::CallbackPanicked,
            Self::InvalidHandle | Self::InvalidNativeInput => Error::NativeFailure,
            Self::Provider => Error::ProviderCallbackFailed,
            Self::DeferredCleanup => Error::OwnerPoisoned,
        }
    }

    fn from_raw(value: u8) -> Option<Self> {
        match value {
            value if value == Self::Panicked as u8 => Some(Self::Panicked),
            value if value == Self::InvalidHandle as u8 => Some(Self::InvalidHandle),
            value if value == Self::InvalidNativeInput as u8 => Some(Self::InvalidNativeInput),
            value if value == Self::Provider as u8 => Some(Self::Provider),
            value if value == Self::DeferredCleanup as u8 => Some(Self::DeferredCleanup),
            _ => None,
        }
    }
}

const NO_SHARED_FAILURE: u8 = 0;

#[derive(Clone)]
pub(crate) struct SharedCallbackState {
    inner: Arc<SharedCallbackInner>,
}

struct SharedCallbackInner {
    failure: AtomicU8,
    cleanup: Mutex<SharedCleanupQueue>,
}

#[derive(Default)]
struct SharedCleanupQueue {
    phase: SharedCleanupPhase,
    pending: VecDeque<SendCleanup>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum SharedCleanupPhase {
    #[default]
    Open,
    Closing,
    Closed,
}

impl Default for SharedCallbackState {
    fn default() -> Self {
        Self {
            inner: Arc::new(SharedCallbackInner {
                failure: AtomicU8::new(NO_SHARED_FAILURE),
                cleanup: Mutex::new(SharedCleanupQueue::default()),
            }),
        }
    }
}

impl fmt::Debug for SharedCallbackState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cleanup = self
            .inner
            .cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        formatter
            .debug_struct("SharedCallbackState")
            .field("failure", &self.inner.failure.load(Ordering::Acquire))
            .field("cleanup_phase", &cleanup.phase)
            .field("pending_cleanup", &cleanup.pending.len())
            .finish()
    }
}

impl Drop for SharedCallbackInner {
    fn drop(&mut self) {
        let queue = self
            .cleanup
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.phase != SharedCleanupPhase::Closed {
            for cleanup in queue.pending.drain(..) {
                std::mem::forget(cleanup);
            }
        }
    }
}

struct SharedInvocationGuard {
    inner: *const SharedCallbackInner,
}

impl SharedInvocationGuard {
    fn enter(inner: &Arc<SharedCallbackInner>) -> Self {
        SHARED_INVOCATIONS.with(|invocations| {
            invocations.borrow_mut().push(Arc::downgrade(inner));
        });
        Self {
            inner: Arc::as_ptr(inner),
        }
    }
}

impl Drop for SharedInvocationGuard {
    fn drop(&mut self) {
        SHARED_INVOCATIONS.with(|invocations| {
            let popped = invocations.borrow_mut().pop();
            debug_assert!(
                popped.is_some_and(|inner| std::ptr::eq(Weak::as_ptr(&inner), self.inner))
            );
        });
    }
}

impl SharedCallbackState {
    #[inline]
    pub(crate) fn has_failed(&self) -> bool {
        self.inner.failure.load(Ordering::Acquire) != NO_SHARED_FAILURE
    }

    pub(crate) fn record(&self, failure: CallbackFailure) {
        let _ = self.inner.failure.compare_exchange(
            NO_SHARED_FAILURE,
            failure as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn invoke<R: Copy>(&self, fallback: R, callback: impl FnOnce() -> R) -> R {
        match catch_unwind(AssertUnwindSafe(|| {
            let _invocation = SharedInvocationGuard::enter(&self.inner);
            let _guard = CallbackGuard::enter();
            callback()
        })) {
            Ok(value) => value,
            Err(_) => {
                self.record(CallbackFailure::Panicked);
                fallback
            }
        }
    }

    pub(crate) fn invoke_while_clear<R: Copy>(
        &self,
        fallback: R,
        callback: impl FnOnce() -> R,
    ) -> R {
        if self.has_failed() {
            fallback
        } else {
            self.invoke(fallback, callback)
        }
    }

    pub(crate) fn drain(&self) -> Result<()> {
        let failure = self.inner.failure.swap(NO_SHARED_FAILURE, Ordering::AcqRel);
        match CallbackFailure::from_raw(failure) {
            Some(failure) => Err(failure.into_error()),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    pub(crate) fn result(&self) -> Result<()> {
        match CallbackFailure::from_raw(self.inner.failure.load(Ordering::Acquire)) {
            Some(failure) => Err(failure.into_error()),
            None => Ok(()),
        }
    }

    #[inline]
    fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Closes worker submission and drains every accepted cleanup exactly once.
    pub(crate) fn close_and_drain_cleanup(&self) {
        let _owner_call_frame = OwnerCallFrame::enter();
        {
            let mut queue = self
                .inner
                .cleanup
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if queue.phase == SharedCleanupPhase::Closed {
                return;
            }
            queue.phase = SharedCleanupPhase::Closing;
        }

        loop {
            let cleanup = {
                self.inner
                    .cleanup
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pending
                    .pop_front()
            };
            let Some(cleanup) = cleanup else {
                break;
            };
            if catch_unwind(AssertUnwindSafe(cleanup)).is_err() {
                self.record(CallbackFailure::DeferredCleanup);
                if let Ok(foundation) = crate::core::foundation::Foundation::get() {
                    foundation.poison();
                }
            }
        }

        self.inner
            .cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .phase = SharedCleanupPhase::Closed;
    }
}

/// Transfers a complete `Send` owner cleanup to the active callback boundary.
///
/// Owner-thread frames are preferred so main-thread callbacks preserve local
/// FIFO ordering. The cleanup is returned unchanged if no accepting boundary
/// exists.
#[allow(dead_code)]
pub(crate) fn try_defer_send_cleanup<C>(cleanup: C) -> std::result::Result<(), C>
where
    C: FnOnce() + Send + 'static,
{
    let cleanup = match try_defer_local_cleanup(cleanup) {
        Ok(()) => return Ok(()),
        Err(cleanup) => cleanup,
    };
    let mut cleanup = Some(cleanup);
    SHARED_INVOCATIONS.with(|invocations| {
        let Some(inner) = invocations.borrow().iter().rev().find_map(Weak::upgrade) else {
            return;
        };
        let mut queue = inner
            .cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.phase == SharedCleanupPhase::Open {
            queue.pending.push_back(Box::new(
                cleanup.take().expect("cleanup is queued at most once"),
            ));
        }
    });
    match cleanup {
        None => Ok(()),
        Some(cleanup) => Err(cleanup),
    }
}

/// World-owned installation point for the failure state of one native invocation.
///
/// Adapters snapshot the installed state before calling user code, so the slot's
/// lock is never held across a callback or a blocking task join.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallbackInvocationSlot {
    state: Arc<RwLock<Option<SharedCallbackState>>>,
}

impl CallbackInvocationSlot {
    pub(crate) fn install(&self, state: SharedCallbackState) -> Result<CallbackInvocationGuard> {
        let mut installed = self.write();
        if installed.is_some() {
            return Err(Error::NativeFailure);
        }
        *installed = Some(state.clone());
        drop(installed);
        Ok(CallbackInvocationGuard {
            slot: self.clone(),
            state,
        })
    }

    #[inline]
    pub(crate) fn current(&self) -> Option<SharedCallbackState> {
        self.read().clone()
    }

    pub(crate) fn record(&self, failure: CallbackFailure) {
        if let Some(state) = self.current() {
            state.record(failure);
        }
    }

    pub(crate) fn invoke_while_clear<R: Copy>(
        &self,
        fallback: R,
        callback: impl FnOnce() -> R,
    ) -> R {
        self.current().map_or(fallback, |state| {
            state.invoke_while_clear(fallback, callback)
        })
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Option<SharedCallbackState>> {
        self.state.read().unwrap_or_else(|error| error.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Option<SharedCallbackState>> {
        self.state
            .write()
            .unwrap_or_else(|error| error.into_inner())
    }
}

#[derive(Debug)]
pub(crate) struct CallbackInvocationGuard {
    slot: CallbackInvocationSlot,
    state: SharedCallbackState,
}

impl Drop for CallbackInvocationGuard {
    fn drop(&mut self) {
        let mut installed = self.slot.write();
        if installed
            .as_ref()
            .is_some_and(|state| state.same_instance(&self.state))
        {
            *installed = None;
        }
    }
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
pub(crate) struct PendingCallback<T: ?Sized> {
    token: ResourceToken,
    callback: Arc<T>,
}

#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
impl<T: ?Sized> PendingCallback<T> {
    pub(crate) fn new(callback: Arc<T>) -> Result<Self> {
        Ok(Self {
            token: allocate_resource_token()?,
            callback,
        })
    }
}

struct Registration<T: ?Sized> {
    token: Option<ResourceToken>,
    callback: Option<Arc<T>>,
}

pub(crate) struct RegisteredCallback<T: ?Sized> {
    registration: RwLock<Registration<T>>,
}

impl<T: ?Sized> Default for RegisteredCallback<T> {
    fn default() -> Self {
        Self {
            registration: RwLock::new(Registration {
                token: None,
                callback: None,
            }),
        }
    }
}

impl<T: ?Sized> RegisteredCallback<T> {
    #[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
    pub(crate) fn publish(&self, pending: PendingCallback<T>) -> Option<Arc<T>> {
        let mut registration = self.write();
        registration.token = Some(pending.token);
        registration.callback.replace(pending.callback)
    }

    pub(crate) fn retire(&self) -> Option<Arc<T>> {
        let mut registration = self.write();
        registration.token = None;
        registration.callback.take()
    }

    pub(crate) fn snapshot(&self) -> Option<CallbackSnapshot<T>> {
        let registration = self.read();
        Some(CallbackSnapshot {
            token: registration.token?,
            callback: Arc::clone(registration.callback.as_ref()?),
        })
    }

    pub(crate) fn is_current(&self, snapshot: &CallbackSnapshot<T>) -> bool {
        self.read().token == Some(snapshot.token)
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Registration<T>> {
        self.registration
            .read()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Registration<T>> {
        self.registration
            .write()
            .unwrap_or_else(|error| error.into_inner())
    }
}

pub(crate) fn release_callback_capture<T: ?Sized>(callback: Option<Arc<T>>) -> Result<()> {
    catch_unwind(AssertUnwindSafe(|| drop(callback))).map_err(|_| Error::CallbackPanicked)
}

pub(crate) struct CallbackSnapshot<T: ?Sized> {
    token: ResourceToken,
    callback: Arc<T>,
}

impl<T: ?Sized> Deref for CallbackSnapshot<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.callback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_not_impl_any;
    use std::cell::Cell;
    use std::sync::{Barrier, Mutex};
    use std::thread;

    assert_not_impl_any!(OwnerCallFrame: Send, Sync);

    #[test]
    fn local_state_preserves_first_failure_and_drains_once() {
        let mut state = LocalCallbackState::new();

        assert!(!state.has_failed());
        assert!(!state.fail(Error::NativeFailure, false));
        assert!(!state.fail(Error::CallbackPanicked, false));
        assert_eq!(state.drain(), Err(Error::NativeFailure));
        assert_eq!(state.drain(), Ok(()));
    }

    #[test]
    fn cleanup_owner_releases_only_after_explicit_finish() {
        struct DropProbe(Rc<Cell<usize>>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let normal_drops = Rc::new(Cell::new(0));
        RetainOnUnwind::new(DropProbe(Rc::clone(&normal_drops))).finish();
        assert_eq!(normal_drops.get(), 1);

        let unfinished_drops = Rc::new(Cell::new(0));
        drop(RetainOnUnwind::new(DropProbe(Rc::clone(&unfinished_drops))));
        assert_eq!(unfinished_drops.get(), 0);

        let unwind_drops = Rc::new(Cell::new(0));
        let unwind_probe = Rc::clone(&unwind_drops);
        assert!(
            catch_unwind(AssertUnwindSafe(move || {
                let _owner = RetainOnUnwind::new(DropProbe(unwind_probe));
                panic!("injected cleanup unwind");
            }))
            .is_err()
        );
        assert_eq!(unwind_drops.get(), 0);

        struct FinishDuringDrop<T>(Option<RetainOnUnwind<T>>);

        impl<T> Drop for FinishDuringDrop<T> {
            fn drop(&mut self) {
                self.0.take().expect("test cleanup finishes once").finish();
            }
        }

        let ambient_unwind_drops = Rc::new(Cell::new(0));
        let ambient_probe = Rc::clone(&ambient_unwind_drops);
        assert!(
            catch_unwind(AssertUnwindSafe(move || {
                let _cleanup =
                    FinishDuringDrop(Some(RetainOnUnwind::new(DropProbe(ambient_probe))));
                panic!("injected outer unwind");
            }))
            .is_err()
        );
        assert_eq!(ambient_unwind_drops.get(), 1);
    }

    #[test]
    fn local_state_contains_panics_and_enters_callback_scope() {
        let mut state = LocalCallbackState::new();
        let in_scope = state.invoke(false, in_callback);
        assert!(in_scope);

        assert!(!state.invoke(false, || panic!("injected local callback panic")));
        assert_eq!(state.drain(), Err(Error::CallbackPanicked));
    }

    #[test]
    fn shared_state_preserves_first_failure_across_clones() {
        let state = SharedCallbackState::default();
        let clone = state.clone();

        clone.record(CallbackFailure::InvalidHandle);
        state.record(CallbackFailure::Panicked);
        assert_eq!(state.drain(), Err(Error::NativeFailure));
        assert_eq!(clone.drain(), Ok(()));
    }

    #[test]
    fn shared_state_can_run_required_cleanup_after_failure() {
        let state = SharedCallbackState::default();
        state.record(CallbackFailure::Panicked);
        let mut ran = false;

        state.invoke((), || ran = true);

        assert!(ran);
        assert_eq!(state.drain(), Err(Error::CallbackPanicked));
    }

    #[test]
    fn invocation_slot_installs_one_state_and_detaches_with_its_guard() {
        let slot = CallbackInvocationSlot::default();
        let state = SharedCallbackState::default();
        let guard = slot.install(state.clone()).unwrap();

        slot.record(CallbackFailure::InvalidHandle);
        assert_eq!(state.result(), Err(Error::NativeFailure));
        assert_eq!(
            slot.install(SharedCallbackState::default()).unwrap_err(),
            Error::NativeFailure
        );

        drop(guard);
        assert!(slot.current().is_none());
        assert!(slot.install(SharedCallbackState::default()).is_ok());
    }

    #[test]
    fn shared_state_chooses_one_stable_winner_under_a_real_thread_race() {
        let state = SharedCallbackState::default();
        let barrier = Arc::new(Barrier::new(3));
        let panicked = {
            let state = state.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                state.record(CallbackFailure::Panicked);
            })
        };
        let provider = {
            let state = state.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                state.record(CallbackFailure::Provider);
            })
        };

        barrier.wait();
        panicked.join().unwrap();
        provider.join().unwrap();

        let winner = state.result();
        assert!(matches!(
            &winner,
            Err(Error::CallbackPanicked | Error::ProviderCallbackFailed)
        ));
        assert_eq!(state.result(), winner);
    }

    #[test]
    fn nested_owner_frames_merge_fifo_and_drain_to_a_fixed_point() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let outer = OwnerCallFrame::enter();
        let outer_events = Rc::clone(&events);
        assert!(
            try_defer_local_cleanup(move || {
                outer_events.borrow_mut().push("outer");
                let follow_up_events = Rc::clone(&outer_events);
                assert!(
                    try_defer_local_cleanup(move || {
                        follow_up_events.borrow_mut().push("follow-up");
                    })
                    .is_ok()
                );
            })
            .is_ok()
        );

        {
            let _inner = OwnerCallFrame::enter();
            let inner_events = Rc::clone(&events);
            assert!(
                try_defer_local_cleanup(move || {
                    inner_events.borrow_mut().push("inner");
                })
                .is_ok()
            );
        }
        assert!(events.borrow().is_empty());

        drop(outer);
        assert_eq!(&*events.borrow(), &["outer", "inner", "follow-up"]);
    }

    #[test]
    fn cleanup_without_a_local_frame_is_retained_whole() {
        let dropped = Rc::new(Cell::new(false));
        let captured = Rc::clone(&dropped);

        defer_local_cleanup_or_retain(move || captured.set(true));

        assert!(!dropped.get());
    }

    #[test]
    fn shared_cleanup_waits_for_close_and_runs_after_worker_join() {
        let state = SharedCallbackState::default();
        let events = Arc::new(Mutex::new(Vec::new()));
        let worker = {
            let state = state.clone();
            let events = Arc::clone(&events);
            thread::spawn(move || {
                state.invoke((), || {
                    assert!(
                        try_defer_send_cleanup(move || {
                            events.lock().unwrap().push("worker");
                        })
                        .is_ok()
                    );
                });
            })
        };

        worker.join().unwrap();
        assert!(events.lock().unwrap().is_empty());
        state.close_and_drain_cleanup();
        assert_eq!(&*events.lock().unwrap(), &["worker"]);
        state.close_and_drain_cleanup();
        assert_eq!(&*events.lock().unwrap(), &["worker"]);
    }

    #[test]
    fn closed_shared_cleanup_returns_late_ownership() {
        let state = SharedCallbackState::default();
        state.close_and_drain_cleanup();
        let ran = Arc::new(AtomicU8::new(0));
        let captured = Arc::clone(&ran);

        state.invoke((), || {
            assert!(
                try_defer_send_cleanup(move || {
                    captured.store(1, Ordering::Release);
                })
                .is_err()
            );
        });

        assert_eq!(ran.load(Ordering::Acquire), 0);
    }

    #[test]
    fn registered_callback_snapshots_release_the_registry_lock() {
        type Callback = dyn Fn() -> usize + Send + Sync;
        let registration = RegisteredCallback::<Callback>::default();
        let pending = PendingCallback::new(Arc::new(|| 7_usize) as Arc<Callback>).unwrap();
        assert!(registration.publish(pending).is_none());

        let old = registration.snapshot().unwrap();
        assert_eq!(old(), 7);
        let replacement = PendingCallback::new(Arc::new(|| 11_usize) as Arc<Callback>).unwrap();
        let previous = registration.publish(replacement);
        release_callback_capture(previous).unwrap();

        assert!(!registration.is_current(&old));
        assert_eq!(registration.snapshot().unwrap()(), 11);
        let previous = registration.retire();
        release_callback_capture(previous).unwrap();
        assert!(registration.snapshot().is_none());
    }
}
