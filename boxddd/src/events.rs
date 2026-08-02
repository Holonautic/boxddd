use crate::core::callback_state;
use crate::core::provenance::ContactEpoch;
use crate::error::{Error, Result};
use crate::types::{BodyId, ContactData, ContactId, JointId, Pos, ShapeId, Vec3, WorldTransform};
use crate::world::World;
use boxddd_sys::ffi;
use std::cell::{RefCell, RefMut};
use std::ffi::c_void;

#[derive(Copy, Clone)]
/// Borrowed view of a body movement event from the latest world step.
///
/// The view borrows Box3D's transient event buffer and is only valid for the
/// callback passed to `with_body_events_view`. Box3D reports bodies moved by
/// simulation here, not bodies moved directly by the user.
pub struct BodyMove<'a> {
    raw: &'a ffi::b3BodyMoveEvent,
    world: &'a World,
}

impl BodyMove<'_> {
    /// Returns the body that moved.
    pub fn body_id(&self) -> Result<BodyId> {
        self.world
            .state()
            .ledger
            .resolve_observed_body(self.raw.bodyId)
    }

    /// Returns the body's world transform after the movement.
    pub fn transform(&self) -> WorldTransform {
        WorldTransform::from_raw(self.raw.transform)
    }

    /// Returns true when the body went to sleep during the step.
    pub fn fell_asleep(&self) -> bool {
        self.raw.fellAsleep
    }

    /// Returns the raw Box3D `userData` pointer value observed in this event.
    ///
    /// The pointer is untyped and not owned by `boxddd`; interpreting or dereferencing it is the
    /// caller's unsafe interop responsibility.
    pub fn raw_user_data(&self) -> *mut c_void {
        self.raw.userData
    }
}

/// Iterator over borrowed body movement events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_body_events_view` closure.
pub struct BodyMoveIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3BodyMoveEvent>,
    world: &'a World,
}

impl<'a> Iterator for BodyMoveIter<'a> {
    type Item = BodyMove<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| BodyMove {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Owned body movement event snapshot.
///
/// Box3D reports bodies moved by simulation here, not bodies moved directly by
/// the user.
pub struct BodyMoveEvent {
    /// Body that moved.
    pub body_id: BodyId,
    /// Body transform after the movement.
    pub transform: WorldTransform,
    /// Whether the body went to sleep during the step.
    pub fell_asleep: bool,
    /// Raw Box3D `userData` pointer value observed in the event.
    pub raw_user_data: *mut c_void,
}

#[derive(Copy, Clone)]
/// Borrowed view of a sensor begin-touch event.
///
/// This view borrows Box3D's transient event buffer and is only valid inside
/// the `with_sensor_events_view` closure that produced it.
pub struct SensorBeginTouch<'a> {
    raw: &'a ffi::b3SensorBeginTouchEvent,
    world: &'a World,
}

impl SensorBeginTouch<'_> {
    /// Returns the sensor shape receiving the touch.
    pub fn sensor_shape(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.sensorShapeId)
    }

    /// Returns the non-sensor shape entering the sensor.
    pub fn visitor_shape(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.visitorShapeId)
    }
}

#[derive(Copy, Clone)]
/// Borrowed view of a sensor end-touch event.
///
/// This view borrows Box3D's transient event buffer and is only valid inside
/// the `with_sensor_events_view` closure that produced it. End-touch ids retain
/// the provenance of destroyed shapes, but using those ids in world operations
/// still returns a stale-handle error.
pub struct SensorEndTouch<'a> {
    raw: &'a ffi::b3SensorEndTouchEvent,
    world: &'a World,
}

impl SensorEndTouch<'_> {
    /// Returns the sensor shape ending the touch.
    pub fn sensor_shape(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.sensorShapeId)
    }

    /// Returns the non-sensor shape leaving the sensor.
    pub fn visitor_shape(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.visitorShapeId)
    }
}

/// Iterator over borrowed sensor begin-touch events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_sensor_events_view` closure.
pub struct SensorBeginIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3SensorBeginTouchEvent>,
    world: &'a World,
}

impl<'a> Iterator for SensorBeginIter<'a> {
    type Item = SensorBeginTouch<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| SensorBeginTouch {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// Iterator over borrowed sensor end-touch events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_sensor_events_view` closure.
pub struct SensorEndIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3SensorEndTouchEvent>,
    world: &'a World,
}

impl<'a> Iterator for SensorEndIter<'a> {
    type Item = SensorEndTouch<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| SensorEndTouch {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Owned sensor begin-touch event snapshot.
pub struct SensorBeginTouchEvent {
    /// Sensor shape receiving the touch.
    pub sensor_shape: ShapeId,
    /// Shape entering the sensor.
    pub visitor_shape: ShapeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Owned sensor end-touch event snapshot.
pub struct SensorEndTouchEvent {
    /// Sensor shape ending the touch.
    pub sensor_shape: ShapeId,
    /// Shape leaving the sensor.
    pub visitor_shape: ShapeId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Owned sensor event snapshot for the latest world step.
pub struct SensorEvents {
    /// Sensor touches that began during the step.
    pub begin: Vec<SensorBeginTouchEvent>,
    /// Sensor touches that ended during the step.
    pub end: Vec<SensorEndTouchEvent>,
}

impl SensorEvents {
    fn clear_buffers(&mut self) {
        self.begin.clear();
        self.end.clear();
    }

    fn commit_buffers(out: &mut Self, scratch: &mut Self) {
        crate::core::ffi_vec::commit_scratch(&mut out.begin, &mut scratch.begin);
        crate::core::ffi_vec::commit_scratch(&mut out.end, &mut scratch.end);
    }
}

#[derive(Copy, Clone)]
/// Borrowed view of a contact begin-touch event.
///
/// This view borrows Box3D's transient event buffer and is only valid inside
/// the `with_contact_events_view` closure that produced it.
pub struct ContactBeginTouch<'a> {
    raw: &'a ffi::b3ContactBeginTouchEvent,
    world: &'a World,
}

impl ContactBeginTouch<'_> {
    /// Returns the first shape in the contact pair.
    pub fn shape_a(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.shapeIdA)
    }

    /// Returns the second shape in the contact pair.
    pub fn shape_b(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.shapeIdB)
    }

    /// Returns the transient contact identifier for the pair.
    ///
    /// Validate the id before later use; Box3D may destroy contacts when the
    /// world is stepped or modified.
    pub fn contact_id(&self) -> Result<ContactId> {
        self.world
            .state()
            .ledger
            .resolve_contact(self.raw.contactId)
    }
}

#[derive(Copy, Clone)]
/// Borrowed view of a contact end-touch event.
///
/// This view borrows Box3D's transient event buffer and is only valid inside
/// the `with_contact_events_view` closure that produced it. End-touch shape and
/// contact ids preserve retired provenance and are stale for world operations.
pub struct ContactEndTouch<'a> {
    raw: &'a ffi::b3ContactEndTouchEvent,
    world: &'a World,
}

impl ContactEndTouch<'_> {
    /// Returns the first shape in the contact pair.
    ///
    /// The shape may have been destroyed before this event is consumed. Its id
    /// still identifies that retired shape and is stale for world operations.
    pub fn shape_a(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.shapeIdA)
    }

    /// Returns the second shape in the contact pair.
    ///
    /// The shape may have been destroyed before this event is consumed. Its id
    /// still identifies that retired shape and is stale for world operations.
    pub fn shape_b(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.shapeIdB)
    }

    /// Returns the transient contact identifier for the pair.
    ///
    /// The contact may have been destroyed before this event is consumed. Its
    /// id still identifies the retired contact and is stale for world operations.
    pub fn contact_id(&self) -> Result<ContactId> {
        self.world
            .state()
            .ledger
            .resolve_contact_end(self.raw.contactId)
    }
}

#[derive(Copy, Clone)]
/// Borrowed view of a contact hit event.
///
/// This view borrows Box3D's transient event buffer and is only valid inside
/// the `with_contact_events_view` closure that produced it. Hit events can be
/// reported for speculative contacts that later receive a confirmed impulse.
pub struct ContactHit<'a> {
    raw: &'a ffi::b3ContactHitEvent,
    world: &'a World,
}

impl ContactHit<'_> {
    /// Returns the first shape in the contact pair.
    pub fn shape_a(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.shapeIdA)
    }

    /// Returns the second shape in the contact pair.
    pub fn shape_b(&self) -> Result<ShapeId> {
        self.world
            .state()
            .ledger
            .resolve_observed_shape(self.raw.shapeIdB)
    }

    /// Returns the transient contact identifier for the pair.
    pub fn contact_id(&self) -> Result<ContactId> {
        self.world
            .state()
            .ledger
            .resolve_contact(self.raw.contactId)
    }

    /// Returns the hit point in world coordinates.
    pub fn point(&self) -> Pos {
        Pos::from_raw(self.raw.point)
    }

    /// Returns the hit normal pointing from shape A to shape B.
    pub fn normal(&self) -> Vec3 {
        Vec3::from_raw(self.raw.normal)
    }

    /// Returns the relative approach speed at impact.
    ///
    /// Box3D reports this as a positive speed, typically in meters per second.
    pub fn approach_speed(&self) -> f32 {
        self.raw.approachSpeed
    }

    /// Returns the user material identifier for shape A.
    pub fn user_material_id_a(&self) -> u64 {
        self.raw.userMaterialIdA
    }

    /// Returns the user material identifier for shape B.
    pub fn user_material_id_b(&self) -> u64 {
        self.raw.userMaterialIdB
    }
}

/// Iterator over borrowed contact begin-touch events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_contact_events_view` closure.
pub struct ContactBeginIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3ContactBeginTouchEvent>,
    world: &'a World,
}

impl<'a> Iterator for ContactBeginIter<'a> {
    type Item = ContactBeginTouch<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| ContactBeginTouch {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// Iterator over borrowed contact end-touch events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_contact_events_view` closure.
pub struct ContactEndIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3ContactEndTouchEvent>,
    world: &'a World,
}

impl<'a> Iterator for ContactEndIter<'a> {
    type Item = ContactEndTouch<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| ContactEndTouch {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// Iterator over borrowed contact hit events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_contact_events_view` closure.
pub struct ContactHitIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3ContactHitEvent>,
    world: &'a World,
}

impl<'a> Iterator for ContactHitIter<'a> {
    type Item = ContactHit<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| ContactHit {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Owned contact begin-touch event snapshot.
pub struct ContactBeginTouchEvent {
    /// First shape in the contact pair.
    pub shape_a: ShapeId,
    /// Second shape in the contact pair.
    pub shape_b: ShapeId,
    /// Contact identifier for the pair.
    pub contact_id: ContactId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Owned contact end-touch event snapshot.
pub struct ContactEndTouchEvent {
    /// First shape in the contact pair.
    pub shape_a: ShapeId,
    /// Second shape in the contact pair.
    pub shape_b: ShapeId,
    /// Contact identifier for the pair.
    pub contact_id: ContactId,
}

#[derive(Clone, Debug, PartialEq)]
/// Owned contact hit event snapshot.
pub struct ContactHitEvent {
    /// First shape in the contact pair.
    pub shape_a: ShapeId,
    /// Second shape in the contact pair.
    pub shape_b: ShapeId,
    /// Contact identifier for the pair.
    pub contact_id: ContactId,
    /// Hit point in world coordinates.
    pub point: Pos,
    /// Hit normal.
    pub normal: Vec3,
    /// Relative approach speed at impact.
    pub approach_speed: f32,
    /// User material identifier for shape A.
    pub user_material_id_a: u64,
    /// User material identifier for shape B.
    pub user_material_id_b: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
/// Owned contact event snapshot for the latest world step.
pub struct ContactEvents {
    /// Contacts that began touching during the step.
    pub begin: Vec<ContactBeginTouchEvent>,
    /// Contacts that stopped touching during the step.
    pub end: Vec<ContactEndTouchEvent>,
    /// Contact hit events reported during the step.
    pub hit: Vec<ContactHitEvent>,
}

impl ContactEvents {
    fn clear_buffers(&mut self) {
        self.begin.clear();
        self.end.clear();
        self.hit.clear();
    }

    fn commit_buffers(out: &mut Self, scratch: &mut Self) {
        crate::core::ffi_vec::commit_scratch(&mut out.begin, &mut scratch.begin);
        crate::core::ffi_vec::commit_scratch(&mut out.end, &mut scratch.end);
        crate::core::ffi_vec::commit_scratch(&mut out.hit, &mut scratch.hit);
    }
}

#[derive(Copy, Clone)]
/// Borrowed view of a joint event.
///
/// This view borrows Box3D's transient event buffer and is only valid inside
/// the `with_joint_events_view` closure that produced it.
pub struct JointEventView<'a> {
    raw: &'a ffi::b3JointEvent,
    world: &'a World,
}

impl JointEventView<'_> {
    /// Returns the joint that generated the event.
    pub fn joint_id(&self) -> Result<JointId> {
        self.world
            .state()
            .ledger
            .resolve_observed_joint(self.raw.jointId)
    }

    /// Returns the raw Box3D `userData` pointer value observed in this event.
    ///
    /// The pointer is untyped and not owned by `boxddd`; interpreting or dereferencing it is the
    /// caller's unsafe interop responsibility.
    pub fn raw_user_data(&self) -> *mut c_void {
        self.raw.userData
    }
}

/// Iterator over borrowed joint events.
///
/// The iterator borrows Box3D's transient event buffer and cannot safely
/// outlive the `with_joint_events_view` closure.
pub struct JointEventIter<'a> {
    inner: std::slice::Iter<'a, ffi::b3JointEvent>,
    world: &'a World,
}

impl<'a> Iterator for JointEventIter<'a> {
    type Item = JointEventView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|raw| JointEventView {
            raw,
            world: self.world,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Owned joint event snapshot.
pub struct JointEvent {
    /// Joint that generated the event.
    pub joint_id: JointId,
    /// Raw Box3D `userData` pointer value observed in the event.
    pub raw_user_data: *mut c_void,
}

#[derive(Debug, Default)]
pub(crate) struct EventScratch {
    body: RefCell<Vec<BodyMoveEvent>>,
    sensor: RefCell<SensorEvents>,
    contact: RefCell<ContactEvents>,
    joint: RefCell<Vec<JointEvent>>,
}

fn borrow_scratch<T>(scratch: &RefCell<T>) -> Result<RefMut<'_, T>> {
    scratch.try_borrow_mut().map_err(|_| Error::NativeFailure)
}

fn raw_event_slice<'a, T>(ptr: *const T, count: i32) -> &'a [T] {
    if count > 0 && !ptr.is_null() {
        unsafe { std::slice::from_raw_parts(ptr, count as usize) }
    } else {
        &[]
    }
}

fn body_event_slice<'a>(raw: ffi::b3BodyEvents) -> &'a [ffi::b3BodyMoveEvent] {
    raw_event_slice(raw.moveEvents, raw.moveCount)
}

fn sensor_begin_slice<'a>(raw: ffi::b3SensorEvents) -> &'a [ffi::b3SensorBeginTouchEvent] {
    raw_event_slice(raw.beginEvents, raw.beginCount)
}

fn sensor_end_slice<'a>(raw: ffi::b3SensorEvents) -> &'a [ffi::b3SensorEndTouchEvent] {
    raw_event_slice(raw.endEvents, raw.endCount)
}

fn contact_begin_slice<'a>(raw: ffi::b3ContactEvents) -> &'a [ffi::b3ContactBeginTouchEvent] {
    raw_event_slice(raw.beginEvents, raw.beginCount)
}

fn contact_end_slice<'a>(raw: ffi::b3ContactEvents) -> &'a [ffi::b3ContactEndTouchEvent] {
    raw_event_slice(raw.endEvents, raw.endCount)
}

fn contact_hit_slice<'a>(raw: ffi::b3ContactEvents) -> &'a [ffi::b3ContactHitEvent] {
    raw_event_slice(raw.hitEvents, raw.hitCount)
}

fn joint_event_slice<'a>(raw: ffi::b3JointEvents) -> &'a [ffi::b3JointEvent] {
    raw_event_slice(raw.jointEvents, raw.count)
}

impl World {
    fn body_event_snapshot(&self, event: &ffi::b3BodyMoveEvent) -> Result<BodyMoveEvent> {
        Ok(BodyMoveEvent {
            body_id: self.state().ledger.resolve_observed_body(event.bodyId)?,
            transform: WorldTransform::from_raw(event.transform),
            fell_asleep: event.fellAsleep,
            raw_user_data: event.userData,
        })
    }

    fn fill_body_events(
        &self,
        out: &mut Vec<BodyMoveEvent>,
        events: &[ffi::b3BodyMoveEvent],
    ) -> Result<()> {
        crate::core::ffi_vec::map_into_scratch(out, events, |event| self.body_event_snapshot(event))
    }

    fn fill_sensor_events(
        &self,
        out: &mut SensorEvents,
        begin: &[ffi::b3SensorBeginTouchEvent],
        end: &[ffi::b3SensorEndTouchEvent],
    ) -> Result<()> {
        crate::core::ffi_vec::map_into_scratch(&mut out.begin, begin, |event| {
            Ok(SensorBeginTouchEvent {
                sensor_shape: self
                    .state()
                    .ledger
                    .resolve_observed_shape(event.sensorShapeId)?,
                visitor_shape: self
                    .state()
                    .ledger
                    .resolve_observed_shape(event.visitorShapeId)?,
            })
        })?;
        crate::core::ffi_vec::map_into_scratch(&mut out.end, end, |event| {
            Ok(SensorEndTouchEvent {
                sensor_shape: self
                    .state()
                    .ledger
                    .resolve_observed_shape(event.sensorShapeId)?,
                visitor_shape: self
                    .state()
                    .ledger
                    .resolve_observed_shape(event.visitorShapeId)?,
            })
        })
    }

    fn fill_contact_events(
        &self,
        out: &mut ContactEvents,
        begin: &[ffi::b3ContactBeginTouchEvent],
        end: &[ffi::b3ContactEndTouchEvent],
        hit: &[ffi::b3ContactHitEvent],
    ) -> Result<()> {
        crate::core::ffi_vec::map_into_scratch(&mut out.begin, begin, |event| {
            Ok(ContactBeginTouchEvent {
                shape_a: self.state().ledger.resolve_observed_shape(event.shapeIdA)?,
                shape_b: self.state().ledger.resolve_observed_shape(event.shapeIdB)?,
                contact_id: self.state().ledger.resolve_contact(event.contactId)?,
            })
        })?;
        crate::core::ffi_vec::map_into_scratch(&mut out.end, end, |event| {
            Ok(ContactEndTouchEvent {
                shape_a: self.state().ledger.resolve_observed_shape(event.shapeIdA)?,
                shape_b: self.state().ledger.resolve_observed_shape(event.shapeIdB)?,
                contact_id: self.state().ledger.resolve_contact_end(event.contactId)?,
            })
        })?;
        crate::core::ffi_vec::map_into_scratch(&mut out.hit, hit, |event| {
            Ok(ContactHitEvent {
                shape_a: self.state().ledger.resolve_observed_shape(event.shapeIdA)?,
                shape_b: self.state().ledger.resolve_observed_shape(event.shapeIdB)?,
                contact_id: self.state().ledger.resolve_contact(event.contactId)?,
                point: Pos::from_raw(event.point),
                normal: Vec3::from_raw(event.normal),
                approach_speed: event.approachSpeed,
                user_material_id_a: event.userMaterialIdA,
                user_material_id_b: event.userMaterialIdB,
            })
        })
    }

    fn fill_joint_events(
        &self,
        out: &mut Vec<JointEvent>,
        events: &[ffi::b3JointEvent],
    ) -> Result<()> {
        crate::core::ffi_vec::map_into_scratch(out, events, |event| {
            self.joint_event_snapshot(event)
        })
    }

    fn joint_event_snapshot(&self, event: &ffi::b3JointEvent) -> Result<JointEvent> {
        Ok(JointEvent {
            joint_id: self.state().ledger.resolve_observed_joint(event.jointId)?,
            raw_user_data: event.userData,
        })
    }

    pub(crate) fn finish_step_provenance(&mut self, next_epoch: ContactEpoch) {
        self.state_mut().ledger.finish_step(next_epoch);
    }

    pub(crate) fn finish_contact_turnover(
        &mut self,
        next_epoch: crate::core::provenance::ContactEpoch,
    ) {
        self.state_mut().ledger.finish_contact_turnover(next_epoch);
    }

    /// Returns owned body movement events from the latest completed world step.
    ///
    /// Box3D stores events in transient step-local arrays. Unlike
    /// [`Self::with_body_events_view`], this allocates a snapshot that remains
    /// valid after later steps or world mutations.
    pub fn body_events(&self) -> Result<Vec<BodyMoveEvent>> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetBodyEvents(self.raw()) };
        let mut out = Vec::new();
        self.fill_body_events(&mut out, body_event_slice(raw))?;
        Ok(out)
    }

    /// Writes owned body movement events into `out` as one transaction.
    ///
    /// Existing capacity is reused after warmup, and every copied event is independent from
    /// Box3D's transient event buffer. If conversion fails, `out` and its allocation are left
    /// unchanged.
    pub fn body_events_into(&self, out: &mut Vec<BodyMoveEvent>) -> Result<()> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetBodyEvents(self.raw()) };
        let events = body_event_slice(raw);
        let mut scratch = borrow_scratch(&self.state().event_scratch.body)?;
        crate::core::ffi_vec::map_into_scratch_transactional(out, &mut scratch, events, |event| {
            self.body_event_snapshot(event)
        })
    }

    /// Borrows body movement events for the duration of `f`.
    ///
    /// Use this when you want to avoid allocation and only need to inspect Box3D's
    /// transient event buffer inside the closure. Safe Rust cannot return the borrowed
    /// iterator from the closure:
    ///
    /// ```compile_fail
    /// use boxddd::{BodyMoveIter, World};
    ///
    /// fn leak_events(world: &World) -> BodyMoveIter<'_> {
    ///     world.with_body_events_view(|events| events)
    /// }
    /// ```
    pub fn with_body_events_view<T>(&self, f: impl FnOnce(BodyMoveIter<'_>) -> T) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetBodyEvents(self.raw()) };
        let events = body_event_slice(raw);
        drop(_call);
        Ok(f(BodyMoveIter {
            inner: events.iter(),
            world: self,
        }))
    }

    /// Borrows raw Box3D body movement events for the duration of `f`.
    ///
    /// # Safety
    ///
    /// The raw event records contain untyped `userData` pointers owned by the application.
    /// The callback must not copy out raw event pointers, retain references to the slice, or
    /// dereference `userData` unless it can uphold the original pointer validity and aliasing
    /// requirements. Box3D may invalidate the native buffer after a later step or world
    /// mutation.
    pub unsafe fn with_body_events_raw<T>(
        &self,
        f: impl FnOnce(&[ffi::b3BodyMoveEvent]) -> T,
    ) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetBodyEvents(self.raw()) };
        let events = body_event_slice(raw);
        drop(_call);
        Ok(f(events))
    }

    /// Returns owned sensor events from the latest completed world step.
    ///
    /// End-touch shape identifiers retain the provenance of shapes destroyed or
    /// invalidated around the step. Such ids are stale for world operations.
    pub fn sensor_events(&self) -> Result<SensorEvents> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetSensorEvents(self.raw()) };
        let mut out = SensorEvents::default();
        self.fill_sensor_events(&mut out, sensor_begin_slice(raw), sensor_end_slice(raw))?;
        Ok(out)
    }

    /// Writes owned sensor events into `out` as one transaction.
    ///
    /// If any begin or end event cannot be copied, every vector in `out` and its allocation are
    /// left unchanged.
    pub fn sensor_events_into(&self, out: &mut SensorEvents) -> Result<()> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetSensorEvents(self.raw()) };
        let begin = sensor_begin_slice(raw);
        let end = sensor_end_slice(raw);
        let mut scratch = borrow_scratch(&self.state().event_scratch.sensor)?;
        crate::core::ffi_vec::replace_from_scratch_transactional(
            out,
            &mut scratch,
            SensorEvents::clear_buffers,
            |scratch| self.fill_sensor_events(scratch, begin, end),
            SensorEvents::commit_buffers,
        )
    }

    /// Borrows sensor begin and end events for the duration of `f`.
    ///
    /// The borrowed iterators view Box3D's transient event arrays and cannot safely escape
    /// the closure. Use [`Self::sensor_events`] when events must be stored or processed
    /// after additional world mutations.
    ///
    /// ```compile_fail
    /// use boxddd::{SensorBeginIter, World};
    ///
    /// fn leak_sensor_events(world: &World) -> SensorBeginIter<'_> {
    ///     world.with_sensor_events_view(|begin, _end| begin)
    /// }
    /// ```
    pub fn with_sensor_events_view<T>(
        &self,
        f: impl FnOnce(SensorBeginIter<'_>, SensorEndIter<'_>) -> T,
    ) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetSensorEvents(self.raw()) };
        let begin = sensor_begin_slice(raw);
        let end = sensor_end_slice(raw);
        drop(_call);
        Ok(f(
            SensorBeginIter {
                inner: begin.iter(),
                world: self,
            },
            SensorEndIter {
                inner: end.iter(),
                world: self,
            },
        ))
    }

    /// Borrows raw Box3D sensor events for the duration of `f`.
    ///
    /// # Safety
    ///
    /// The callback must not copy out raw event pointers or retain references to the slices
    /// beyond its call. The raw records must be interpreted according to Box3D's event
    /// layout, including the possibility that end-touch ids are already invalid.
    pub unsafe fn with_sensor_events_raw<T>(
        &self,
        f: impl FnOnce(&[ffi::b3SensorBeginTouchEvent], &[ffi::b3SensorEndTouchEvent]) -> T,
    ) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetSensorEvents(self.raw()) };
        let begin = sensor_begin_slice(raw);
        let end = sensor_end_slice(raw);
        drop(_call);
        Ok(f(begin, end))
    }

    /// Returns owned contact events from the latest completed world step.
    ///
    /// End-touch contact and shape ids retain their retired provenance for the
    /// current event window and are stale for world operations.
    pub fn contact_events(&self) -> Result<ContactEvents> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetContactEvents(self.raw()) };
        let mut out = ContactEvents::default();
        self.fill_contact_events(
            &mut out,
            contact_begin_slice(raw),
            contact_end_slice(raw),
            contact_hit_slice(raw),
        )?;
        Ok(out)
    }

    /// Returns the current data for a live contact.
    ///
    /// Returns an error when `contact_id` is stale or belongs to a different world.
    pub fn contact_data(&self, contact_id: ContactId) -> Result<ContactData> {
        callback_state::check_not_in_callback()?;
        let raw_contact = self.state().ledger.authorize_contact(contact_id)?;
        let _call = self.enter_world_call()?;
        crate::core::debug_checks::check_contact_valid_raw(raw_contact)?;
        let raw = unsafe { ffi::b3Contact_GetData(contact_id.into_raw()) };
        let shape_id_a = self.state().ledger.resolve_shape(raw.shapeIdA)?;
        let shape_id_b = self.state().ledger.resolve_shape(raw.shapeIdB)?;
        Ok(unsafe { ContactData::from_raw_parts(raw, contact_id, shape_id_a, shape_id_b) })
    }

    /// Writes owned contact events into `out` as one transaction.
    ///
    /// If any begin, end, or hit event cannot be copied, every vector in `out` and its allocation
    /// are left unchanged.
    pub fn contact_events_into(&self, out: &mut ContactEvents) -> Result<()> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetContactEvents(self.raw()) };
        let begin = contact_begin_slice(raw);
        let end = contact_end_slice(raw);
        let hit = contact_hit_slice(raw);
        let mut scratch = borrow_scratch(&self.state().event_scratch.contact)?;
        crate::core::ffi_vec::replace_from_scratch_transactional(
            out,
            &mut scratch,
            ContactEvents::clear_buffers,
            |scratch| self.fill_contact_events(scratch, begin, end, hit),
            ContactEvents::commit_buffers,
        )
    }

    /// Borrows contact begin, end, and hit events for the duration of `f`.
    ///
    /// The borrowed iterators view Box3D's transient event arrays. Use
    /// [`Self::contact_events`] when events must outlive the closure or be processed
    /// after additional world mutations.
    ///
    /// ```compile_fail
    /// use boxddd::{ContactBeginIter, World};
    ///
    /// fn leak_contact_events(world: &World) -> ContactBeginIter<'_> {
    ///     world.with_contact_events_view(|begin, _end, _hit| begin)
    /// }
    /// ```
    pub fn with_contact_events_view<T>(
        &self,
        f: impl FnOnce(ContactBeginIter<'_>, ContactEndIter<'_>, ContactHitIter<'_>) -> T,
    ) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetContactEvents(self.raw()) };
        let begin = contact_begin_slice(raw);
        let end = contact_end_slice(raw);
        let hit = contact_hit_slice(raw);
        drop(_call);
        Ok(f(
            ContactBeginIter {
                inner: begin.iter(),
                world: self,
            },
            ContactEndIter {
                inner: end.iter(),
                world: self,
            },
            ContactHitIter {
                inner: hit.iter(),
                world: self,
            },
        ))
    }

    /// Borrows raw Box3D contact events for the duration of `f`.
    ///
    /// # Safety
    ///
    /// The callback must not copy out raw event pointers or retain references to the slices
    /// beyond its call. The raw records must be interpreted according to Box3D's event
    /// layout, including transient contact ids and application-owned material ids.
    pub unsafe fn with_contact_events_raw<T>(
        &self,
        f: impl FnOnce(
            &[ffi::b3ContactBeginTouchEvent],
            &[ffi::b3ContactEndTouchEvent],
            &[ffi::b3ContactHitEvent],
        ) -> T,
    ) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetContactEvents(self.raw()) };
        let begin = contact_begin_slice(raw);
        let end = contact_end_slice(raw);
        let hit = contact_hit_slice(raw);
        drop(_call);
        Ok(f(begin, end, hit))
    }

    /// Returns owned joint events from the latest completed world step.
    ///
    /// Joint events report awake joints that exceeded their configured force or
    /// torque thresholds. Box3D does not include the observed force or torque.
    pub fn joint_events(&self) -> Result<Vec<JointEvent>> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetJointEvents(self.raw()) };
        let mut out = Vec::new();
        self.fill_joint_events(&mut out, joint_event_slice(raw))?;
        Ok(out)
    }

    /// Writes owned joint events into `out`, leaving it unchanged if conversion fails.
    pub fn joint_events_into(&self, out: &mut Vec<JointEvent>) -> Result<()> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetJointEvents(self.raw()) };
        let events = joint_event_slice(raw);
        let mut scratch = borrow_scratch(&self.state().event_scratch.joint)?;
        crate::core::ffi_vec::map_into_scratch_transactional(out, &mut scratch, events, |event| {
            self.joint_event_snapshot(event)
        })
    }

    /// Borrows joint events for the duration of `f`.
    ///
    /// The borrowed iterator views Box3D's transient joint event array. Use
    /// [`Self::joint_events`] when events must be stored after the closure.
    ///
    /// ```compile_fail
    /// use boxddd::{JointEventIter, World};
    ///
    /// fn leak_joint_events(world: &World) -> JointEventIter<'_> {
    ///     world.with_joint_events_view(|events| events)
    /// }
    /// ```
    pub fn with_joint_events_view<T>(&self, f: impl FnOnce(JointEventIter<'_>) -> T) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetJointEvents(self.raw()) };
        let events = joint_event_slice(raw);
        drop(_call);
        Ok(f(JointEventIter {
            inner: events.iter(),
            world: self,
        }))
    }

    /// Borrows raw Box3D joint events for the duration of `f`.
    ///
    /// # Safety
    ///
    /// The callback must not copy out raw event pointers or retain references to the slice
    /// beyond its call. Raw `userData` pointers are application-owned and must only be
    /// dereferenced when their validity is known.
    pub unsafe fn with_joint_events_raw<T>(
        &self,
        f: impl FnOnce(&[ffi::b3JointEvent]) -> T,
    ) -> Result<T> {
        let _call = self.enter_world_call()?;
        let raw = unsafe { ffi::b3World_GetJointEvents(self.raw()) };
        let events = joint_event_slice(raw);
        drop(_call);
        Ok(f(events))
    }
}
