use boxddd::error::{HandleKind, InvalidValueReason};
use boxddd::{
    BodyDef, BodyId, BodyType, ContactId, DynamicTree, DynamicTreeProxyId, Error, JointId,
    RecPlayer, Recording, ReplayWorldId, ShapeDef, ShapeId, Sphere, Vec3, World, WorldDef,
};
use static_assertions::{assert_impl_all, assert_not_impl_any};

assert_not_impl_any!(World: Send, Sync);
assert_not_impl_any!(DynamicTree: Send, Sync);
assert_not_impl_any!(Recording: Send, Sync);
assert_not_impl_any!(RecPlayer: Send, Sync);
assert_impl_all!(BodyId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(ShapeId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(JointId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(ContactId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(DynamicTreeProxyId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(ReplayWorldId: Copy, Eq, std::hash::Hash, Send, Sync);

#[test]
fn invalid_world_definition_returns_error() {
    assert_eq!(
        WorldDef::builder()
            .gravity([f32::NAN, 0.0, 0.0])
            .build()
            .unwrap_err(),
        Error::InvalidValue {
            context: "world.gravity",
            reason: InvalidValueReason::NonFinite,
        }
    );
}

#[test]
fn callback_guard_blocks_result_first_apis() {
    let mut world = World::new(WorldDef::default()).unwrap();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(
        world.set_gravity(Vec3::ZERO).unwrap_err(),
        Error::InCallback
    );
    assert_eq!(world.counters().unwrap_err(), Error::InCallback);
}

#[test]
fn stale_body_id_returns_result_first_error() {
    let mut world = World::new(WorldDef::default()).unwrap();
    let body = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    assert!(world.contains_body(body));

    world.destroy_body(body).unwrap();
    assert!(!world.contains_body(body));

    assert_eq!(
        world.body_position(body).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Body,
        }
    );
}

#[test]
fn invalid_create_inputs_return_typed_errors() {
    let mut world = World::new(WorldDef::default()).unwrap();

    assert_eq!(
        BodyDef::builder()
            .position([f32::INFINITY, 0.0, 0.0])
            .build()
            .unwrap_err(),
        Error::InvalidValue {
            context: "body.position",
            reason: InvalidValueReason::NonFinite,
        }
    );

    let body = world.create_body(BodyDef::default()).unwrap();
    let shape_def = ShapeDef::default();
    let bad_sphere = Sphere::new([0.0, 0.0, 0.0], f32::NAN);
    assert_eq!(
        world.create_sphere_shape(body, &shape_def, &bad_sphere),
        Err(Error::InvalidValue {
            context: "sphere.radius",
            reason: InvalidValueReason::NonFinite,
        })
    );
}
