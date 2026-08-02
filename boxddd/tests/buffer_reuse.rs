use boxddd::{Aabb, BodyType, QueryFilter, Sphere};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn query_buffers_are_cleared_and_capacity_is_reused() {
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
        .create_sphere_shape(
            body,
            &foundation.shape_def(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    let mut hits = Vec::with_capacity(8);
    hits.push(boxddd::QueryHit { shape_id: shape });
    let before_capacity = hits.capacity();
    let hit_aabb = Aabb {
        lower_bound: [-1.0, -1.0, -1.0].into(),
        upper_bound: [1.0, 1.0, 1.0].into(),
    };
    world
        .overlap_aabb_into(hit_aabb, QueryFilter::default(), &mut hits)
        .unwrap();
    assert_eq!(hits.capacity(), before_capacity);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].shape_id, shape);
    assert_eq!(world.contains_shape(hits[0].shape_id), Ok(true));

    let miss_aabb = Aabb {
        lower_bound: [10.0, 10.0, 10.0].into(),
        upper_bound: [11.0, 11.0, 11.0].into(),
    };
    world
        .overlap_aabb_into(miss_aabb, QueryFilter::default(), &mut hits)
        .unwrap();
    assert_eq!(hits.capacity(), before_capacity);
    assert!(hits.is_empty());
}
