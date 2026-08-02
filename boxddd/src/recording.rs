mod replay;

#[doc(inline)]
pub use replay::{
    RecPlayer, RecPlayerInfo, RecQueryHit, RecQueryInfo, RecQueryType, ReplayWorldId,
};

use crate::core::foundation::Foundation;
use crate::core::provenance::{ResourceToken, allocate_resource_token};
use crate::core::{callback_state, validation};
use crate::error::{Error, InvalidValueReason, Result};
use crate::world::World;
use boxddd_sys::ffi;
use std::cell::Cell;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

/// Owning handle for a Box3D recording stream.
#[derive(Debug)]
pub struct Recording {
    owner: Option<RecordingOwner>,
}

#[derive(Debug)]
struct RecordingOwner {
    raw: NonNull<ffi::b3Recording>,
    activity: Rc<RecordingActivity>,
}

#[derive(Clone, Copy, Debug, Default)]
enum RecordingAttachment {
    #[default]
    Detached,
    Attached {
        world: ffi::b3WorldId,
        token: ResourceToken,
    },
    StopPending {
        world: ffi::b3WorldId,
        token: ResourceToken,
    },
}

#[derive(Debug, Default)]
pub(crate) struct RecordingActivity {
    attachment: Cell<RecordingAttachment>,
}

impl RecordingActivity {
    fn attach(&self, world: ffi::b3WorldId) -> Result<()> {
        if !matches!(self.attachment.get(), RecordingAttachment::Detached) {
            return Err(Error::RecordingInUse);
        }
        self.attachment.set(RecordingAttachment::Attached {
            world,
            token: allocate_resource_token()?,
        });
        Ok(())
    }

    fn detach(&self, world: ffi::b3WorldId) -> bool {
        match self.attachment.get() {
            RecordingAttachment::Attached {
                world: active_world,
                ..
            }
            | RecordingAttachment::StopPending {
                world: active_world,
                ..
            } if same_world(active_world, world) => {
                self.attachment.set(RecordingAttachment::Detached);
                true
            }
            _ => false,
        }
    }

    fn request_stop(&self, world: ffi::b3WorldId) -> Option<ResourceToken> {
        let RecordingAttachment::Attached {
            world: active_world,
            token,
        } = self.attachment.get()
        else {
            return None;
        };
        if !same_world(active_world, world) {
            return None;
        }
        self.attachment
            .set(RecordingAttachment::StopPending { world, token });
        Some(token)
    }

    fn complete_stop(&self, world: ffi::b3WorldId, token: ResourceToken) -> bool {
        match self.attachment.get() {
            RecordingAttachment::StopPending {
                world: active_world,
                token: active_token,
            } if same_world(active_world, world) && active_token == token => {
                self.attachment.set(RecordingAttachment::Detached);
                true
            }
            _ => false,
        }
    }

    fn take(&self) -> Option<ffi::b3WorldId> {
        let world = match self.attachment.get() {
            RecordingAttachment::Detached => None,
            RecordingAttachment::Attached { world, .. }
            | RecordingAttachment::StopPending { world, .. } => Some(world),
        };
        self.attachment.set(RecordingAttachment::Detached);
        world
    }

    fn is_active(&self) -> bool {
        !matches!(self.attachment.get(), RecordingAttachment::Detached)
    }
}

impl Recording {
    /// Creates a recording buffer with Box3D's default initial capacity.
    pub fn new() -> Result<Self> {
        Self::with_capacity(0)
    }

    /// Creates a recording buffer with an optional initial byte capacity.
    pub fn with_capacity(byte_capacity: usize) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let byte_capacity = validation::count_i32("recording.byte_capacity", byte_capacity)?;
        let _call = Foundation::enter_transient_call()?;
        let raw = unsafe { ffi::b3CreateRecording(byte_capacity) };
        Ok(Self {
            owner: Some(RecordingOwner {
                raw: NonNull::new(raw).ok_or(Error::NativeFailure)?,
                activity: Rc::new(RecordingActivity::default()),
            }),
        })
    }

    /// Loads a recording buffer from a file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        callback_state::check_not_in_callback()?;
        let path = path_to_cstring(path)?;
        let _call = Foundation::enter_transient_call()?;
        let raw = unsafe { ffi::b3LoadRecordingFromFile(path.as_ptr()) };
        Ok(Self {
            owner: Some(RecordingOwner {
                raw: NonNull::new(raw).ok_or(Error::RecordingIoFailed)?,
                activity: Rc::new(RecordingActivity::default()),
            }),
        })
    }

    /// Saves the current recording bytes to a file.
    ///
    /// Returns [`Error::RecordingInUse`] while a world is writing to this recording.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let path = path.as_ref();
        let bytes = {
            let _call = Foundation::enter_transient_call()?;
            self.check_inactive()?;
            self.native_bytes().to_vec()
        };

        let mut file = File::create(path).map_err(|_| Error::RecordingIoFailed)?;
        file.write_all(&bytes)
            .and_then(|_| file.flush())
            .map_err(|_| Error::RecordingIoFailed)
    }

    /// Returns the number of bytes currently stored in the recording buffer.
    ///
    /// Returns [`Error::RecordingInUse`] while a world is writing to this recording.
    pub fn len(&self) -> Result<usize> {
        let _call = Foundation::enter_transient_call()?;
        self.check_inactive()?;
        Ok(unsafe { ffi::b3Recording_GetSize(self.raw_ptr()) }.max(0) as usize)
    }

    /// Returns whether the recording buffer currently has no bytes.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Borrows the raw recording bytes.
    ///
    /// Returns [`Error::RecordingInUse`] while a world is writing to this recording.
    pub fn bytes(&self) -> Result<&[u8]> {
        let _call = Foundation::enter_transient_call()?;
        self.check_inactive()?;
        Ok(self.native_bytes())
    }

    /// Copies the raw recording bytes into an owned vector.
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        Ok(self.bytes()?.to_vec())
    }

    #[inline]
    fn native_bytes(&self) -> &[u8] {
        let size = unsafe { ffi::b3Recording_GetSize(self.raw_ptr()) }.max(0) as usize;
        let data = unsafe { ffi::b3Recording_GetData(self.raw_ptr()) };
        if data.is_null() || size == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, size) }
        }
    }

    fn check_inactive(&self) -> Result<()> {
        if self.activity().is_active() {
            Err(Error::RecordingInUse)
        } else {
            Ok(())
        }
    }

    fn owner(&self) -> &RecordingOwner {
        self.owner
            .as_ref()
            .expect("Recording owner is present outside destruction")
    }

    fn raw_ptr(&self) -> *mut ffi::b3Recording {
        self.owner().raw.as_ptr()
    }

    fn activity(&self) -> &Rc<RecordingActivity> {
        &self.owner().activity
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let cleanup = move || destroy_recording(owner);
        if callback_state::in_callback() {
            callback_state::defer_local_cleanup_or_retain(cleanup);
        } else {
            cleanup();
        }
    }
}

fn destroy_recording(owner: RecordingOwner) {
    let owner = callback_state::RetainOnUnwind::new(owner);
    if let Some(world) = owner.activity.take()
        && unsafe { ffi::b3World_IsValid(world) }
    {
        unsafe { ffi::b3World_StopRecording(world) };
    }
    unsafe { ffi::b3DestroyRecording(owner.raw.as_ptr()) };
    owner.finish();
}

/// A mutable world borrow that records every native mutation into a [`Recording`].
///
/// Dropping an unfinished session stops recording before releasing either borrow.
///
/// ```compile_fail
/// use boxddd::{Foundation, Recording};
///
/// let foundation = Foundation::initialize_default().unwrap();
/// let mut world = foundation.create_world(foundation.world_def()).unwrap();
/// let mut recording = Recording::new().unwrap();
/// let mut session = world.record(&mut recording).unwrap();
/// let _second = world.record(&mut recording);
/// session.finish().unwrap();
/// ```
#[must_use = "recording remains active until the session is finished or dropped"]
#[derive(Debug)]
pub struct RecordingSession<'a> {
    world: &'a mut World,
    _recording: &'a mut Recording,
}

impl RecordingSession<'_> {
    /// Returns the world whose operations are being recorded.
    pub fn world(&mut self) -> &mut World {
        self.world
    }

    /// Stops recording and finalizes the recording stream.
    pub fn finish(self) -> Result<()> {
        let _call = self.world.enter_world_call()?;
        self.world.stop_recording();
        Ok(())
    }
}

impl Drop for RecordingSession<'_> {
    fn drop(&mut self) {
        if callback_state::in_callback() {
            self.world.defer_stop_recording();
        } else {
            let _call = callback_state::OwnerCallFrame::enter();
            self.world.stop_recording();
        }
    }
}

impl World {
    /// Starts recording world mutations into `recording`.
    ///
    /// Returns [`Error::RecordingInUse`] if this world or `recording` is already attached to an
    /// active recording session.
    pub fn record<'a>(&'a mut self, recording: &'a mut Recording) -> Result<RecordingSession<'a>> {
        let _call = self.enter_world_call()?;
        let raw = self.raw();
        if self
            .state()
            .active_recording
            .as_ref()
            .is_some_and(|activity| activity.is_active())
        {
            return Err(Error::RecordingInUse);
        }
        self.state_mut().active_recording = None;
        recording.activity().attach(raw)?;
        self.state_mut().active_recording = Some(Rc::clone(recording.activity()));
        unsafe { ffi::b3World_StartRecording(raw, recording.raw_ptr()) };
        Ok(RecordingSession {
            world: self,
            _recording: recording,
        })
    }

    fn stop_recording(&mut self) {
        let world = self.raw();
        stop_recording_owner(world, self.state_mut());
    }

    fn defer_stop_recording(&mut self) {
        let Some(activity) = self.state().active_recording.as_ref().map(Rc::clone) else {
            return;
        };
        let world = self.raw();
        let Some(token) = activity.request_stop(world) else {
            return;
        };
        callback_state::defer_local_cleanup_or_retain(move || {
            if activity.complete_stop(world, token) && unsafe { ffi::b3World_IsValid(world) } {
                unsafe { ffi::b3World_StopRecording(world) };
            }
        });
    }
}

pub(crate) fn stop_recording_owner(world: ffi::b3WorldId, state: &mut crate::world::WorldState) {
    let Some(activity) = state.active_recording.take() else {
        return;
    };
    if activity.detach(world) && unsafe { ffi::b3World_IsValid(world) } {
        unsafe { ffi::b3World_StopRecording(world) };
    }
}

fn same_world(a: ffi::b3WorldId, b: ffi::b3WorldId) -> bool {
    a.index1 == b.index1 && a.generation == b.generation
}

fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    CString::new(path.as_ref().as_os_str().to_string_lossy().as_bytes())
        .map_err(|_| validation::invalid("recording.path", InvalidValueReason::InteriorNul))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_paths_preserve_context() {
        assert_eq!(
            path_to_cstring("invalid\0path"),
            Err(Error::InvalidValue {
                context: "recording.path",
                reason: InvalidValueReason::InteriorNul,
            })
        );
    }
}
