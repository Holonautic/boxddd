use boxddd::error::HandleKind;
use boxddd::{BodyDef, BodyType, BoxHull, Error, ShapeDef, World, WorldDef};

#[test]
fn dropping_world_makes_body_and_shape_handles_foreign_to_new_worlds() {
    let body;
    let shape;
    {
        let mut world = World::new(WorldDef::default()).unwrap();
        body = world.create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 1.0, 0.0])
                .build()
                .unwrap(),
        );
        shape = world.create_hull_shape(
            body,
            &ShapeDef::builder().density(1.0).build().unwrap(),
            &BoxHull::cube(0.5),
        );
        assert!(world.contains_body(body));
        assert!(world.contains_shape(shape));
    }

    let replacement = World::new(WorldDef::default()).unwrap();
    assert!(!replacement.contains_body(body));
    assert!(!replacement.contains_shape(shape));
    assert_eq!(
        replacement.try_body_position(body).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        replacement.try_shape_type(shape).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Shape,
        }
    );
}
