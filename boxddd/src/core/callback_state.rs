use crate::core::provenance::ResourceToken;
#[cfg(not(all(target_arch = "wasm32", boxddd_wasm_provider)))]
use crate::core::provenance::allocate_resource_token;
use crate::error::{Error, Result};
use std::cell::Cell;
use std::ops::Deref;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
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
}

impl CallbackFailure {
    fn into_error(self) -> Error {
        match self {
            Self::Panicked => Error::CallbackPanicked,
            Self::InvalidHandle | Self::InvalidNativeInput => Error::NativeFailure,
            Self::Provider => Error::ProviderCallbackFailed,
        }
    }

    fn from_raw(value: u8) -> Option<Self> {
        match value {
            value if value == Self::Panicked as u8 => Some(Self::Panicked),
            value if value == Self::InvalidHandle as u8 => Some(Self::InvalidHandle),
            value if value == Self::InvalidNativeInput as u8 => Some(Self::InvalidNativeInput),
            value if value == Self::Provider as u8 => Some(Self::Provider),
            _ => None,
        }
    }
}

const NO_SHARED_FAILURE: u8 = 0;

#[derive(Clone, Debug, Default)]
pub(crate) struct SharedCallbackState {
    failure: Arc<AtomicU8>,
}

impl SharedCallbackState {
    #[inline]
    pub(crate) fn reset(&self) {
        self.failure.store(NO_SHARED_FAILURE, Ordering::Release);
    }

    #[inline]
    pub(crate) fn has_failed(&self) -> bool {
        self.failure.load(Ordering::Acquire) != NO_SHARED_FAILURE
    }

    pub(crate) fn record(&self, failure: CallbackFailure) {
        let _ = self.failure.compare_exchange(
            NO_SHARED_FAILURE,
            failure as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn invoke<R: Copy>(&self, fallback: R, callback: impl FnOnce() -> R) -> R {
        match catch_unwind(AssertUnwindSafe(|| {
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
        let failure = self.failure.swap(NO_SHARED_FAILURE, Ordering::AcqRel);
        match CallbackFailure::from_raw(failure) {
            Some(failure) => Err(failure.into_error()),
            None => Ok(()),
        }
    }

    pub(crate) fn result(&self) -> Result<()> {
        match CallbackFailure::from_raw(self.failure.load(Ordering::Acquire)) {
            Some(failure) => Err(failure.into_error()),
            None => Ok(()),
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
    pub(crate) fn publish(&self, pending: PendingCallback<T>) {
        let mut registration = self.write();
        registration.token = Some(pending.token);
        registration.callback = Some(pending.callback);
    }

    pub(crate) fn retire(&self) {
        let mut registration = self.write();
        registration.token = None;
        registration.callback = None;
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
    fn registered_callback_snapshots_release_the_registry_lock() {
        type Callback = dyn Fn() -> usize + Send + Sync;
        let registration = RegisteredCallback::<Callback>::default();
        let pending = PendingCallback::new(Arc::new(|| 7_usize) as Arc<Callback>).unwrap();
        registration.publish(pending);

        let old = registration.snapshot().unwrap();
        assert_eq!(old(), 7);
        let replacement = PendingCallback::new(Arc::new(|| 11_usize) as Arc<Callback>).unwrap();
        registration.publish(replacement);

        assert!(!registration.is_current(&old));
        assert_eq!(registration.snapshot().unwrap()(), 11);
        registration.retire();
        assert!(registration.snapshot().is_none());
    }
}
