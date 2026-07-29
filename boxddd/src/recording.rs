use crate::core::length_units_override::LengthUnitsOverride;
use crate::core::provenance::{
    OwnerToken, ResourceToken, allocate_owner_token, allocate_resource_token,
};
use crate::core::{box3d_lock, callback_state, validation};
use crate::debug_draw::{
    CollectDebugDraw, DebugDraw, DebugDrawCommand, DebugDrawOptions, with_debug_draw,
};
use crate::error::{Error, InvalidValueReason, Result};
use crate::query::QueryFilter;
use crate::types::{Aabb, BodyId, BodyKey, Pos, ShapeId, ShapeKey, Vec3};
use crate::world::World;
use boxddd_sys::ffi;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Write;
use std::marker::PhantomData;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

/// Owning handle for a Box3D recording stream.
#[derive(Debug)]
pub struct Recording {
    raw: NonNull<ffi::b3Recording>,
    activity: Rc<RecordingActivity>,
}

#[derive(Debug, Default)]
pub(crate) struct RecordingActivity {
    world: Cell<Option<ffi::b3WorldId>>,
}

impl RecordingActivity {
    fn attach(&self, world: ffi::b3WorldId) -> Result<()> {
        if self.world.get().is_some() {
            return Err(Error::RecordingInUse);
        }
        self.world.set(Some(world));
        Ok(())
    }

    pub(crate) fn detach(&self, world: ffi::b3WorldId) -> bool {
        let Some(active_world) = self.world.get() else {
            return false;
        };
        if !same_world(active_world, world) {
            return false;
        }
        self.world.set(None);
        true
    }

    fn take(&self) -> Option<ffi::b3WorldId> {
        let world = self.world.get();
        self.world.set(None);
        world
    }

    fn is_active(&self) -> bool {
        self.world.get().is_some()
    }
}

impl Recording {
    /// Creates a recording buffer with Box3D's default initial capacity.
    pub fn new() -> Result<Self> {
        Self::with_capacity(0)
    }

    /// Creates a recording buffer with an optional initial byte capacity.
    pub fn with_capacity(byte_capacity: usize) -> Result<Self> {
        let byte_capacity = validation::count_i32("recording.byte_capacity", byte_capacity)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        let raw = unsafe { ffi::b3CreateRecording(byte_capacity) };
        Ok(Self {
            raw: NonNull::new(raw).ok_or(Error::NativeFailure)?,
            activity: Rc::new(RecordingActivity::default()),
        })
    }

    /// Loads a recording buffer from a file.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path_to_cstring(path)?;
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        let raw = unsafe { ffi::b3LoadRecordingFromFile(path.as_ptr()) };
        Ok(Self {
            raw: NonNull::new(raw).ok_or(Error::RecordingIoFailed)?,
            activity: Rc::new(RecordingActivity::default()),
        })
    }

    /// Saves the current recording bytes to a file.
    ///
    /// Returns [`Error::RecordingInUse`] while a world is writing to this recording.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        callback_state::check_not_in_callback()?;
        let bytes = {
            let _guard = box3d_lock::lock();
            self.check_inactive_locked()?;
            unsafe { self.bytes_locked() }.to_vec()
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
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_inactive_locked()?;
        Ok(unsafe { ffi::b3Recording_GetSize(self.raw.as_ptr()) }.max(0) as usize)
    }

    /// Returns whether the recording buffer currently has no bytes.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    /// Borrows the raw recording bytes.
    ///
    /// Returns [`Error::RecordingInUse`] while a world is writing to this recording.
    pub fn bytes(&self) -> Result<&[u8]> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_inactive_locked()?;
        Ok(unsafe { self.bytes_locked() })
    }

    /// Copies the raw recording bytes into an owned vector.
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        Ok(self.bytes()?.to_vec())
    }

    /// Validates that this recording can replay with the requested worker count.
    pub fn validate_replay(&self, worker_count: i32) -> Result<bool> {
        validate_replay_bytes(self.bytes()?, worker_count)
    }

    /// Creates a replay player from this recording.
    pub fn create_player(&self, worker_count: i32) -> Result<RecPlayer> {
        RecPlayer::from_bytes(self.bytes()?, worker_count)
    }

    #[inline]
    unsafe fn bytes_locked(&self) -> &[u8] {
        let size = unsafe { ffi::b3Recording_GetSize(self.raw.as_ptr()) }.max(0) as usize;
        let data = unsafe { ffi::b3Recording_GetData(self.raw.as_ptr()) };
        if data.is_null() || size == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, size) }
        }
    }

    fn check_inactive_locked(&self) -> Result<()> {
        if self.activity.is_active() {
            Err(Error::RecordingInUse)
        } else {
            Ok(())
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let _guard = box3d_lock::lock();
        if let Some(world) = self.activity.take()
            && unsafe { ffi::b3World_IsValid(world) }
        {
            unsafe { ffi::b3World_StopRecording(world) };
        }
        unsafe { ffi::b3DestroyRecording(self.raw.as_ptr()) };
    }
}

/// A mutable world borrow that records every native mutation into a [`Recording`].
///
/// Dropping an unfinished session stops recording before releasing either borrow.
///
/// ```compile_fail
/// use boxddd::{Recording, World, WorldDef};
///
/// let mut world = World::new(WorldDef::default()).unwrap();
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
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.world.check_world_valid_locked()?;
        self.world.stop_recording_locked();
        Ok(())
    }
}

impl Drop for RecordingSession<'_> {
    fn drop(&mut self) {
        let _guard = box3d_lock::lock();
        self.world.stop_recording_locked();
    }
}

/// Kind of query captured in a recording.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RecQueryType {
    /// Axis-aligned bounding-box overlap query.
    OverlapAabb,
    /// Shape overlap query.
    OverlapShape,
    /// Ray-cast query.
    CastRay,
    /// Shape-cast query.
    CastShape,
    /// Closest-hit ray-cast query.
    CastRayClosest,
    /// Capsule mover cast query.
    CastMover,
    /// Capsule mover collision-plane query.
    CollideMover,
}

impl RecQueryType {
    /// Converts raw Box3D data into the safe value type.
    pub const fn from_raw(raw: ffi::b3RecQueryType) -> Option<Self> {
        match raw {
            ffi::b3RecQueryType_b3_recQueryOverlapAABB => Some(Self::OverlapAabb),
            ffi::b3RecQueryType_b3_recQueryOverlapShape => Some(Self::OverlapShape),
            ffi::b3RecQueryType_b3_recQueryCastRay => Some(Self::CastRay),
            ffi::b3RecQueryType_b3_recQueryCastShape => Some(Self::CastShape),
            ffi::b3RecQueryType_b3_recQueryCastRayClosest => Some(Self::CastRayClosest),
            ffi::b3RecQueryType_b3_recQueryCastMover => Some(Self::CastMover),
            ffi::b3RecQueryType_b3_recQueryCollideMover => Some(Self::CollideMover),
            _ => None,
        }
    }
}

/// Metadata stored by a recording replay player.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct RecPlayerInfo {
    /// Number of recorded frames.
    pub frame_count: i32,
    /// Worker count used by the recording.
    pub worker_count: i32,
    /// Simulation time step for the frame.
    pub time_step: f32,
    /// Sub-step count used by the recording.
    pub sub_step_count: i32,
    /// Length scale used by the recording.
    pub length_scale: f32,
    /// Bounds stored in the recording metadata.
    pub bounds: Aabb,
}

impl RecPlayerInfo {
    /// Converts raw Box3D data into the safe value type.
    #[inline]
    pub fn from_raw(raw: ffi::b3RecPlayerInfo) -> Self {
        Self {
            frame_count: raw.frameCount,
            worker_count: raw.workerCount,
            time_step: raw.timeStep,
            sub_step_count: raw.subStepCount,
            length_scale: raw.lengthScale,
            bounds: Aabb::from_raw(raw.bounds),
        }
    }
}

/// Metadata for a query captured in the current replay frame.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct RecQueryInfo {
    /// Recorded query kind.
    pub query_type: RecQueryType,
    /// Collision filter used by the query.
    pub filter: QueryFilter,
    /// Axis-aligned bounding box.
    pub aabb: Aabb,
    /// Query origin.
    pub origin: Pos,
    /// Translation vector used by the query.
    pub translation: Vec3,
    /// Number of hits recorded for the query.
    pub hit_count: i32,
    /// Recording key.
    pub key: u64,
    /// User-defined query identifier.
    pub id: u64,
    /// Recorded name.
    pub name: Option<String>,
}

impl RecQueryInfo {
    fn from_raw(raw: ffi::b3RecQueryInfo) -> Result<Self> {
        Ok(Self {
            query_type: RecQueryType::from_raw(raw.type_).ok_or(Error::NativeFailure)?,
            filter: QueryFilter {
                category_bits: raw.filter.categoryBits,
                mask_bits: raw.filter.maskBits,
                id: raw.filter.id,
            },
            aabb: Aabb::from_raw(raw.aabb),
            origin: Pos::from_raw(raw.origin),
            translation: Vec3::from_raw(raw.translation),
            hit_count: raw.hitCount,
            key: raw.key,
            id: raw.id,
            name: if raw.name.is_null() {
                None
            } else {
                Some(
                    unsafe { CStr::from_ptr(raw.name) }
                        .to_string_lossy()
                        .into_owned(),
                )
            },
        })
    }
}

/// Hit recorded for a replay-frame query.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RecQueryHit {
    /// Shape associated with the result.
    pub shape_id: ShapeId,
    /// World-space point for the result.
    pub point: Pos,
    /// World-space normal for the result.
    pub normal: Vec3,
    /// Fraction along the query translation.
    pub fraction: f32,
}

impl RecQueryHit {
    #[inline]
    fn from_raw(raw: ffi::b3RecQueryHit, player: &RecPlayer) -> Result<Self> {
        Ok(Self {
            shape_id: player.resolve_shape(raw.shape)?,
            point: Pos::from_raw(raw.point),
            normal: Vec3::from_raw(raw.normal),
            fraction: raw.fraction,
        })
    }
}

/// World id created by a replay player.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ReplayWorldId {
    raw: ReplayWorldKey,
    owner: OwnerToken,
    resource: ResourceToken,
}

impl std::fmt::Debug for ReplayWorldId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ReplayWorldId(..)")
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct ReplayWorldKey {
    index1: u16,
    generation: u16,
}

impl ReplayWorldId {
    #[inline]
    const fn from_parts(raw: ffi::b3WorldId, owner: OwnerToken, resource: ResourceToken) -> Self {
        Self {
            raw: ReplayWorldKey {
                index1: raw.index1,
                generation: raw.generation,
            },
            owner,
            resource,
        }
    }
}

#[derive(Default, Debug)]
struct ReplayResources {
    bodies: HashMap<BodyKey, ResourceToken>,
    shapes: HashMap<ShapeKey, ResourceToken>,
}

impl ReplayResources {
    fn resolve_body(&mut self, raw: ffi::b3BodyId, owner: OwnerToken) -> Result<BodyId> {
        let key = BodyKey::from_raw(raw);
        let resource = match self.bodies.get(&key).copied() {
            Some(resource) => resource,
            None => {
                self.bodies
                    .try_reserve(1)
                    .map_err(|_| Error::AllocationFailed)?;
                let resource = allocate_resource_token()?;
                self.bodies.insert(key, resource);
                resource
            }
        };
        Ok(BodyId::from_parts(raw, owner, resource))
    }

    fn resolve_shape(&mut self, raw: ffi::b3ShapeId, owner: OwnerToken) -> Result<ShapeId> {
        let key = ShapeKey::from_raw(raw);
        let resource = match self.shapes.get(&key).copied() {
            Some(resource) => resource,
            None => {
                self.shapes
                    .try_reserve(1)
                    .map_err(|_| Error::AllocationFailed)?;
                let resource = allocate_resource_token()?;
                self.shapes.insert(key, resource);
                resource
            }
        };
        Ok(ShapeId::from_parts(raw, owner, resource))
    }
}

/// Player used to replay and inspect a Box3D recording.
///
/// Each native operation temporarily applies the recording's length scale and restores the
/// process-global setting before returning.
#[derive(Debug)]
pub struct RecPlayer {
    raw: NonNull<ffi::b3RecPlayer>,
    owner: OwnerToken,
    replay_world: ReplayWorldId,
    resources: RefCell<ReplayResources>,
    length_units: f32,
    _not_send_sync: PhantomData<Rc<()>>,
}

impl RecPlayer {
    /// Creates a replay player from raw recording bytes.
    pub fn from_bytes(bytes: &[u8], worker_count: i32) -> Result<Self> {
        let length_units = validate_replay_input(bytes, worker_count)?;
        callback_state::check_not_in_callback()?;
        let owner = allocate_owner_token()?;
        let replay_world_resource = allocate_resource_token()?;
        let (raw, replay_world_raw) = with_replay_length_units(length_units, || {
            let raw = unsafe {
                ffi::b3RecPlayer_Create(bytes.as_ptr().cast(), bytes.len() as i32, worker_count)
            };
            let raw = NonNull::new(raw).ok_or(Error::NativeFailure)?;
            let replay_world_raw = unsafe { ffi::b3RecPlayer_GetWorldId(raw.as_ptr()) };
            Ok::<_, Error>((raw, replay_world_raw))
        })?;
        Ok(Self {
            raw,
            owner,
            replay_world: ReplayWorldId::from_parts(replay_world_raw, owner, replay_world_resource),
            resources: RefCell::new(ReplayResources::default()),
            length_units,
            _not_send_sync: PhantomData,
        })
    }

    /// Returns the world id owned by the replay player.
    pub fn world_id(&self) -> ReplayWorldId {
        self.replay_world
    }

    /// Steps the replay by one recorded frame.
    pub fn step_frame(&mut self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        let stepped = self.with_native(|raw| unsafe { ffi::b3RecPlayer_StepFrame(raw) });
        if stepped {
            self.resources.get_mut().bodies.clear();
            self.resources.get_mut().shapes.clear();
        }
        Ok(stepped)
    }

    /// Advances one replay frame in two phases, pausing after body creation.
    pub fn sub_step_frame(&mut self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_SubStepFrame(raw) });
        self.resources.get_mut().bodies.clear();
        self.resources.get_mut().shapes.clear();
        Ok(())
    }

    /// Restarts replay from the first recorded frame.
    pub fn restart(&mut self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_Restart(raw) });
        self.resources.get_mut().bodies.clear();
        self.resources.get_mut().shapes.clear();
        Ok(())
    }

    /// Seeks replay to a recorded frame.
    pub fn seek_frame(&mut self, target_frame: i32) -> Result<()> {
        if target_frame < 0 {
            return Err(validation::invalid(
                "rec_player.target_frame",
                InvalidValueReason::OutOfRange,
            ));
        }
        callback_state::check_not_in_callback()?;
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_SeekFrame(raw, target_frame) });
        self.resources.get_mut().bodies.clear();
        self.resources.get_mut().shapes.clear();
        Ok(())
    }

    /// Returns the current replay frame index.
    pub fn frame(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrame(raw) }))
    }

    /// Returns the number of recorded frames.
    pub fn frame_count(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrameCount(raw) }))
    }

    /// Returns whether replay has reached the end of the recording.
    pub fn is_at_end(&self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_IsAtEnd(raw) }))
    }

    /// Returns whether a sub-stepped replay is paused before the world step.
    pub fn is_at_pre_step(&self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_IsAtPreStep(raw) }))
    }

    /// Returns whether replay has diverged from the recorded state.
    pub fn has_diverged(&self) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_HasDiverged(raw) }))
    }

    /// Returns metadata for the loaded recording.
    pub fn info(&self) -> Result<RecPlayerInfo> {
        callback_state::check_not_in_callback()?;
        Ok(RecPlayerInfo::from_raw(self.with_native(|raw| unsafe {
            ffi::b3RecPlayer_GetInfo(raw)
        })))
    }

    /// Returns the first frame where replay diverged, if any.
    pub fn diverge_frame(&self) -> Result<Option<i32>> {
        callback_state::check_not_in_callback()?;
        let frame = self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetDivergeFrame(raw) });
        Ok((frame >= 0).then_some(frame))
    }

    /// Sets the worker count.
    pub fn set_worker_count(&mut self, count: i32) -> Result<()> {
        validate_replay_worker_count(count)?;
        callback_state::check_not_in_callback()?;
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_SetWorkerCount(raw, count) });
        Ok(())
    }

    /// Sets the keyframe policy.
    pub fn set_keyframe_policy(
        &mut self,
        budget_bytes: usize,
        min_interval_frames: i32,
    ) -> Result<()> {
        if min_interval_frames < 0 {
            return Err(validation::invalid(
                "rec_player.keyframe_min_interval",
                InvalidValueReason::OutOfRange,
            ));
        }
        callback_state::check_not_in_callback()?;
        self.with_native(|raw| unsafe {
            ffi::b3RecPlayer_SetKeyframePolicy(raw, budget_bytes, min_interval_frames)
        });
        Ok(())
    }

    /// Returns the configured replay keyframe memory budget.
    pub fn keyframe_budget(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeBudget(raw) }))
    }

    /// Returns the minimum frame interval between replay keyframes.
    pub fn keyframe_min_interval(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeMinInterval(raw) }))
    }

    /// Returns the current frame interval between replay keyframes.
    pub fn keyframe_interval(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeInterval(raw) }))
    }

    /// Returns the bytes currently used by replay keyframes.
    pub fn keyframe_bytes(&self) -> Result<usize> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeBytes(raw) }))
    }

    /// Returns the number of bodies in the replay world.
    pub fn body_count(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetBodyCount(raw) }))
    }

    /// Returns a body id from the replay world by index.
    pub fn body_id(&self, index: i32) -> Result<Option<BodyId>> {
        if index < 0 {
            return Err(validation::invalid(
                "rec_player.body_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        callback_state::check_not_in_callback()?;
        let raw = self.with_native(|player| unsafe { ffi::b3RecPlayer_GetBodyId(player, index) });
        if raw.index1 == 0 {
            Ok(None)
        } else {
            Ok(Some(self.resolve_body(raw)?))
        }
    }

    /// Returns the number of queries captured for the current replay frame.
    pub fn frame_query_count(&self) -> Result<i32> {
        callback_state::check_not_in_callback()?;
        Ok(self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrameQueryCount(raw) }))
    }

    /// Returns metadata for a captured query in the current replay frame.
    pub fn frame_query(&self, index: i32) -> Result<RecQueryInfo> {
        callback_state::check_not_in_callback()?;
        if index < 0 || index >= self.frame_query_count()? {
            return Err(validation::invalid(
                "rec_player.query_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        RecQueryInfo::from_raw(
            self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrameQuery(raw, index) }),
        )
    }

    /// Returns a hit recorded for a captured frame query.
    pub fn frame_query_hit(&self, query_index: i32, hit_index: i32) -> Result<RecQueryHit> {
        callback_state::check_not_in_callback()?;
        let query = self.frame_query(query_index)?;
        if hit_index < 0 || hit_index >= query.hit_count {
            return Err(validation::invalid(
                "rec_player.hit_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        RecQueryHit::from_raw(
            self.with_native(|raw| unsafe {
                ffi::b3RecPlayer_GetFrameQueryHit(raw, query_index, hit_index)
            }),
            self,
        )
    }

    /// Collects debug draw commands for recorded frame queries.
    pub fn draw_frame_queries_collect(
        &mut self,
        options: DebugDrawOptions,
        query_index: Option<i32>,
        selected_index: Option<i32>,
    ) -> Result<Vec<DebugDrawCommand>> {
        let mut commands = Vec::new();
        self.draw_frame_queries_collect_into(&mut commands, options, query_index, selected_index)?;
        Ok(commands)
    }

    /// Collects debug draw commands for recorded frame queries into `out`.
    pub fn draw_frame_queries_collect_into(
        &mut self,
        out: &mut Vec<DebugDrawCommand>,
        options: DebugDrawOptions,
        query_index: Option<i32>,
        selected_index: Option<i32>,
    ) -> Result<()> {
        out.clear();
        let mut collector = CollectDebugDraw::new(out);
        self.draw_frame_queries(&mut collector, options, query_index, selected_index)?;
        collector.finish();
        Ok(())
    }

    /// Draws recorded frame queries with a custom debug draw sink.
    pub fn draw_frame_queries(
        &mut self,
        drawer: &mut impl DebugDraw,
        options: DebugDrawOptions,
        query_index: Option<i32>,
        selected_index: Option<i32>,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let query_index = checked_optional_index("rec_player.query_index", query_index)?;
        let selected_index = checked_optional_index("rec_player.selected_index", selected_index)?;
        if query_index >= 0 && query_index >= self.frame_query_count()? {
            return Err(validation::invalid(
                "rec_player.query_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        with_debug_draw(drawer, options, |draw| {
            self.with_native(|raw| unsafe {
                ffi::b3RecPlayer_DrawFrameQueries(raw, draw, query_index, selected_index)
            });
            Ok(())
        })
    }
}

impl RecPlayer {
    fn with_native<R>(&self, operation: impl FnOnce(*mut ffi::b3RecPlayer) -> R) -> R {
        with_replay_length_units(self.length_units, || operation(self.raw.as_ptr()))
    }

    fn resolve_body(&self, raw: ffi::b3BodyId) -> Result<BodyId> {
        self.resources.borrow_mut().resolve_body(raw, self.owner)
    }

    fn resolve_shape(&self, raw: ffi::b3ShapeId) -> Result<ShapeId> {
        self.resources.borrow_mut().resolve_shape(raw, self.owner)
    }
}

impl Drop for RecPlayer {
    fn drop(&mut self) {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_Destroy(raw) });
    }
}

impl World {
    /// Starts recording world mutations into `recording`.
    ///
    /// Returns [`Error::RecordingInUse`] if this world or `recording` is already attached to an
    /// active recording session.
    pub fn record<'a>(&'a mut self, recording: &'a mut Recording) -> Result<RecordingSession<'a>> {
        callback_state::check_not_in_callback()?;
        let _guard = box3d_lock::lock();
        self.check_world_valid_locked()?;
        if self
            .state
            .active_recording
            .as_ref()
            .is_some_and(|activity| activity.is_active())
        {
            return Err(Error::RecordingInUse);
        }
        self.state.active_recording = None;
        recording.activity.attach(self.raw())?;
        self.state.active_recording = Some(Rc::clone(&recording.activity));
        unsafe { ffi::b3World_StartRecording(self.raw(), recording.raw.as_ptr()) };
        Ok(RecordingSession {
            world: self,
            _recording: recording,
        })
    }

    pub(crate) fn stop_recording_locked(&mut self) {
        let Some(activity) = self.state.active_recording.take() else {
            return;
        };
        if activity.detach(self.raw()) && unsafe { ffi::b3World_IsValid(self.raw()) } {
            unsafe { ffi::b3World_StopRecording(self.raw()) };
        }
    }
}

fn same_world(a: ffi::b3WorldId, b: ffi::b3WorldId) -> bool {
    a.index1 == b.index1 && a.generation == b.generation
}

/// Validates that raw recording bytes can replay with the requested worker count.
pub fn validate_replay_bytes(bytes: &[u8], worker_count: i32) -> Result<bool> {
    let length_units = validate_replay_input(bytes, worker_count)?;
    callback_state::check_not_in_callback()?;
    Ok(with_replay_length_units(length_units, || unsafe {
        ffi::b3ValidateReplay(bytes.as_ptr().cast(), bytes.len() as i32, worker_count)
    }))
}

const RECORDING_HEADER_BYTES: usize = 48;
const RECORDING_LENGTH_SCALE_OFFSET: usize = 12;

fn validate_replay_input(bytes: &[u8], worker_count: i32) -> Result<f32> {
    validate_replay_worker_count(worker_count)?;
    validation::count_i32("recording.replay_bytes", bytes.len())?;
    let header = bytes.get(..RECORDING_HEADER_BYTES).ok_or_else(|| {
        validation::invalid("recording.replay_bytes", InvalidValueReason::Malformed)
    })?;
    let scale_bytes: [u8; std::mem::size_of::<f32>()] = header
        .get(
            RECORDING_LENGTH_SCALE_OFFSET
                ..RECORDING_LENGTH_SCALE_OFFSET + std::mem::size_of::<f32>(),
        )
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            validation::invalid("recording.replay_bytes", InvalidValueReason::Malformed)
        })?;
    let length_units = f32::from_le_bytes(scale_bytes);
    validation::positive("recording.length_scale", length_units)?;
    Ok(length_units)
}

fn with_replay_length_units<R>(length_units: f32, operation: impl FnOnce() -> R) -> R {
    let guard = box3d_lock::lock();
    let _length_units_override = LengthUnitsOverride::new_locked(&guard, length_units);
    operation()
}

fn validate_replay_worker_count(worker_count: i32) -> Result<()> {
    if worker_count < 1 || worker_count > ffi::B3_MAX_WORKERS as i32 {
        return Err(validation::invalid(
            "rec_player.worker_count",
            InvalidValueReason::OutOfRange,
        ));
    }
    #[cfg(target_arch = "wasm32")]
    if worker_count > 1 {
        return Err(Error::UnsupportedOnWasm);
    }
    Ok(())
}

fn checked_optional_index(context: &'static str, index: Option<i32>) -> Result<i32> {
    match index {
        Some(index) if index < 0 => {
            Err(validation::invalid(context, InvalidValueReason::OutOfRange))
        }
        Some(index) => Ok(index),
        None => Ok(-1),
    }
}

fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    CString::new(path.as_ref().as_os_str().to_string_lossy().as_bytes())
        .map_err(|_| validation::invalid("recording.path", InvalidValueReason::InteriorNul))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_input_errors_are_typed() {
        assert_eq!(
            validate_replay_input(&[], 1),
            Err(Error::InvalidValue {
                context: "recording.replay_bytes",
                reason: InvalidValueReason::Malformed,
            })
        );
        assert_eq!(
            validate_replay_worker_count(0),
            Err(Error::InvalidValue {
                context: "rec_player.worker_count",
                reason: InvalidValueReason::OutOfRange,
            })
        );
    }

    #[test]
    fn replay_input_reads_a_positive_little_endian_header_scale() {
        let mut header = [0_u8; RECORDING_HEADER_BYTES];
        header[RECORDING_LENGTH_SCALE_OFFSET
            ..RECORDING_LENGTH_SCALE_OFFSET + std::mem::size_of::<f32>()]
            .copy_from_slice(&2.5_f32.to_le_bytes());
        assert_eq!(validate_replay_input(&header, 1), Ok(2.5));

        header[RECORDING_LENGTH_SCALE_OFFSET
            ..RECORDING_LENGTH_SCALE_OFFSET + std::mem::size_of::<f32>()]
            .copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert_eq!(
            validate_replay_input(&header, 1),
            Err(Error::InvalidValue {
                context: "recording.length_scale",
                reason: InvalidValueReason::NonFinite,
            })
        );
    }

    #[test]
    fn recording_indices_and_paths_preserve_context() {
        assert_eq!(
            checked_optional_index("rec_player.query_index", Some(-1)),
            Err(Error::InvalidValue {
                context: "rec_player.query_index",
                reason: InvalidValueReason::OutOfRange,
            })
        );
        assert_eq!(
            path_to_cstring("invalid\0path"),
            Err(Error::InvalidValue {
                context: "recording.path",
                reason: InvalidValueReason::InteriorNul,
            })
        );
    }
}
