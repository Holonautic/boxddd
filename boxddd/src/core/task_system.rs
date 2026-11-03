use crate::core::callback_state::{CallbackFailure, CallbackInvocationSlot, SharedCallbackState};
use crate::error::Error;
use boxddd_sys::ffi;
use std::os::raw::{c_char, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Safe task-system adapter installed on a Box3D world definition.
///
/// Box3D calls `enqueueTask` during `b3World_Step` and later calls `finishTask`
/// for every non-null task handle returned by enqueue. `finishTask` must block
/// until that task has completed; schedulers that cannot provide this blocking
/// guarantee must not be installed through this adapter. Do not call
/// `World::step` from a job system worker that cannot park or join child
/// work; that scheduler shape can deadlock when Box3D asks `finishTask` to wait.
#[derive(Clone, Debug)]
pub struct TaskSystem {
    inner: Arc<TaskSystemInner>,
}

impl TaskSystem {
    /// Creates the blocking-thread scheduler.
    ///
    /// Each Box3D task runs on a dedicated blocking operating-system thread and
    /// is joined by the corresponding Box3D `finishTask` callback. Panics from
    /// task callbacks are contained and reported by [`crate::World::step`] as
    /// [`Error::CallbackPanicked`].
    ///
    /// Browser and WASI targets return [`Error::UnsupportedOnWasm`] because
    /// Box3D requires `finishTask` to block until child tasks complete.
    #[inline]
    pub fn blocking_threads() -> crate::Result<Self> {
        #[cfg(target_arch = "wasm32")]
        {
            Err(Error::UnsupportedOnWasm)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(Self::new(FaultMode::None))
        }
    }

    /// Returns counters for diagnostics and tests.
    #[inline]
    pub fn stats(&self) -> TaskSystemStats {
        self.inner.stats()
    }

    #[doc(hidden)]
    #[inline]
    pub fn __guard_rejections_for_test(&self) -> usize {
        self.inner.guard_rejections.load(Ordering::Relaxed)
    }

    #[inline]
    fn new(fault_mode: FaultMode) -> Self {
        Self {
            inner: Arc::new(TaskSystemInner::new(fault_mode)),
        }
    }

    #[doc(hidden)]
    #[inline]
    pub fn __panic_on_enqueue_for_test() -> Self {
        Self::new(FaultMode::PanicOnEnqueue)
    }

    #[doc(hidden)]
    #[inline]
    pub fn __panic_on_task_for_test() -> Self {
        Self::new(FaultMode::PanicOnTask)
    }

    #[doc(hidden)]
    #[inline]
    pub fn __panic_on_finish_for_test() -> Self {
        Self::new(FaultMode::PanicOnFinish)
    }

    #[doc(hidden)]
    #[inline]
    pub fn __check_callback_guard_for_test() -> Self {
        Self::new(FaultMode::CheckCallbackGuard)
    }
}

/// Stable native task context owned by one World.
///
/// The scheduler can be shared by many worlds, while each installed context
/// points at that world's invocation slot.
#[derive(Debug)]
pub(crate) struct InstalledTaskContext {
    scheduler: Arc<TaskSystemInner>,
    invocation: CallbackInvocationSlot,
}

impl InstalledTaskContext {
    pub(crate) fn new(task_system: &TaskSystem, invocation: CallbackInvocationSlot) -> Box<Self> {
        Box::new(Self {
            scheduler: Arc::clone(&task_system.inner),
            invocation,
        })
    }

    #[inline]
    fn raw_context(&self) -> *mut c_void {
        self as *const Self as *mut c_void
    }
}

/// Snapshot of a [`TaskSystem`]'s task counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskSystemStats {
    /// Number of tasks enqueued.
    pub enqueued: usize,
    /// Number of tasks started.
    pub started: usize,
    /// Number of tasks completed.
    pub completed: usize,
    /// Number of `finishTask` callbacks observed.
    pub finished: usize,
    /// Whether any task callback has panicked since this scheduler was created.
    pub panicked: bool,
}

#[derive(Debug)]
struct TaskSystemInner {
    fault_mode: FaultMode,
    panicked: AtomicBool,
    enqueued: AtomicUsize,
    started: AtomicUsize,
    completed: AtomicUsize,
    finished: AtomicUsize,
    guard_rejections: AtomicUsize,
}

impl TaskSystemInner {
    fn new(fault_mode: FaultMode) -> Self {
        Self {
            fault_mode,
            panicked: AtomicBool::new(false),
            enqueued: AtomicUsize::new(0),
            started: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            finished: AtomicUsize::new(0),
            guard_rejections: AtomicUsize::new(0),
        }
    }

    fn stats(&self) -> TaskSystemStats {
        TaskSystemStats {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            started: self.started.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            finished: self.finished.load(Ordering::Relaxed),
            panicked: self.panicked.load(Ordering::Relaxed),
        }
    }

    fn mark_panicked(&self, failures: &SharedCallbackState) {
        self.panicked.store(true, Ordering::Relaxed);
        failures.record(CallbackFailure::Panicked);
    }

    fn run_task(&self, failures: &SharedCallbackState, invocation: TaskInvocation) {
        self.started.fetch_add(1, Ordering::Relaxed);
        failures.invoke((), || {
            if self.fault_mode == FaultMode::CheckCallbackGuard
                && matches!(
                    crate::core::callback_state::check_not_in_callback(),
                    Err(Error::InCallback)
                )
            {
                self.guard_rejections.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { (invocation.task)(invocation.task_context) };
            if self.fault_mode == FaultMode::PanicOnTask {
                self.mark_panicked(failures);
                panic!("injected Box3D task panic");
            }
        });
        self.completed.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultMode {
    #[cfg(not(target_arch = "wasm32"))]
    None,
    PanicOnEnqueue,
    PanicOnTask,
    PanicOnFinish,
    CheckCallbackGuard,
}

#[derive(Clone, Copy)]
struct TaskInvocation {
    task: unsafe extern "C" fn(*mut c_void),
    task_context: *mut c_void,
}

unsafe impl Send for TaskInvocation {}

#[cfg(not(target_arch = "wasm32"))]
struct TaskHandle {
    join: Option<pool::Join>,
    failures: SharedCallbackState,
    scheduler: Arc<TaskSystemInner>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) unsafe extern "C" fn enqueue_task(
    task: ffi::b3TaskCallback,
    task_context: *mut c_void,
    user_context: *mut c_void,
    task_name: *const c_char,
) -> *mut c_void {
    let Some(context) = (unsafe { task_context_from_raw_checked(user_context) }) else {
        return std::ptr::null_mut();
    };
    let scheduler = &context.scheduler;
    let failures = context.invocation.current().unwrap_or_default();
    failures.invoke(std::ptr::null_mut(), || {
        scheduler.enqueued.fetch_add(1, Ordering::Relaxed);
        let Some(task) = task else {
            failures.record(CallbackFailure::InvalidNativeInput);
            return std::ptr::null_mut();
        };
        let invocation = TaskInvocation { task, task_context };
        if scheduler.fault_mode == FaultMode::PanicOnEnqueue {
            scheduler.run_task(&failures, invocation);
            scheduler.mark_panicked(&failures);
            panic!("injected Box3D enqueue panic");
        }

        let scheduler_for_task = Arc::clone(&context.scheduler);
        let failures_for_task = failures.clone();
        let _ = task_name;
        match pool::spawn(move || scheduler_for_task.run_task(&failures_for_task, invocation)) {
            Ok(join) => Box::into_raw(Box::new(TaskHandle {
                join: Some(join),
                failures: failures.clone(),
                scheduler: Arc::clone(&context.scheduler),
            }))
            .cast(),
            Err(_) => {
                scheduler.run_task(&failures, invocation);
                std::ptr::null_mut()
            }
        }
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) unsafe extern "C" fn enqueue_task(
    task: ffi::b3TaskCallback,
    task_context: *mut c_void,
    user_context: *mut c_void,
    _task_name: *const c_char,
) -> *mut c_void {
    let Some(context) = (unsafe { task_context_from_raw_checked(user_context) }) else {
        return std::ptr::null_mut();
    };
    let scheduler = &context.scheduler;
    let failures = context.invocation.current().unwrap_or_default();
    failures.invoke((), || {
        scheduler.enqueued.fetch_add(1, Ordering::Relaxed);
        let Some(task) = task else {
            failures.record(CallbackFailure::InvalidNativeInput);
            return;
        };
        scheduler.run_task(&failures, TaskInvocation { task, task_context });
    });
    std::ptr::null_mut()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) unsafe extern "C" fn finish_task(user_task: *mut c_void, user_context: *mut c_void) {
    if user_task.is_null() {
        return;
    }
    let mut handle = unsafe { Box::from_raw(user_task.cast::<TaskHandle>()) };
    let failures = handle.failures.clone();
    let scheduler = Arc::clone(&handle.scheduler);
    failures.invoke((), || {
        if user_context.is_null() {
            failures.record(CallbackFailure::InvalidNativeInput);
        }
        if let Some(join) = handle.join.take()
            && join.join().is_err()
        {
            scheduler.mark_panicked(&failures);
        }
        scheduler.finished.fetch_add(1, Ordering::Relaxed);
        if scheduler.fault_mode == FaultMode::PanicOnFinish {
            scheduler.mark_panicked(&failures);
            panic!("injected Box3D finish panic");
        }
    });
}

#[cfg(target_arch = "wasm32")]
pub(crate) unsafe extern "C" fn finish_task(user_task: *mut c_void, user_context: *mut c_void) {
    let Some(context) = (unsafe { task_context_from_raw_checked(user_context) }) else {
        return;
    };
    let scheduler = &context.scheduler;
    let failures = context.invocation.current().unwrap_or_default();
    failures.invoke((), || {
        if !user_task.is_null() {
            failures.record(CallbackFailure::InvalidNativeInput);
        }
        scheduler.finished.fetch_add(1, Ordering::Relaxed);
    });
}

#[inline]
pub(crate) fn install_callbacks(raw_def: &mut ffi::b3WorldDef, context: &InstalledTaskContext) {
    raw_def.enqueueTask = Some(enqueue_task);
    raw_def.finishTask = Some(finish_task);
    raw_def.userTaskContext = context.raw_context();
}

unsafe fn task_context_from_raw<'a>(context: *mut c_void) -> &'a InstalledTaskContext {
    debug_assert!(!context.is_null());
    unsafe { &*context.cast::<InstalledTaskContext>() }
}

unsafe fn task_context_from_raw_checked<'a>(
    context: *mut c_void,
) -> Option<&'a InstalledTaskContext> {
    if context.is_null() {
        None
    } else {
        Some(unsafe { task_context_from_raw(context) })
    }
}

/// A pool of parked worker threads that Box3D's tasks run on (Holonautic
/// fork). Starting an operating-system thread for every task, as upstream
/// does, cost more than the work on a Quest: the 32-ragdoll step went from
/// 1.7 ms on one thread to 3.0 ms on two and 3.2 ms on three.
#[cfg(not(target_arch = "wasm32"))]
mod pool {
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Condvar, Mutex, OnceLock};

    type Job = Box<dyn FnOnce() + Send>;

    /// Waits for one job: `Ok` once it ran, `Err` if it panicked.
    pub(super) struct Join(Arc<(Mutex<Option<bool>>, Condvar)>);

    impl Join {
        pub(super) fn join(self) -> Result<(), ()> {
            let (state, ready) = &*self.0;
            let mut done = state.lock().unwrap_or_else(|e| e.into_inner());
            while done.is_none() {
                done = ready.wait(done).unwrap_or_else(|e| e.into_inner());
            }
            if done.unwrap_or(false) { Ok(()) } else { Err(()) }
        }
    }

    fn queue() -> &'static Mutex<Sender<Job>> {
        static QUEUE: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
        QUEUE.get_or_init(|| {
            let (send, receive) = channel::<Job>();
            let receive = Arc::new(Mutex::new(receive));
            let count = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 8);
            for k in 0..count {
                let receive: Arc<Mutex<Receiver<Job>>> = Arc::clone(&receive);
                let _ = std::thread::Builder::new().name(format!("boxddd-worker-{k}")).spawn(move || {
                    loop {
                        let job = match receive.lock() {
                            Ok(r) => r.recv(),
                            Err(_) => return,
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => return,
                        }
                    }
                });
            }
            Mutex::new(send)
        })
    }

    /// Runs `f` on a pool thread.
    pub(super) fn spawn(f: impl FnOnce() + Send + 'static) -> Result<Join, ()> {
        let done = Arc::new((Mutex::new(None), Condvar::new()));
        let signal = Arc::clone(&done);
        let job: Job = Box::new(move || {
            let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_ok();
            let (state, ready) = &*signal;
            *state.lock().unwrap_or_else(|e| e.into_inner()) = Some(ok);
            ready.notify_all();
        });
        queue().lock().map_err(|_| ())?.send(job).map_err(|_| ())?;
        Ok(Join(done))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn finish_task_joins_and_reclaims_without_a_context_pointer() {
        let scheduler = Arc::new(TaskSystemInner::new(FaultMode::None));
        let failures = SharedCallbackState::default();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_by_task = Arc::clone(&completed);
        let join = pool::spawn(move || {
            completed_by_task.store(true, Ordering::Release);
        })
        .unwrap();
        let handle = Box::new(TaskHandle {
            join: Some(join),
            failures: failures.clone(),
            scheduler: Arc::clone(&scheduler),
        });

        unsafe {
            finish_task(Box::into_raw(handle).cast(), std::ptr::null_mut());
        }

        assert!(completed.load(Ordering::Acquire));
        assert_eq!(scheduler.finished.load(Ordering::Relaxed), 1);
        assert_eq!(failures.result(), Err(Error::NativeFailure));
    }

    #[test]
    fn enqueue_task_rejects_an_empty_native_callback() {
        let task_system = TaskSystem::new(FaultMode::None);
        let invocation = CallbackInvocationSlot::default();
        let context = InstalledTaskContext::new(&task_system, invocation.clone());
        let failures = SharedCallbackState::default();
        let _guard = invocation.install(failures.clone()).unwrap();

        let handle = unsafe {
            enqueue_task(
                None,
                std::ptr::null_mut(),
                context.raw_context(),
                std::ptr::null(),
            )
        };

        assert!(handle.is_null());
        assert_eq!(failures.result(), Err(Error::NativeFailure));
    }
}
