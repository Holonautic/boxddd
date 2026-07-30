use crate::core::callback_state;
use crate::core::foundation::{Foundation, ReplayLease, TransientLease};
use crate::error::Result;
use std::ops::Deref;
use std::sync::{Mutex, MutexGuard, OnceLock};

static BOX3D_GLOBAL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Transitional U1 admission guard removed with the global operation lock in U5.
pub(crate) struct Box3dLockGuard<'a> {
    mutex: MutexGuard<'static, ()>,
    _owner_call_frame: callback_state::OwnerCallFrame,
    _activity: Option<TransientLease>,
    _replay: Option<&'a ReplayLease>,
}

impl Deref for Box3dLockGuard<'_> {
    type Target = MutexGuard<'static, ()>;

    fn deref(&self) -> &Self::Target {
        &self.mutex
    }
}

pub(crate) fn lock() -> Result<Box3dLockGuard<'static>> {
    callback_state::check_not_in_callback()?;
    let activity = Foundation::get()?.acquire_transient()?;
    let owner_call_frame = callback_state::OwnerCallFrame::enter();
    let mutex = lock_mutex();
    Ok(Box3dLockGuard {
        mutex,
        _owner_call_frame: owner_call_frame,
        _activity: Some(activity),
        _replay: None,
    })
}

/// Acquires the transitional operation mutex under an existing exclusive replay lease.
pub(crate) fn lock_replay(lease: &ReplayLease) -> Result<Box3dLockGuard<'_>> {
    callback_state::check_not_in_callback()?;
    lease.foundation().ensure_healthy()?;
    let owner_call_frame = callback_state::OwnerCallFrame::enter();
    let mutex = lock_mutex();
    Ok(Box3dLockGuard {
        mutex,
        _owner_call_frame: owner_call_frame,
        _activity: None,
        _replay: Some(lease),
    })
}

/// Acquires the transitional mutex while Foundation is still being established.
pub(crate) fn lock_for_initialization() -> MutexGuard<'static, ()> {
    lock_mutex()
}

/// Acquires the transitional mutex for infallible owner cleanup.
pub(crate) fn lock_for_cleanup() -> MutexGuard<'static, ()> {
    lock_mutex()
}

/// Acquires the transitional operation mutex while an exclusive replay owner is being destroyed.
pub(crate) struct ReplayCleanupGuard<'a> {
    mutex: MutexGuard<'static, ()>,
    _lease: &'a ReplayLease,
}

impl Deref for ReplayCleanupGuard<'_> {
    type Target = MutexGuard<'static, ()>;

    fn deref(&self) -> &Self::Target {
        &self.mutex
    }
}

pub(crate) fn lock_replay_for_cleanup(lease: &ReplayLease) -> ReplayCleanupGuard<'_> {
    ReplayCleanupGuard {
        mutex: lock_mutex(),
        _lease: lease,
    }
}

fn lock_mutex() -> MutexGuard<'static, ()> {
    BOX3D_GLOBAL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
