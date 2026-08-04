use boxddd::{BodyType, BoxHull, Error, Foundation, TaskSystem, Vec3, World};
use static_assertions::assert_not_impl_any;

assert_not_impl_any!(World: Send, Sync);

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

fn populate_parallel_scene(world: &mut World) {
    let foundation = foundation();
    let ground = world
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, -1.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_hull_shape(
            ground,
            &foundation.shape_def(),
            &BoxHull::new(30.0, 0.5, 30.0).unwrap(),
        )
        .unwrap();

    let shape_def = foundation
        .shape_def_builder()
        .density(1.0)
        .friction(0.4)
        .build()
        .unwrap();
    for x in 0..12 {
        for z in 0..12 {
            let body = world
                .create_body(
                    foundation
                        .body_def_builder()
                        .body_type(BodyType::Dynamic)
                        .position([(x as f32 - 6.0) * 0.8, 3.0, (z as f32 - 6.0) * 0.8])
                        .build()
                        .unwrap(),
                )
                .unwrap();
            world
                .create_hull_shape(body, &shape_def, &BoxHull::cube(0.35).unwrap())
                .unwrap();
        }
    }
}

fn step_parallel_world(task_system: TaskSystem) -> (TaskSystem, World) {
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .gravity(Vec3::new(0.0, -10.0, 0.0))
        .worker_count(2)
        .task_system(task_system.clone())
        .build()
        .unwrap();
    let mut world = foundation.create_world(def).unwrap();
    populate_parallel_scene(&mut world);
    world.step(1.0 / 60.0, 4).unwrap();
    (task_system, world)
}

#[test]
fn blocking_task_system_steps_world_through_box3d_callbacks() {
    let (task_system, world) = step_parallel_world(TaskSystem::blocking_threads().unwrap());

    let stats = task_system.stats();
    assert!(stats.enqueued > 0, "{stats:?}");
    assert_eq!(stats.enqueued, stats.started, "{stats:?}");
    assert_eq!(stats.enqueued, stats.completed, "{stats:?}");
    assert_eq!(stats.enqueued, stats.finished, "{stats:?}");
    assert_eq!(
        boxddd::__private::task_system_guard_rejections_for_test(&task_system),
        0
    );
    assert!(!stats.panicked, "{stats:?}");
    assert!(world.counters().unwrap().task_count > 0);
}

#[test]
fn world_def_clone_preserves_task_system_context() {
    let task_system = TaskSystem::blocking_threads().unwrap();
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .worker_count(2)
        .task_system(task_system.clone())
        .build()
        .unwrap();
    let mut world = foundation.create_world(def.clone()).unwrap();
    populate_parallel_scene(&mut world);

    world.step(1.0 / 60.0, 4).unwrap();

    assert!(task_system.stats().enqueued > 0);
}

#[test]
fn zero_worker_count_uses_native_serial_fallback() {
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .worker_count(0)
        .build()
        .unwrap();
    let mut world = foundation.create_world(def).unwrap();

    world.step(1.0 / 60.0, 4).unwrap();
    assert!(world.worker_count().unwrap() >= 1);
}

#[test]
fn task_system_builder_normalizes_zero_workers_regardless_of_call_order() {
    let task_system = TaskSystem::blocking_threads().unwrap();
    let foundation = foundation();
    let task_then_workers = foundation
        .world_def_builder()
        .task_system(task_system.clone())
        .worker_count(0)
        .build()
        .unwrap();
    let workers_then_task = foundation
        .world_def_builder()
        .worker_count(0)
        .task_system(task_system)
        .build()
        .unwrap();

    assert_eq!(task_then_workers.worker_count, 1);
    assert_eq!(workers_then_task.worker_count, 1);
}

#[test]
fn enqueue_panic_is_reported_as_callback_panic() {
    let task_system = boxddd::__private::task_system_panic_on_enqueue_for_test();
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .worker_count(2)
        .task_system(task_system.clone())
        .build()
        .unwrap();
    let mut world = foundation.create_world(def).unwrap();
    populate_parallel_scene(&mut world);

    assert_eq!(world.step(1.0 / 60.0, 4), Err(Error::CallbackPanicked));
    assert!(task_system.stats().panicked);
}

#[test]
fn task_panic_is_reported_as_callback_panic() {
    let task_system = boxddd::__private::task_system_panic_on_task_for_test();
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .worker_count(2)
        .task_system(task_system.clone())
        .build()
        .unwrap();
    let mut world = foundation.create_world(def).unwrap();
    populate_parallel_scene(&mut world);

    let outcome = world.step_outcome(1.0 / 60.0, 4).unwrap();
    assert_eq!(outcome.post_step_error(), Some(&Error::CallbackPanicked));
    assert_eq!(outcome.into_result(), Err(Error::CallbackPanicked));
    let stats = task_system.stats();
    assert!(stats.panicked);
    assert_eq!(stats.enqueued, stats.started, "{stats:?}");
    assert_eq!(stats.enqueued, stats.completed, "{stats:?}");
    assert_eq!(stats.enqueued, stats.finished, "{stats:?}");
}

#[test]
fn finish_panic_is_reported_as_callback_panic() {
    let task_system = boxddd::__private::task_system_panic_on_finish_for_test();
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .worker_count(2)
        .task_system(task_system.clone())
        .build()
        .unwrap();
    let mut world = foundation.create_world(def).unwrap();
    populate_parallel_scene(&mut world);

    assert_eq!(world.step(1.0 / 60.0, 4), Err(Error::CallbackPanicked));
    assert!(task_system.stats().panicked);
}

#[test]
fn task_callback_runs_under_callback_guard() {
    let task_system = boxddd::__private::task_system_check_callback_guard_for_test();
    let foundation = foundation();
    let def = foundation
        .world_def_builder()
        .worker_count(2)
        .task_system(task_system.clone())
        .build()
        .unwrap();
    let mut world = foundation.create_world(def).unwrap();
    populate_parallel_scene(&mut world);

    world.step(1.0 / 60.0, 4).unwrap();

    let stats = task_system.stats();
    assert!(boxddd::__private::task_system_guard_rejections_for_test(&task_system) > 0);
    assert!(!stats.panicked, "{stats:?}");
}
