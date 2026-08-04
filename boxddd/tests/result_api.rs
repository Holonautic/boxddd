use boxddd::error::{HandleKind, InvalidValueReason};
use boxddd::{
    Aabb, BodyDef, BodyDefBuilder, BodyId, BodyType, ContactId, DynamicTree, DynamicTreeProxyId,
    Error, JointId, RecPlayer, Recording, ReplayWorldId, ShapeDef, ShapeDefBuilder, ShapeId,
    Sphere, Vec3, Version, World, WorldDef, WorldDefBuilder,
};
use static_assertions::{assert_impl_all, assert_not_impl_any};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

assert_not_impl_any!(World: Send, Sync);
assert_not_impl_any!(DynamicTree: Send, Sync);
assert_not_impl_any!(Recording: Send, Sync);
assert_not_impl_any!(RecPlayer: Send, Sync);
assert_not_impl_any!(BodyDef: Default);
assert_not_impl_any!(BodyDefBuilder: Default);
assert_not_impl_any!(ShapeDef: Default);
assert_not_impl_any!(ShapeDefBuilder: Default);
assert_not_impl_any!(WorldDef: Default);
assert_not_impl_any!(WorldDefBuilder: Default);
assert_impl_all!(BodyId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(ShapeId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(JointId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(ContactId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(DynamicTreeProxyId: Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(ReplayWorldId: Copy, Eq, std::hash::Hash, Send, Sync);

#[test]
fn manual_break_inventory_matches_public_result_signatures() {
    let _: fn() -> boxddd::Result<Version> = boxddd::version;
    let _: fn() -> boxddd::Result<Version> = boxddd::prelude::version;
    let _: fn() -> boxddd::Result<Version> = boxddd::world::version;
    let _: fn(Aabb) -> boxddd::Result<bool> = Aabb::is_bounded;
    let _: fn(Aabb) -> boxddd::Result<bool> = Aabb::is_sane;
    let _: fn(&World, ContactId) -> boxddd::Result<bool> = World::contains_contact;
}

#[test]
fn invalid_world_definition_returns_error() {
    let foundation = foundation();
    assert_eq!(
        foundation
            .world_def_builder()
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
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(
        world.set_gravity(Vec3::ZERO).unwrap_err(),
        Error::InCallback
    );
    assert_eq!(world.counters().unwrap_err(), Error::InCallback);
}

#[test]
fn stale_body_id_returns_result_first_error() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(world.contains_body(body), Ok(true));

    world.destroy_body(body).unwrap();
    assert_eq!(world.contains_body(body), Ok(false));

    assert_eq!(
        world.body_position(body).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Body,
        }
    );
}

#[test]
fn invalid_create_inputs_return_typed_errors() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();

    assert_eq!(
        foundation
            .body_def_builder()
            .position([f32::INFINITY, 0.0, 0.0])
            .build()
            .unwrap_err(),
        Error::InvalidValue {
            context: "body.position",
            reason: InvalidValueReason::NonFinite,
        }
    );

    let body = world.create_body(foundation.body_def()).unwrap();
    let shape_def = foundation.shape_def();
    let bad_sphere = Sphere::new([0.0, 0.0, 0.0], f32::NAN);
    assert_eq!(
        world.create_sphere_shape(body, &shape_def, &bad_sphere),
        Err(Error::InvalidValue {
            context: "sphere.radius",
            reason: InvalidValueReason::NonFinite,
        })
    );
}
