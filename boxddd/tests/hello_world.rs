use boxddd::{BodyType, BoxHull, Vec3};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn box3d_hello_world_falls_onto_ground() {
    let foundation = foundation();
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity(Vec3::new(0.0, -10.0, 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();

    let ground = world
        .create_body(
            foundation
                .body_def_builder()
                .position(Vec3::new(0.0, -10.0, 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let ground_box = BoxHull::new(50.0, 10.0, 50.0).unwrap();
    world
        .create_hull_shape(ground, &foundation.shape_def(), &ground_box)
        .unwrap();

    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position(Vec3::new(0.0, 4.0, 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let dynamic_cube = BoxHull::cube(1.0).unwrap();
    let shape_def = foundation
        .shape_def_builder()
        .density(1.0)
        .friction(0.3)
        .build()
        .unwrap();
    world
        .create_hull_shape(body, &shape_def, &dynamic_cube)
        .unwrap();

    for _ in 0..90 {
        world.step(1.0 / 60.0, 4).unwrap();
    }

    let position = world.body_position(body).unwrap();
    let rotation = world.body_rotation(body).unwrap();

    assert!((position.y - 1.0).abs() < 0.05, "{position:?}");
    assert!(rotation.v.x.abs() < 0.05, "{rotation:?}");
    assert!(rotation.v.z.abs() < 0.05, "{rotation:?}");
}

#[test]
fn sphere_shape_can_be_created() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .build()
                .unwrap(),
        )
        .unwrap();
    let shape = world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &boxddd::Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    assert_eq!(world.contains_shape(shape), Ok(true));
}
