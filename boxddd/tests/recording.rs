use boxddd::{
    Aabb, BodyDef, BodyType, BoxHull, DebugDrawOptions, Error, QueryFilter, RecPlayer, Recording,
    ShapeDef, Sphere, Vec3, World, WorldDef, raw, validate_replay_bytes,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn replay_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn record_basic_scene(frame_count: usize) -> Recording {
    let mut world = World::new(
        WorldDef::builder()
            .gravity([0.0, -10.0, 0.0])
            .build()
            .unwrap(),
    )
    .unwrap();
    let mut recording = Recording::new().unwrap();
    let mut session = world.record(&mut recording).unwrap();

    let ground = session.world().create_body(
        BodyDef::builder()
            .position([0.0, -1.0, 0.0])
            .build()
            .unwrap(),
    );
    session
        .world()
        .create_hull_shape(ground, &ShapeDef::default(), &BoxHull::new(10.0, 0.5, 10.0));
    let body = session.world().create_body(
        BodyDef::builder()
            .body_type(BodyType::Dynamic)
            .position([0.0, 2.0, 0.0])
            .build()
            .unwrap(),
    );
    session.world().create_sphere_shape(
        body,
        &ShapeDef::builder().density(1.0).build().unwrap(),
        &Sphere::new([0.0, 0.0, 0.0], 0.25),
    );

    for _ in 0..frame_count {
        session.world().step(1.0 / 60.0, 4);
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
    assert!(!recording.is_empty());
    assert!(recording.bytes().unwrap().len() > 128);
    assert!(recording.validate_replay(1).unwrap());

    let mut player = recording.create_player(1).unwrap();
    assert_eq!(player.frame(), 0);
    assert!(player.frame_count() >= 8);
    let info = player.info();
    assert_eq!(info.frame_count, player.frame_count());
    assert_eq!(info.sub_step_count, 4);
    assert_eq!(info.worker_count, 1);

    assert!(player.step_frame().unwrap());
    assert_eq!(player.frame(), 1);
    player.seek_frame(player.frame_count()).unwrap();
    assert_eq!(player.frame(), player.frame_count());
    assert!(!player.step_frame().unwrap());
    assert!(player.is_at_end());
    assert!(!player.has_diverged());
    assert_eq!(player.diverge_frame(), None);
    assert!(player.body_count() >= 2);
    assert!(player.body_id(0).unwrap().is_some());

    player.restart().unwrap();
    assert_eq!(player.frame(), 0);
    assert!(!player.is_at_end());
    assert!(!player.is_at_pre_step());
    player.sub_step_frame().unwrap();
    assert!(player.is_at_pre_step());
    player.sub_step_frame().unwrap();
    assert!(!player.is_at_pre_step());
    let world_id = player.world_id();
    player.seek_frame(1).unwrap();
    player.restart().unwrap();
    assert_eq!(player.world_id(), world_id);

    player.set_worker_count(1).unwrap();
    player.set_keyframe_policy(256 * 1024, 4).unwrap();
    assert_eq!(player.keyframe_budget(), 256 * 1024);
    assert_eq!(player.keyframe_min_interval(), 4);
    assert!(player.keyframe_interval() >= 4);
    assert!(player.keyframe_bytes() <= player.keyframe_budget());
}

#[test]
fn replay_player_exclusively_leases_length_units() {
    let _guard = replay_test_lock();
    let recording = record_basic_scene(1);
    let bytes = recording.to_vec().unwrap();
    let original_length_units = raw::length_units_per_meter();
    let player = recording.create_player(1).unwrap();

    assert_eq!(
        RecPlayer::from_bytes(&bytes, 1).unwrap_err(),
        Error::ResourceLifetimeViolation
    );
    assert_eq!(
        validate_replay_bytes(&bytes, 1).unwrap_err(),
        Error::ResourceLifetimeViolation
    );
    assert_eq!(
        raw::try_set_length_units_per_meter(original_length_units).unwrap_err(),
        Error::ResourceLifetimeViolation
    );

    drop(player);
    assert_eq!(raw::length_units_per_meter(), original_length_units);
}

#[test]
fn recording_can_be_saved_loaded_and_validated_from_bytes() {
    let recording = record_basic_scene(4);
    let bytes = recording.to_vec().unwrap();
    assert!(validate_replay_bytes(&bytes, 1).unwrap());

    let path = temp_recording_path("roundtrip");
    let _ = std::fs::remove_file(&path);
    recording.save_to_file(&path).unwrap();
    let loaded = Recording::load_from_file(&path).unwrap();
    assert_eq!(loaded.bytes().unwrap(), bytes.as_slice());
    assert!(loaded.validate_replay(1).unwrap());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn unfinished_session_stops_recording_before_world_drop() {
    let mut recording = Recording::new().unwrap();
    let mut world = World::new(WorldDef::default()).unwrap();
    {
        let mut session = world.record(&mut recording).unwrap();
        let body = session.world().create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .build()
                .unwrap(),
        );
        session.world().create_sphere_shape(
            body,
            &ShapeDef::builder().density(1.0).build().unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.25),
        );
        session.world().step(1.0 / 60.0, 4);
    }

    assert!(!recording.bytes().unwrap().is_empty());
    assert!(recording.validate_replay(1).unwrap());
    drop(world);
}

#[test]
fn malformed_recordings_fail_without_panicking() {
    let malformed = [0_u8; 64];
    assert_eq!(
        RecPlayer::from_bytes(&malformed, 1).unwrap_err(),
        Error::CreateRecPlayerFailed
    );
    assert!(!validate_replay_bytes(&malformed, 1).unwrap());
    assert_eq!(
        validate_replay_bytes(&[], 1).unwrap_err(),
        Error::InvalidArgument
    );
    assert_eq!(
        RecPlayer::from_bytes(&malformed, 0).unwrap_err(),
        Error::InvalidArgument
    );
}

#[test]
fn replay_exposes_recorded_queries_and_draws_them() {
    let _guard = replay_test_lock();
    let mut world = World::new(WorldDef::default()).unwrap();
    let body = world.create_body(
        BodyDef::builder()
            .position([0.0, 0.0, 0.0])
            .build()
            .unwrap(),
    );
    world.create_sphere_shape(
        body,
        &ShapeDef::default(),
        &Sphere::new([0.0, 0.0, 0.0], 0.5),
    );
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
        session.world().step(1.0 / 60.0, 4);
    }
    session.finish().unwrap();

    let mut player = recording.create_player(1).unwrap();
    player.seek_frame(1).unwrap();
    assert_eq!(player.frame_query_count(), 1);
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
