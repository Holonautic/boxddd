use crate::core::foundation::{Foundation, ReplayLease, validate_length_units};
use crate::core::provenance::{
    OwnerToken, ResourceToken, allocate_owner_token, allocate_resource_token,
};
use crate::core::{callback_state, units, validation};
use crate::debug_draw::{
    CollectDebugDraw, DebugDraw, DebugDrawCommand, DebugDrawOptions, with_replay_debug_draw,
};
use crate::error::{Error, InvalidValueReason, Result};
use crate::query::QueryFilter;
use crate::types::{Aabb, BodyId, BodyKey, Pos, ShapeId, ShapeKey, Vec3};
use boxddd_sys::ffi;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

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
/// A player owns exclusive Foundation activity for its complete native lifetime. Ordinary native
/// owners and worldless Box3D operations are rejected until the player is closed or dropped.
#[derive(Debug)]
pub struct RecPlayer {
    owner: Option<RecPlayerOwner>,
}

#[derive(Debug)]
struct RecPlayerOwner {
    raw: NonNull<ffi::b3RecPlayer>,
    owner_token: OwnerToken,
    replay_world: ReplayWorldId,
    resources: RefCell<ReplayResources>,
    foundation_lease: ReplayLease,
    _not_send_sync: PhantomData<Rc<()>>,
}

struct ReplayTransactionGuard<'a> {
    player: Option<NonNull<ffi::b3RecPlayer>>,
    lease: &'a ReplayLease,
    restore_scale: bool,
}

impl<'a> ReplayTransactionGuard<'a> {
    fn new(lease: &'a ReplayLease) -> Self {
        Self {
            player: None,
            lease,
            restore_scale: true,
        }
    }

    fn track_player(&mut self, player: NonNull<ffi::b3RecPlayer>) {
        self.player = Some(player);
    }

    fn restore(mut self) -> Result<()> {
        if let Some(player) = self.player.take() {
            unsafe { ffi::b3RecPlayer_Destroy(player.as_ptr()) };
        }
        let result = units::restore_foundation_scale(self.lease);
        self.restore_scale = false;
        result
    }

    fn commit(mut self) {
        self.player = None;
        self.restore_scale = false;
    }
}

impl Drop for ReplayTransactionGuard<'_> {
    fn drop(&mut self) {
        if let Some(player) = self.player.take() {
            unsafe { ffi::b3RecPlayer_Destroy(player.as_ptr()) };
        }
        if self.restore_scale {
            let _ = units::restore_foundation_scale(self.lease);
        }
    }
}

impl Foundation {
    /// Creates a replay player that owns exclusive Foundation activity until native destruction.
    pub fn create_replay_player(
        &'static self,
        bytes: &[u8],
        worker_count: i32,
    ) -> Result<RecPlayer> {
        callback_state::check_not_in_callback()?;
        let length_units = validate_replay_input(bytes, worker_count)?;
        let foundation_lease = self.acquire_replay()?;
        let owner = allocate_owner_token()?;
        let replay_world_resource = allocate_resource_token()?;
        let _call = foundation_lease.enter_call()?;
        let _slot_guard = crate::core::foundation::world_slot_mutation_lock();
        let mut transaction = ReplayTransactionGuard::new(&foundation_lease);
        let raw = NonNull::new(unsafe {
            ffi::b3RecPlayer_Create(bytes.as_ptr().cast(), bytes.len() as i32, worker_count)
        });
        let Some(raw) = raw else {
            transaction.restore()?;
            return Err(Error::NativeFailure);
        };
        transaction.track_player(raw);

        if let Err(error) = units::verify_replay_scale(&foundation_lease, length_units) {
            let _ = transaction.restore();
            return Err(error);
        }
        let replay_world_raw = unsafe { ffi::b3RecPlayer_GetWorldId(raw.as_ptr()) };
        if !unsafe { ffi::b3World_IsValid(replay_world_raw) } {
            transaction.restore()?;
            return Err(Error::NativeFailure);
        }
        transaction.commit();
        drop(_slot_guard);
        drop(_call);

        Ok(RecPlayer {
            owner: Some(RecPlayerOwner {
                raw,
                owner_token: owner,
                replay_world: ReplayWorldId::from_parts(
                    replay_world_raw,
                    owner,
                    replay_world_resource,
                ),
                resources: RefCell::new(ReplayResources::default()),
                foundation_lease,
                _not_send_sync: PhantomData,
            }),
        })
    }

    /// Validates replay bytes while holding exclusive Foundation activity for the full native call.
    pub fn validate_replay(&'static self, bytes: &[u8], worker_count: i32) -> Result<bool> {
        callback_state::check_not_in_callback()?;
        validate_replay_input(bytes, worker_count)?;
        let foundation_lease = self.acquire_replay()?;
        let _call = foundation_lease.enter_call()?;
        let _slot_guard = crate::core::foundation::world_slot_mutation_lock();
        let transaction = ReplayTransactionGuard::new(&foundation_lease);
        let valid = unsafe {
            ffi::b3ValidateReplay(bytes.as_ptr().cast(), bytes.len() as i32, worker_count)
        };
        transaction.restore()?;
        Ok(valid)
    }
}

impl RecPlayer {
    /// Returns the world id owned by the replay player.
    pub fn world_id(&self) -> ReplayWorldId {
        self.player_owner().replay_world
    }

    /// Steps the replay by one recorded frame.
    pub fn step_frame(&mut self) -> Result<bool> {
        let stepped = self.with_native(|raw| unsafe { ffi::b3RecPlayer_StepFrame(raw) })?;
        if stepped {
            let resources = self.player_owner_mut().resources.get_mut();
            resources.bodies.clear();
            resources.shapes.clear();
        }
        Ok(stepped)
    }

    /// Advances one replay frame in two phases, pausing after body creation.
    pub fn sub_step_frame(&mut self) -> Result<()> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_SubStepFrame(raw) })?;
        let resources = self.player_owner_mut().resources.get_mut();
        resources.bodies.clear();
        resources.shapes.clear();
        Ok(())
    }

    /// Restarts replay from the first recorded frame.
    pub fn restart(&mut self) -> Result<()> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_Restart(raw) })?;
        let resources = self.player_owner_mut().resources.get_mut();
        resources.bodies.clear();
        resources.shapes.clear();
        Ok(())
    }

    /// Seeks replay to a recorded frame.
    pub fn seek_frame(&mut self, target_frame: i32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        if target_frame < 0 {
            return Err(validation::invalid(
                "rec_player.target_frame",
                InvalidValueReason::OutOfRange,
            ));
        }
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_SeekFrame(raw, target_frame) })?;
        let resources = self.player_owner_mut().resources.get_mut();
        resources.bodies.clear();
        resources.shapes.clear();
        Ok(())
    }

    /// Returns the current replay frame index.
    pub fn frame(&self) -> Result<i32> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrame(raw) })
    }

    /// Returns the number of recorded frames.
    pub fn frame_count(&self) -> Result<i32> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrameCount(raw) })
    }

    /// Returns whether replay has reached the end of the recording.
    pub fn is_at_end(&self) -> Result<bool> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_IsAtEnd(raw) })
    }

    /// Returns whether a sub-stepped replay is paused before the world step.
    pub fn is_at_pre_step(&self) -> Result<bool> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_IsAtPreStep(raw) })
    }

    /// Returns whether replay has diverged from the recorded state.
    pub fn has_diverged(&self) -> Result<bool> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_HasDiverged(raw) })
    }

    /// Returns metadata for the loaded recording.
    pub fn info(&self) -> Result<RecPlayerInfo> {
        Ok(RecPlayerInfo::from_raw(self.with_native(|raw| unsafe {
            ffi::b3RecPlayer_GetInfo(raw)
        })?))
    }

    /// Returns the first frame where replay diverged, if any.
    pub fn diverge_frame(&self) -> Result<Option<i32>> {
        let frame = self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetDivergeFrame(raw) })?;
        Ok((frame >= 0).then_some(frame))
    }

    /// Sets the worker count.
    pub fn set_worker_count(&mut self, count: i32) -> Result<()> {
        callback_state::check_not_in_callback()?;
        validate_replay_worker_count(count)?;
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_SetWorkerCount(raw, count) })?;
        Ok(())
    }

    /// Sets the keyframe policy.
    pub fn set_keyframe_policy(
        &mut self,
        budget_bytes: usize,
        min_interval_frames: i32,
    ) -> Result<()> {
        callback_state::check_not_in_callback()?;
        if min_interval_frames < 0 {
            return Err(validation::invalid(
                "rec_player.keyframe_min_interval",
                InvalidValueReason::OutOfRange,
            ));
        }
        self.with_native(|raw| unsafe {
            ffi::b3RecPlayer_SetKeyframePolicy(raw, budget_bytes, min_interval_frames)
        })?;
        Ok(())
    }

    /// Returns the configured replay keyframe memory budget.
    pub fn keyframe_budget(&self) -> Result<usize> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeBudget(raw) })
    }

    /// Returns the minimum frame interval between replay keyframes.
    pub fn keyframe_min_interval(&self) -> Result<i32> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeMinInterval(raw) })
    }

    /// Returns the current frame interval between replay keyframes.
    pub fn keyframe_interval(&self) -> Result<i32> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeInterval(raw) })
    }

    /// Returns the bytes currently used by replay keyframes.
    pub fn keyframe_bytes(&self) -> Result<usize> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetKeyframeBytes(raw) })
    }

    /// Returns the number of bodies in the replay world.
    pub fn body_count(&self) -> Result<i32> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetBodyCount(raw) })
    }

    /// Returns a body id from the replay world by index.
    pub fn body_id(&self, index: i32) -> Result<Option<BodyId>> {
        callback_state::check_not_in_callback()?;
        if index < 0 {
            return Err(validation::invalid(
                "rec_player.body_index",
                InvalidValueReason::OutOfRange,
            ));
        }
        let raw =
            self.with_native(|player| unsafe { ffi::b3RecPlayer_GetBodyId(player, index) })?;
        if raw.index1 == 0 {
            Ok(None)
        } else {
            Ok(Some(self.resolve_body(raw)?))
        }
    }

    /// Returns the number of queries captured for the current replay frame.
    pub fn frame_query_count(&self) -> Result<i32> {
        self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrameQueryCount(raw) })
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
            self.with_native(|raw| unsafe { ffi::b3RecPlayer_GetFrameQuery(raw, index) })?,
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
            })?,
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
        callback_state::check_not_in_callback()?;
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
        let owner = self.player_owner();
        let raw = owner.raw;
        with_replay_debug_draw(drawer, options, &owner.foundation_lease, |draw| {
            unsafe {
                ffi::b3RecPlayer_DrawFrameQueries(raw.as_ptr(), draw, query_index, selected_index)
            };
            Ok(())
        })
    }

    /// Destroys the native player, restores the Foundation scale, and reports restoration failure.
    pub fn close(mut self) -> Result<()> {
        callback_state::check_not_in_callback()?;
        let owner = self
            .owner
            .take()
            .expect("RecPlayer owner is present outside destruction");
        destroy_rec_player(owner)
    }
}

impl RecPlayer {
    fn with_native<R>(&self, operation: impl FnOnce(*mut ffi::b3RecPlayer) -> R) -> Result<R> {
        let owner = self.player_owner();
        let _call = owner.foundation_lease.enter_call()?;
        Ok(operation(owner.raw.as_ptr()))
    }

    fn resolve_body(&self, raw: ffi::b3BodyId) -> Result<BodyId> {
        let owner = self.player_owner();
        owner
            .resources
            .borrow_mut()
            .resolve_body(raw, owner.owner_token)
    }

    fn resolve_shape(&self, raw: ffi::b3ShapeId) -> Result<ShapeId> {
        let owner = self.player_owner();
        owner
            .resources
            .borrow_mut()
            .resolve_shape(raw, owner.owner_token)
    }

    fn player_owner(&self) -> &RecPlayerOwner {
        self.owner
            .as_ref()
            .expect("RecPlayer owner is present outside destruction")
    }

    fn player_owner_mut(&mut self) -> &mut RecPlayerOwner {
        self.owner
            .as_mut()
            .expect("RecPlayer owner is present outside destruction")
    }
}

impl Drop for RecPlayer {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let cleanup = move || destroy_rec_player(owner);
        if callback_state::in_callback() {
            callback_state::defer_local_cleanup_or_retain(move || {
                let _ = cleanup();
            });
        } else {
            let _ = cleanup();
        }
    }
}

fn destroy_rec_player(owner: RecPlayerOwner) -> Result<()> {
    let owner = callback_state::RetainOnUnwind::new(owner);
    let foundation = owner.foundation_lease.foundation();
    let owner_call_frame = callback_state::OwnerCallFrame::enter();
    let result = {
        let _slot_guard = crate::core::foundation::world_slot_mutation_lock();
        unsafe { ffi::b3RecPlayer_Destroy(owner.raw.as_ptr()) };
        units::restore_foundation_scale(&owner.foundation_lease)
    };
    drop(owner_call_frame);
    let result = result.and_then(|()| foundation.ensure_healthy());
    owner.finish();
    result
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
    validate_length_units("recording.length_scale", length_units)?;
    Ok(length_units)
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
    fn replay_indices_preserve_context() {
        assert_eq!(
            checked_optional_index("rec_player.query_index", Some(-1)),
            Err(Error::InvalidValue {
                context: "rec_player.query_index",
                reason: InvalidValueReason::OutOfRange,
            })
        );
    }
}
