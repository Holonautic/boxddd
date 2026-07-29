use boxddd::{BodyDef, BodyType, BoxHull, ShapeDef, Vec3, World, WorldDef};

#[test]
fn box3d_hello_world_falls_onto_ground() {
    let mut world = World::new(
        WorldDef::builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()
            .unwrap(),
    )
    .unwrap();

    let ground = world
        .create_body(
            BodyDef::builder()
                .position(Vec3::new(0.0, -10.0, 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let ground_box = BoxHull::new(50.0, 10.0, 50.0).unwrap();
    world
        .create_hull_shape(ground, &ShapeDef::default(), &ground_box)
        .unwrap();

    let body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position(Vec3::new(0.0, 4.0, 0.0))
                .build()
                .unwrap(),
        )
        .unwrap();
    let dynamic_cube = BoxHull::cube(1.0).unwrap();
    let shape_def = ShapeDef::builder()
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
    let mut world = World::new(WorldDef::default()).unwrap();
    let body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .build()
                .unwrap(),
        )
        .unwrap();
    let shape = world
        .create_sphere_shape(
            body,
            &ShapeDef::builder().density(1.0).build().unwrap(),
            &boxddd::Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    assert!(world.contains_shape(shape));
}
