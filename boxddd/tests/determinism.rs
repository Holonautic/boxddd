use boxddd::{BodyType, Recording, Sphere};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn replay_validation_is_explicit_about_worker_count() {
    let foundation = foundation();
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity([0.0, -9.8, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let mut recording = Recording::new().unwrap();
    let mut session = world.record(&mut recording).unwrap();
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
    for _ in 0..6 {
        session.world().step(1.0 / 60.0, 4).unwrap();
    }
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
    assert_eq!(player.info().unwrap().worker_count, 1);
    player.set_worker_count(1).unwrap();
    player.restart().unwrap();
    while player.step_frame().unwrap() {}
    assert!(!player.has_diverged().unwrap());
}
