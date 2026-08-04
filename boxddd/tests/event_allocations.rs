use boxddd::{
    BodyId, BodyType, BoxHull, ContactEvents, DistanceJointDef, Foundation, SensorEvents, Sphere,
    Vec3, World,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct GuardedAllocator;

static COUNT_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: GuardedAllocator = GuardedAllocator;

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

unsafe impl GlobalAlloc for GuardedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let allocation = unsafe { System.alloc(layout) };
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        allocation
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let allocation = unsafe { System.alloc_zeroed(layout) };
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        allocation
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let allocation = unsafe { System.realloc(ptr, layout, new_size) };
        if COUNT_ALLOCATIONS.load(Ordering::Relaxed) {
            ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        allocation
    }
}

fn allocations_during<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATION_COUNT.store(0, Ordering::Relaxed);
    assert!(!COUNT_ALLOCATIONS.swap(true, Ordering::SeqCst));
    let result = f();
    COUNT_ALLOCATIONS.store(false, Ordering::SeqCst);
    let allocations = ALLOCATION_COUNT.load(Ordering::Relaxed);
    (result, allocations)
}

fn body_event_count(world: &World) -> usize {
    world
        .with_body_events_view(|events| events.count())
        .unwrap()
}

fn sensor_event_count(world: &World) -> usize {
    world
        .with_sensor_events_view(|begin, end| begin.count() + end.count())
        .unwrap()
}

fn contact_event_count(world: &World) -> usize {
    world
        .with_contact_events_view(|begin, end, hit| begin.count() + end.count() + hit.count())
        .unwrap()
}

fn resolve_contact_event_ids(world: &World) {
    world
        .with_contact_events_view(|begin, end, hit| {
            for event in begin {
                event.shape_a()?;
                event.shape_b()?;
                event.contact_id()?;
            }
            for event in end {
                event.shape_a()?;
                event.shape_b()?;
                event.contact_id()?;
            }
            for event in hit {
                event.shape_a()?;
                event.shape_b()?;
                event.contact_id()?;
            }
            Ok::<_, boxddd::Error>(())
        })
        .unwrap()
        .unwrap();
}

fn joint_event_count(world: &World) -> usize {
    world
        .with_joint_events_view(|events| events.count())
        .unwrap()
}

fn create_body_event_body(world: &mut World, x: f32) -> BodyId {
    let foundation = foundation();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([x, 0.0, 0.0])
                .linear_velocity([1.0, 0.0, 0.0])
                .gravity_scale(0.0)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.25),
        )
        .unwrap();
    body
}

fn create_sensor_visitor(world: &mut World, x: f32) {
    let foundation = foundation();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([x, 0.0, 0.0])
                .gravity_scale(0.0)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .enable_sensor_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.25),
        )
        .unwrap();
}

fn create_contact_body(world: &mut World, x: f32) {
    let foundation = foundation();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([x, 0.9, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .enable_contact_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
}

fn create_joint_pair(world: &mut World, x: f32) -> BodyId {
    let foundation = foundation();
    let anchor = world
        .create_body(
            foundation
                .body_def_builder()
                .position([x, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([x + 1.0, 0.0, 0.0])
                .gravity_scale(0.0)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.25),
        )
        .unwrap();
    world
        .create_distance_joint(
            DistanceJointDef::new(anchor, body)
                .length(1.0)
                .force_threshold(0.0)
                .torque_threshold(0.0),
        )
        .unwrap();
    body
}

fn assert_variable_counts(initial: usize, smaller: usize, larger: usize, family: &str) {
    assert!(initial >= 3, "{family} did not produce the grow window");
    assert!(
        smaller > 0 && smaller < initial,
        "{family} did not produce a smaller non-empty window: {initial} -> {smaller}"
    );
    assert!(
        larger > smaller && larger <= initial,
        "{family} did not grow within capacity: {smaller} -> {larger}, initial {initial}"
    );
}

fn assert_no_allocations(result: boxddd::Result<()>, allocations: usize, operation: &str) {
    result.unwrap();
    assert_eq!(allocations, 0, "{operation} allocated after warmup");
}

#[test]
fn warmed_event_into_calls_reuse_capacity_across_variable_non_empty_windows() {
    {
        let foundation = foundation();
        let mut world = foundation
            .create_world(
                foundation
                    .world_def_builder()
                    .gravity(Vec3::ZERO)
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let bodies = [
            create_body_event_body(&mut world, -4.0),
            create_body_event_body(&mut world, 0.0),
            create_body_event_body(&mut world, 4.0),
        ];
        world.step(1.0 / 60.0, 4).unwrap();
        let initial = body_event_count(&world);
        let mut events = Vec::new();
        world.body_events_into(&mut events).unwrap();
        world.body_events_into(&mut events).unwrap();
        assert_eq!(events.len(), initial);
        assert_eq!(world.body_events().unwrap().len(), initial);

        world.disable_body(bodies[1]).unwrap();
        world.disable_body(bodies[2]).unwrap();
        world.step(1.0 / 60.0, 4).unwrap();
        let smaller = body_event_count(&world);
        let (result, allocations) = allocations_during(|| world.body_events_into(&mut events));
        assert_no_allocations(result, allocations, "body_events_into smaller window");
        assert_eq!(events.len(), smaller);

        world.enable_body(bodies[1]).unwrap();
        world
            .set_body_linear_velocity(bodies[1], [1.0, 0.0, 0.0])
            .unwrap();
        world.step(1.0 / 60.0, 4).unwrap();
        let larger = body_event_count(&world);
        let (result, allocations) = allocations_during(|| world.body_events_into(&mut events));
        assert_no_allocations(result, allocations, "body_events_into larger window");
        assert_eq!(events.len(), larger);
        assert_variable_counts(initial, smaller, larger, "body events");
        assert_eq!(world.body_events().unwrap().len(), larger);
    }

    {
        let foundation = foundation();
        let mut world = foundation
            .create_world(
                foundation
                    .world_def_builder()
                    .gravity(Vec3::ZERO)
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let sensor_body = world.create_body(foundation.body_def()).unwrap();
        world
            .create_hull_shape(
                sensor_body,
                &foundation
                    .shape_def_builder()
                    .sensor(true)
                    .enable_sensor_events(true)
                    .build()
                    .unwrap(),
                &BoxHull::new(20.0, 2.0, 2.0).unwrap(),
            )
            .unwrap();
        for x in [-6.0, -3.0, 0.0] {
            create_sensor_visitor(&mut world, x);
        }
        world.step(1.0 / 60.0, 4).unwrap();
        let initial = sensor_event_count(&world);
        let mut events = SensorEvents::default();
        world.sensor_events_into(&mut events).unwrap();
        world.sensor_events_into(&mut events).unwrap();
        assert_eq!(events.begin.len() + events.end.len(), initial);
        let owned = world.sensor_events().unwrap();
        assert_eq!(owned.begin.len() + owned.end.len(), initial);

        create_sensor_visitor(&mut world, 3.0);
        world.step(1.0 / 60.0, 4).unwrap();
        let smaller = sensor_event_count(&world);
        let (result, allocations) = allocations_during(|| world.sensor_events_into(&mut events));
        assert_no_allocations(result, allocations, "sensor_events_into smaller window");
        assert_eq!(events.begin.len() + events.end.len(), smaller);

        create_sensor_visitor(&mut world, 6.0);
        create_sensor_visitor(&mut world, 9.0);
        world.step(1.0 / 60.0, 4).unwrap();
        let larger = sensor_event_count(&world);
        let (result, allocations) = allocations_during(|| world.sensor_events_into(&mut events));
        assert_no_allocations(result, allocations, "sensor_events_into larger window");
        assert_eq!(events.begin.len() + events.end.len(), larger);
        assert_variable_counts(initial, smaller, larger, "sensor events");
        let owned = world.sensor_events().unwrap();
        assert_eq!(owned.begin.len() + owned.end.len(), larger);
    }

    {
        let foundation = foundation();
        let mut world = foundation.create_world(foundation.world_def()).unwrap();
        let ground = world.create_body(foundation.body_def()).unwrap();
        world
            .create_hull_shape(
                ground,
                &foundation.shape_def(),
                &BoxHull::new(20.0, 0.5, 2.0).unwrap(),
            )
            .unwrap();
        for x in [-6.0, -3.0, 0.0] {
            create_contact_body(&mut world, x);
        }
        world.step(1.0 / 60.0, 4).unwrap();
        let initial = contact_event_count(&world);
        let mut events = ContactEvents::default();
        world.contact_events_into(&mut events).unwrap();
        world.contact_events_into(&mut events).unwrap();
        assert_eq!(
            events.begin.len() + events.end.len() + events.hit.len(),
            initial
        );
        let owned = world.contact_events().unwrap();
        assert_eq!(
            owned.begin.len() + owned.end.len() + owned.hit.len(),
            initial
        );

        create_contact_body(&mut world, 3.0);
        world.step(1.0 / 60.0, 4).unwrap();
        let smaller = contact_event_count(&world);
        resolve_contact_event_ids(&world);
        let (result, allocations) = allocations_during(|| world.contact_events_into(&mut events));
        assert_no_allocations(result, allocations, "contact_events_into smaller window");
        assert_eq!(
            events.begin.len() + events.end.len() + events.hit.len(),
            smaller
        );

        create_contact_body(&mut world, 6.0);
        create_contact_body(&mut world, 9.0);
        world.step(1.0 / 60.0, 4).unwrap();
        let larger = contact_event_count(&world);
        resolve_contact_event_ids(&world);
        let (result, allocations) = allocations_during(|| world.contact_events_into(&mut events));
        assert_no_allocations(result, allocations, "contact_events_into larger window");
        assert_eq!(
            events.begin.len() + events.end.len() + events.hit.len(),
            larger
        );
        assert_variable_counts(initial, smaller, larger, "contact events");
        let owned = world.contact_events().unwrap();
        assert_eq!(
            owned.begin.len() + owned.end.len() + owned.hit.len(),
            larger
        );
    }

    {
        let foundation = foundation();
        let mut world = foundation
            .create_world(
                foundation
                    .world_def_builder()
                    .gravity(Vec3::ZERO)
                    .build()
                    .unwrap(),
            )
            .unwrap();
        let bodies = [
            create_joint_pair(&mut world, -6.0),
            create_joint_pair(&mut world, 0.0),
            create_joint_pair(&mut world, 6.0),
        ];
        for body in bodies {
            world
                .apply_force_to_center(body, [250.0, 0.0, 0.0], true)
                .unwrap();
        }
        world.step(1.0 / 60.0, 4).unwrap();
        let initial = joint_event_count(&world);
        let mut events = Vec::new();
        world.joint_events_into(&mut events).unwrap();
        world.joint_events_into(&mut events).unwrap();
        assert_eq!(events.len(), initial);
        assert_eq!(world.joint_events().unwrap().len(), initial);

        world.disable_body(bodies[1]).unwrap();
        world.disable_body(bodies[2]).unwrap();
        world
            .apply_force_to_center(bodies[0], [250.0, 0.0, 0.0], true)
            .unwrap();
        world.step(1.0 / 60.0, 4).unwrap();
        let smaller = joint_event_count(&world);
        let (result, allocations) = allocations_during(|| world.joint_events_into(&mut events));
        assert_no_allocations(result, allocations, "joint_events_into smaller window");
        assert_eq!(events.len(), smaller);

        world.enable_body(bodies[1]).unwrap();
        for body in [bodies[0], bodies[1]] {
            world
                .apply_force_to_center(body, [250.0, 0.0, 0.0], true)
                .unwrap();
        }
        world.step(1.0 / 60.0, 4).unwrap();
        let larger = joint_event_count(&world);
        let (result, allocations) = allocations_during(|| world.joint_events_into(&mut events));
        assert_no_allocations(result, allocations, "joint_events_into larger window");
        assert_eq!(events.len(), larger);
        assert_variable_counts(initial, smaller, larger, "joint events");
        assert_eq!(world.joint_events().unwrap().len(), larger);
    }
}
