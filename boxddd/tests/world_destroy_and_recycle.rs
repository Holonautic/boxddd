use boxddd::error::HandleKind;
use boxddd::{BodyType, BoxHull, Error};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn dropping_world_makes_body_and_shape_handles_foreign_to_new_worlds() {
    let foundation = foundation();
    let body;
    let shape;
    {
        let mut world = foundation.create_world(foundation.world_def()).unwrap();
        body = world
            .create_body(
                foundation
                    .body_def_builder()
                    .body_type(BodyType::Dynamic)
                    .position([0.0, 1.0, 0.0])
                    .build()
                    .unwrap(),
            )
            .unwrap();
        shape = world
            .create_hull_shape(
                body,
                &foundation.shape_def_builder().density(1.0).build().unwrap(),
                &BoxHull::cube(0.5).unwrap(),
            )
            .unwrap();
        assert!(world.contains_body(body));
        assert!(world.contains_shape(shape));
    }

    let replacement = foundation.create_world(foundation.world_def()).unwrap();
    assert!(!replacement.contains_body(body));
    assert!(!replacement.contains_shape(shape));
    assert_eq!(
        replacement.body_position(body).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        replacement.shape_type(shape).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Shape,
        }
    );
}
