use boxddd::error::HandleKind;
use boxddd::{BodyDef, BodyType, Error, World, WorldDef};

#[test]
fn stale_body_id_returns_typed_error_at_safe_boundary() {
    let mut world = World::new(WorldDef::default()).unwrap();
    let body = world.create_body(
        BodyDef::builder()
            .body_type(BodyType::Dynamic)
            .position([0.0, 2.0, 0.0])
            .build()
            .unwrap(),
    );
    assert!(world.contains_body(body));

    world.destroy_body(body);
    assert!(!world.contains_body(body));
    assert_eq!(
        world.try_body_position(body).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Body,
        }
    );
}
