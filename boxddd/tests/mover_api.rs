use boxddd::error::InvalidValueReason;
use boxddd::{
    BodyType, BoxHull, Capsule, Error, ExplosionDef, Foundation, QueryFilter, Sphere, Vec3,
};

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

#[test]
fn cast_mover_returns_fraction_for_simple_static_scene() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_hull_shape(
            body,
            &foundation.shape_def(),
            &BoxHull::new(1.0, 0.5, 1.0).unwrap(),
        )
        .unwrap();
    let mover = Capsule::new([0.0, 2.0, 0.0], [0.0, 3.0, 0.0], 0.25);
    let fraction = world
        .cast_mover(
            [0.0, 0.0, 0.0],
            &mover,
            [0.0, -4.0, 0.0],
            QueryFilter::default(),
        )
        .unwrap();
    assert!((0.0..=1.0).contains(&fraction));
}

#[test]
fn collide_mover_returns_world_and_body_planes() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    let shape = world
        .create_hull_shape(
            body,
            &foundation.shape_def(),
            &BoxHull::new(0.5, 0.5, 0.5).unwrap(),
        )
        .unwrap();
    let mover = Capsule::new([-0.3, 0.6, 0.0], [0.3, 0.6, 0.0], 0.2);

    let world_planes = world
        .collide_mover([0.0, 0.0, 0.0], &mover, QueryFilter::default())
        .unwrap();
    assert!(world_planes.iter().any(|plane| plane.shape_id == shape));

    let body_planes = world
        .body_collide_mover(body, [0.0, 0.0, 0.0], &mover, QueryFilter::default())
        .unwrap();
    assert!(body_planes.iter().any(|plane| plane.shape_id == shape));
}

#[test]
fn collide_mover_callback_panic_is_reported() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_hull_shape(
            body,
            &foundation.shape_def(),
            &BoxHull::new(0.5, 0.5, 0.5).unwrap(),
        )
        .unwrap();
    let mover = Capsule::new([-0.3, 0.6, 0.0], [0.3, 0.6, 0.0], 0.2);

    assert_eq!(
        world
            .visit_collide_mover([0.0, 0.0, 0.0], &mover, QueryFilter::default(), |_| {
                panic!("callback panic should be contained");
            })
            .unwrap_err(),
        Error::CallbackPanicked
    );
}

#[test]
fn explode_changes_near_dynamic_body_velocity_and_rejects_invalid_input() {
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
    let near_body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([1.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            near_body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
    let far_body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([10.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            far_body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();

    let explosion = ExplosionDef::builder()
        .position(Vec3::ZERO)
        .radius(2.0)
        .falloff(0.0)
        .impulse_per_area(20.0)
        .build()
        .unwrap();
    world.explode(&explosion).unwrap();

    assert!(world.body_linear_velocity(near_body).unwrap().x > 0.0);
    assert_eq!(world.body_linear_velocity(far_body).unwrap(), Vec3::ZERO);

    assert_eq!(
        ExplosionDef::builder().radius(-1.0).build().unwrap_err(),
        Error::InvalidValue {
            context: "explosion.radius",
            reason: InvalidValueReason::OutOfRange,
        }
    );
}
