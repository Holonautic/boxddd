use boxddd::{BodyType, BoxHull, Error, Foundation, Recording, Sphere, Vec3, World};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

fn overlapping_contact_world(
    overlap: Arc<NativeStepOverlap>,
    callback_count: Arc<AtomicUsize>,
) -> World {
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
    let ground = world.create_body(foundation.body_def()).unwrap();
    world
        .create_hull_shape(
            ground,
            &foundation
                .shape_def_builder()
                .enable_custom_filtering(true)
                .build()
                .unwrap(),
            &BoxHull::new(2.0, 0.5, 2.0).unwrap(),
        )
        .unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 0.25, 0.0])
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
                .enable_custom_filtering(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
    world
        .set_custom_filter(move |_, _| {
            callback_count.fetch_add(1, Ordering::SeqCst);
            overlap.wait_for_peer();
            false
        })
        .unwrap();
    world
}

fn replay_fixture() -> Vec<u8> {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let mut recording = Recording::new().unwrap();
    let mut session = world.record(&mut recording).unwrap();
    let body = session
        .world()
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .build()
                .unwrap(),
        )
        .unwrap();
    session
        .world()
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
    session.world().step(1.0 / 60.0, 2).unwrap();
    session.finish().unwrap();
    drop(world);
    recording.to_vec().unwrap()
}

struct NativeStepOverlap {
    entered: AtomicUsize,
    overlapped: AtomicBool,
    timed_out: AtomicBool,
}

impl NativeStepOverlap {
    fn new() -> Self {
        Self {
            entered: AtomicUsize::new(0),
            overlapped: AtomicBool::new(false),
            timed_out: AtomicBool::new(false),
        }
    }

    fn wait_for_peer(&self) {
        let arrival = self.entered.fetch_add(1, Ordering::SeqCst) + 1;
        if arrival > 2 {
            return;
        }

        let deadline = Instant::now() + Duration::from_secs(3);
        while self.entered.load(Ordering::SeqCst) < 2 {
            if Instant::now() >= deadline {
                self.timed_out.store(true, Ordering::SeqCst);
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        self.overlapped.store(true, Ordering::SeqCst);
    }
}

#[test]
fn independent_world_steps_overlap_inside_native_callbacks() {
    foundation();
    let overlap = Arc::new(NativeStepOverlap::new());
    let start = Arc::new(Barrier::new(2));

    thread::scope(|scope| {
        for _ in 0..2 {
            let overlap = Arc::clone(&overlap);
            let start = Arc::clone(&start);
            scope.spawn(move || {
                let callback_count = Arc::new(AtomicUsize::new(0));
                let mut world =
                    overlapping_contact_world(Arc::clone(&overlap), Arc::clone(&callback_count));
                start.wait();
                for _ in 0..4 {
                    world.step(1.0 / 60.0, 4).unwrap();
                    if callback_count.load(Ordering::SeqCst) > 0 {
                        break;
                    }
                }
                assert!(callback_count.load(Ordering::SeqCst) > 0);
            });
        }
    });

    assert!(
        overlap.overlapped.load(Ordering::SeqCst),
        "separate Worlds never overlapped inside b3World_Step"
    );
    assert!(
        !overlap.timed_out.load(Ordering::SeqCst),
        "one World step waited behind process-wide serialization"
    );
}

#[test]
fn concurrent_world_slot_mutation_releases_every_ordinary_owner() {
    let foundation = foundation();
    let baseline = foundation.activity();
    let start = Arc::new(Barrier::new(4));

    thread::scope(|scope| {
        for _ in 0..4 {
            let start = Arc::clone(&start);
            scope.spawn(move || {
                start.wait();
                for _ in 0..32 {
                    let mut world = foundation.create_world(foundation.world_def()).unwrap();
                    world.create_body(foundation.body_def()).unwrap();
                    world.step(1.0 / 120.0, 1).unwrap();
                }
            });
        }
    });

    assert_eq!(foundation.activity(), baseline);
    let world = foundation.create_world(foundation.world_def()).unwrap();
    drop(world);
    assert_eq!(foundation.activity(), baseline);
}

#[test]
fn world_slot_and_replay_rebuild_race_without_lock_order_deadlock() {
    const ITERATIONS: usize = 32;

    let foundation = foundation();
    let bytes = Arc::new(replay_fixture());
    let baseline = foundation.activity();
    let turn = Arc::new(Barrier::new(2));
    let world_attempts = Arc::new(AtomicUsize::new(0));
    let replay_validation_attempts = Arc::new(AtomicUsize::new(0));
    let replay_creation_attempts = Arc::new(AtomicUsize::new(0));

    thread::scope(|scope| {
        {
            let turn = Arc::clone(&turn);
            let world_attempts = Arc::clone(&world_attempts);
            scope.spawn(move || {
                for iteration in 0..ITERATIONS {
                    turn.wait();
                    if iteration % 4 < 2 {
                        thread::yield_now();
                    }
                    world_attempts.fetch_add(1, Ordering::SeqCst);
                    match foundation.create_world(foundation.world_def()) {
                        Ok(mut world) => {
                            world.create_body(foundation.body_def()).unwrap();
                            world.step(1.0 / 120.0, 1).unwrap();
                        }
                        Err(Error::FoundationBusy) => {}
                        Err(error) => panic!("unexpected World admission failure: {error}"),
                    }
                    turn.wait();
                }
            });
        }
        {
            let bytes = Arc::clone(&bytes);
            let turn = Arc::clone(&turn);
            let replay_validation_attempts = Arc::clone(&replay_validation_attempts);
            let replay_creation_attempts = Arc::clone(&replay_creation_attempts);
            scope.spawn(move || {
                for iteration in 0..ITERATIONS {
                    turn.wait();
                    if iteration % 4 >= 2 {
                        thread::yield_now();
                    }
                    if iteration % 2 == 0 {
                        replay_validation_attempts.fetch_add(1, Ordering::SeqCst);
                        match foundation.validate_replay(&bytes, 1) {
                            Ok(true) => {}
                            Ok(false) => panic!("replay fixture became invalid"),
                            Err(Error::FoundationBusy) => {}
                            Err(error) => panic!("unexpected replay validation failure: {error}"),
                        }
                    } else {
                        replay_creation_attempts.fetch_add(1, Ordering::SeqCst);
                        match foundation.create_replay_player(&bytes, 1) {
                            Ok(player) => {
                                player.close().unwrap();
                            }
                            Err(Error::FoundationBusy) => {}
                            Err(error) => panic!("unexpected replay creation failure: {error}"),
                        }
                    }
                    turn.wait();
                }
            });
        }
    });

    assert_eq!(world_attempts.load(Ordering::SeqCst), ITERATIONS);
    assert_eq!(
        replay_validation_attempts.load(Ordering::SeqCst),
        ITERATIONS / 2
    );
    assert_eq!(
        replay_creation_attempts.load(Ordering::SeqCst),
        ITERATIONS / 2
    );
    assert_eq!(foundation.activity(), baseline);

    let world = foundation.create_world(foundation.world_def()).unwrap();
    drop(world);
    assert!(foundation.validate_replay(&bytes, 1).unwrap());
    foundation
        .create_replay_player(&bytes, 1)
        .unwrap()
        .close()
        .unwrap();
    assert_eq!(foundation.activity(), baseline);
}
