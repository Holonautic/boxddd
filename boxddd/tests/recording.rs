use boxddd::error::InvalidValueReason;
use boxddd::{
    Aabb, BodyType, BoxHull, DebugDrawOptions, Error, Foundation, QueryFilter, Recording, Sphere,
    Vec3,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn replay_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

fn record_basic_scene(frame_count: usize) -> Recording {
    let foundation = foundation();
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity([0.0, -10.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let mut recording = Recording::new().unwrap();
    let mut session = world.record(&mut recording).unwrap();

    let ground = session
        .world()
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, -1.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    session
        .world()
        .create_hull_shape(
            ground,
            &foundation.shape_def(),
            &BoxHull::new(10.0, 0.5, 10.0).unwrap(),
        )
        .unwrap();
    let body = session
        .world()
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    session
        .world()
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.25),
        )
        .unwrap();

    for _ in 0..frame_count {
        session.world().step(1.0 / 60.0, 4).unwrap();
    }
    session.finish().unwrap();
    recording
}

fn temp_recording_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("boxddd-{name}-{}.b3rec", std::process::id()))
}

#[test]
fn recording_round_trips_and_player_controls_work() {
    let _guard = replay_test_lock();
    let recording = record_basic_scene(8);
    assert!(!recording.is_empty().unwrap());
    assert!(recording.bytes().unwrap().len() > 128);
    let foundation = foundation();
    assert!(
        foundation
            .validate_replay(recording.bytes().unwrap(), 1)
            .unwrap()
    );

    let mut player = foundation
        .create_replay_player(recording.bytes().unwrap(), 1)
        .unwrap();
    assert_eq!(player.frame().unwrap(), 0);
    assert!(player.frame_count().unwrap() >= 8);
    let info = player.info().unwrap();
    assert_eq!(info.frame_count, player.frame_count().unwrap());
    assert_eq!(info.sub_step_count, 4);
    assert_eq!(info.worker_count, 1);

    assert!(player.step_frame().unwrap());
    assert_eq!(player.frame().unwrap(), 1);
    player.seek_frame(player.frame_count().unwrap()).unwrap();
    assert_eq!(player.frame().unwrap(), player.frame_count().unwrap());
    assert!(!player.step_frame().unwrap());
    assert!(player.is_at_end().unwrap());
    assert!(!player.has_diverged().unwrap());
    assert_eq!(player.diverge_frame().unwrap(), None);
    assert!(player.body_count().unwrap() >= 2);
    assert!(player.body_id(0).unwrap().is_some());

    player.restart().unwrap();
    assert_eq!(player.frame().unwrap(), 0);
    assert!(!player.is_at_end().unwrap());
    assert!(!player.is_at_pre_step().unwrap());
    player.sub_step_frame().unwrap();
    assert!(player.is_at_pre_step().unwrap());
    player.sub_step_frame().unwrap();
    assert!(!player.is_at_pre_step().unwrap());
    let world_id = player.world_id();
    player.seek_frame(1).unwrap();
    player.restart().unwrap();
    assert_eq!(player.world_id(), world_id);

    player.set_worker_count(1).unwrap();
    player.set_keyframe_policy(256 * 1024, 4).unwrap();
    assert_eq!(player.keyframe_budget().unwrap(), 256 * 1024);
    assert_eq!(player.keyframe_min_interval().unwrap(), 4);
    assert!(player.keyframe_interval().unwrap() >= 4);
    assert!(player.keyframe_bytes().unwrap() <= player.keyframe_budget().unwrap());
}

#[test]
fn replay_ccd_allows_missing_pre_solve_callback() {
    let _guard = replay_test_lock();
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
    let mut recording = Recording::new().unwrap();
    let mut session = world.record(&mut recording).unwrap();

    let wall = session.world().create_body(foundation.body_def()).unwrap();
    session
        .world()
        .create_hull_shape(
            wall,
            &foundation.shape_def(),
            &BoxHull::new(0.05, 4.0, 4.0).unwrap(),
        )
        .unwrap();
    let bullet = session
        .world()
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([-5.0, 0.0, 0.0])
                .linear_velocity([600.0, 0.0, 0.0])
                .gravity_scale(0.0)
                .bullet(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    session
        .world()
        .create_sphere_shape(
            bullet,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .enable_pre_solve_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.2),
        )
        .unwrap();
    session.world().step(1.0 / 60.0, 4).unwrap();
    session.finish().unwrap();
    drop(world);

    assert!(
        foundation
            .validate_replay(recording.bytes().unwrap(), 1)
            .unwrap()
    );
    let mut player = foundation
        .create_replay_player(recording.bytes().unwrap(), 1)
        .unwrap();
    assert!(player.step_frame().unwrap());
}

#[test]
fn invalid_recording_length_scales_are_rejected_before_native_entry() {
    const LENGTH_SCALE_OFFSET: usize = 12;

    let _guard = replay_test_lock();
    let recording = record_basic_scene(1);
    let valid_bytes = recording.to_vec().unwrap();

    for (length_units, reason) in [
        (0.0, InvalidValueReason::OutOfRange),
        (-1.0, InvalidValueReason::OutOfRange),
        (f32::from_bits(1), InvalidValueReason::OutOfRange),
        (f32::MIN_POSITIVE, InvalidValueReason::OutOfRange),
        (f32::MAX, InvalidValueReason::OutOfRange),
        (f32::NAN, InvalidValueReason::NonFinite),
        (f32::INFINITY, InvalidValueReason::NonFinite),
    ] {
        let mut bytes = valid_bytes.clone();
        bytes[LENGTH_SCALE_OFFSET..LENGTH_SCALE_OFFSET + std::mem::size_of::<f32>()]
            .copy_from_slice(&length_units.to_le_bytes());
        let expected = Error::InvalidValue {
            context: "recording.length_scale",
            reason,
        };

        assert_eq!(
            foundation().create_replay_player(&bytes, 1).unwrap_err(),
            expected
        );
        assert_eq!(
            foundation().validate_replay(&bytes, 1).unwrap_err(),
            expected
        );
    }

    assert_eq!(
        foundation()
            .create_replay_player(&valid_bytes[..47], 1)
            .unwrap_err(),
        Error::InvalidValue {
            context: "recording.replay_bytes",
            reason: InvalidValueReason::Malformed,
        }
    );
}

#[test]
fn recording_can_be_saved_loaded_and_validated_from_bytes() {
    let foundation = foundation();
    let recording = record_basic_scene(4);
    let bytes = recording.to_vec().unwrap();
    assert!(foundation.validate_replay(&bytes, 1).unwrap());

    let path = temp_recording_path("roundtrip");
    let _ = std::fs::remove_file(&path);
    recording.save_to_file(&path).unwrap();
    let loaded = Recording::load_from_file(&path).unwrap();
    assert_eq!(loaded.bytes().unwrap(), bytes.as_slice());
    assert!(
        foundation
            .validate_replay(loaded.bytes().unwrap(), 1)
            .unwrap()
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn unfinished_session_stops_recording_before_world_drop() {
    let foundation = foundation();
    let mut recording = Recording::new().unwrap();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    {
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
                &Sphere::new([0.0, 0.0, 0.0], 0.25),
            )
            .unwrap();
        session.world().step(1.0 / 60.0, 4).unwrap();
    }

    assert!(!recording.bytes().unwrap().is_empty());
    assert_eq!(
        foundation
            .validate_replay(recording.bytes().unwrap(), 1)
            .unwrap_err(),
        Error::FoundationBusy
    );
    drop(world);
    assert!(
        foundation
            .validate_replay(recording.bytes().unwrap(), 1)
            .unwrap()
    );
}

#[test]
fn forgotten_session_cannot_leave_a_dangling_native_recording() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let mut recording = Recording::new().unwrap();
    let session = world.record(&mut recording).unwrap();
    std::mem::forget(session);

    assert_eq!(recording.bytes().unwrap_err(), Error::RecordingInUse);
    let mut replacement = Recording::new().unwrap();
    assert_eq!(
        world.record(&mut replacement).unwrap_err(),
        Error::RecordingInUse
    );
    drop(recording);

    world.step(1.0 / 60.0, 4).unwrap();
    world.record(&mut replacement).unwrap().finish().unwrap();
    assert!(!replacement.is_empty().unwrap());
}

#[test]
fn world_drop_detaches_a_forgotten_recording_session() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let mut recording = Recording::new().unwrap();
    let session = world.record(&mut recording).unwrap();
    std::mem::forget(session);

    drop(world);

    assert!(!recording.is_empty().unwrap());
    assert!(
        foundation
            .validate_replay(recording.bytes().unwrap(), 1)
            .unwrap()
    );
}

#[test]
fn recording_and_replay_accessors_respect_callback_guard() {
    let _serial = replay_test_lock();
    let recording = record_basic_scene(1);
    let player = foundation()
        .create_replay_player(recording.bytes().unwrap(), 1)
        .unwrap();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(recording.len().unwrap_err(), Error::InCallback);
    assert_eq!(recording.is_empty().unwrap_err(), Error::InCallback);
    assert_eq!(player.frame().unwrap_err(), Error::InCallback);
    assert_eq!(player.info().unwrap_err(), Error::InCallback);
    assert_eq!(player.body_id(0).unwrap_err(), Error::InCallback);
}

#[test]
fn malformed_recordings_fail_without_panicking() {
    const RECORDING_HEADER_BYTES: usize = 48;

    let _guard = replay_test_lock();
    foundation();
    let mut malformed = [0_u8; 64];
    malformed[12..16].copy_from_slice(&1.0_f32.to_le_bytes());
    assert_eq!(
        foundation()
            .create_replay_player(&malformed, 1)
            .unwrap_err(),
        Error::NativeFailure
    );
    assert!(!foundation().validate_replay(&malformed, 1).unwrap());
    assert_eq!(
        foundation().validate_replay(&[], 1).unwrap_err(),
        Error::InvalidValue {
            context: "recording.replay_bytes",
            reason: InvalidValueReason::Malformed,
        }
    );
    assert_eq!(
        foundation()
            .create_replay_player(&malformed, 0)
            .unwrap_err(),
        Error::InvalidValue {
            context: "rec_player.worker_count",
            reason: InvalidValueReason::OutOfRange,
        }
    );

    let recording = record_basic_scene(1);
    let mut corrupt_snapshot = recording.to_vec().unwrap();
    // Box3D statically asserts that b3RecHeader is 48 bytes; corrupt the first snapshot instead.
    corrupt_snapshot[RECORDING_HEADER_BYTES..64].fill(0);
    assert_eq!(
        foundation()
            .create_replay_player(&corrupt_snapshot, 1)
            .unwrap_err(),
        Error::NativeFailure
    );
}

#[test]
fn replay_exposes_recorded_queries_and_draws_them() {
    let _guard = replay_test_lock();
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &foundation.shape_def(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();
    let mut recording = Recording::new().unwrap();
    let mut session = world.record(&mut recording).unwrap();

    let aabb = Aabb {
        lower_bound: Vec3::new(-1.0, -1.0, -1.0),
        upper_bound: Vec3::new(1.0, 1.0, 1.0),
    };
    for _ in 0..3 {
        let hits = session
            .world()
            .overlap_aabb(aabb, QueryFilter::new().id(42))
            .unwrap();
        assert_eq!(hits.len(), 1);
        session.world().step(1.0 / 60.0, 4).unwrap();
    }
    session.finish().unwrap();
    drop(world);

    let mut player = foundation
        .create_replay_player(recording.bytes().unwrap(), 1)
        .unwrap();
    player.seek_frame(1).unwrap();
    assert_eq!(player.frame_query_count().unwrap(), 1);
    let query = player.frame_query(0).unwrap();
    assert_eq!(query.id, 42);
    assert_eq!(query.hit_count, 1);
    let hit = player.frame_query_hit(0, 0).unwrap();
    let _shape_id = hit.shape_id;

    let commands = player
        .draw_frame_queries_collect(DebugDrawOptions::default(), None, None)
        .unwrap();
    assert!(!commands.is_empty());
}
