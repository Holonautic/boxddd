use boxddd::error::{HandleKind, InvalidValueReason};
use boxddd::{
    Aabb, BodyType, BoxHull, Error, Filter, QueryFilter, ShapeCastInput, ShapeProxy, Sphere, Vec3,
    World,
};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

fn query_world() -> (World, Vec<boxddd::ShapeId>) {
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
    let left = world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new([-1.0, 0.0, 0.0], 0.4),
        )
        .unwrap();
    let right = world
        .create_hull_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &BoxHull::cube(0.35).unwrap(),
        )
        .unwrap();
    (world, vec![left, right])
}

#[test]
fn overlap_aabb_owned_into_and_visitor_agree() {
    let (world, shapes) = query_world();
    let aabb = Aabb {
        lower_bound: [-2.0, -1.0, -1.0].into(),
        upper_bound: [1.0, 1.0, 1.0].into(),
    };

    let owned = world.overlap_aabb(aabb, QueryFilter::default()).unwrap();
    let mut into = Vec::from([boxddd::QueryHit {
        shape_id: shapes[0],
    }]);
    world
        .overlap_aabb_into(aabb, QueryFilter::default(), &mut into)
        .unwrap();
    assert_eq!(owned, into);
    assert!(owned.iter().any(|hit| hit.shape_id == shapes[0]));
    assert!(
        owned
            .iter()
            .all(|hit| world.contains_shape(hit.shape_id).unwrap())
    );

    let mut visited = Vec::new();
    world
        .visit_overlap_aabb(aabb, QueryFilter::default(), |shape_id| {
            visited.push(shape_id);
            true
        })
        .unwrap();
    assert_eq!(owned.len(), visited.len());
    assert!(
        visited
            .iter()
            .all(|shape_id| world.contains_shape(*shape_id).unwrap())
    );
}

#[test]
fn overlap_shape_and_ray_casts_find_expected_shapes() {
    let (world, shapes) = query_world();
    let proxy = ShapeProxy::sphere(0.5).unwrap();
    let hits = world
        .overlap_shape([0.0, 0.0, 0.0], &proxy, QueryFilter::default())
        .unwrap();
    assert!(hits.iter().any(|hit| hit.shape_id == shapes[1]));

    let all_ray_hits = world
        .cast_ray([-3.0, 0.0, 0.0], [5.0, 0.0, 0.0], QueryFilter::default())
        .unwrap();
    assert!(all_ray_hits.iter().any(|hit| hit.shape_id == shapes[0]));
    let closest = world
        .cast_ray_closest([-3.0, 0.0, 0.0], [5.0, 0.0, 0.0], QueryFilter::default())
        .unwrap()
        .unwrap();
    assert_eq!(closest.shape_id, shapes[0]);

    let shape_hits = world
        .cast_shape(
            [-3.0, 0.0, 0.0],
            boxddd::ShapeCastInput::new(proxy, [5.0, 0.0, 0.0]).unwrap(),
            QueryFilter::default(),
        )
        .unwrap();
    assert!(!shape_hits.is_empty());

    let mut invalid_input =
        ShapeCastInput::new(ShapeProxy::sphere(0.25).unwrap(), [5.0, 0.0, 0.0]).unwrap();
    invalid_input.translation = Vec3::new(f32::NAN, 0.0, 0.0);
    assert_eq!(
        world
            .cast_shape([-3.0, 0.0, 0.0], invalid_input, QueryFilter::default())
            .unwrap_err(),
        Error::InvalidValue {
            context: "shape_cast.translation",
            reason: InvalidValueReason::NonFinite,
        }
    );
}

#[test]
fn body_scoped_ray_cast_and_shape_cast_find_only_that_body() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let left_body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .position([-1.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let left_shape = world
        .create_sphere_shape(
            left_body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.4),
        )
        .unwrap();
    let right_body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .position([2.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_hull_shape(
            right_body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &BoxHull::cube(0.35).unwrap(),
        )
        .unwrap();

    let ray_hit = world
        .body_cast_ray(
            left_body,
            [-3.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            QueryFilter::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(ray_hit.shape_id, left_shape);
    assert!(ray_hit.fraction > 0.0 && ray_hit.fraction < 1.0);

    let body_miss = world
        .body_cast_ray(
            right_body,
            [-3.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            QueryFilter::default(),
        )
        .unwrap();
    assert!(body_miss.is_none());

    let shape_hit = world
        .body_cast_shape(
            left_body,
            [-3.0, 0.0, 0.0],
            ShapeCastInput::new(ShapeProxy::sphere(0.25).unwrap(), [5.0, 0.0, 0.0]).unwrap(),
            QueryFilter::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(shape_hit.shape_id, left_shape);

    let mut invalid_input =
        ShapeCastInput::new(ShapeProxy::sphere(0.25).unwrap(), [5.0, 0.0, 0.0]).unwrap();
    invalid_input.max_fraction = -1.0;
    assert_eq!(
        world
            .body_cast_shape(
                left_body,
                [-3.0, 0.0, 0.0],
                invalid_input,
                QueryFilter::default(),
            )
            .unwrap_err(),
        Error::InvalidValue {
            context: "shape_cast.max_fraction",
            reason: InvalidValueReason::OutOfRange,
        }
    );
}

#[test]
fn body_scoped_overlap_respects_query_filter() {
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
        .create_sphere_shape(
            body,
            &foundation
                .shape_def_builder()
                .density(1.0)
                .filter(Filter {
                    category_bits: 0b10,
                    mask_bits: u64::MAX,
                    group_index: 0,
                })
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
    let proxy = ShapeProxy::sphere(0.5).unwrap();

    assert!(
        world
            .body_overlap_shape(
                body,
                Vec3::ZERO,
                &proxy,
                QueryFilter::default().mask_bits(0b10),
            )
            .unwrap()
    );
    assert!(
        !world
            .body_overlap_shape(
                body,
                Vec3::ZERO,
                &proxy,
                QueryFilter::default().mask_bits(0b100),
            )
            .unwrap()
    );
}

#[test]
fn shape_scoped_ray_cast_closest_point_and_mass_data_are_typed() {
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
    let sphere = Sphere::new([1.0, 0.0, 0.0], 0.5);
    let shape = world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(2.0).build().unwrap(),
            &sphere,
        )
        .unwrap();

    let hit = world
        .shape_cast_ray(shape, [-1.0, 0.0, 0.0], [4.0, 0.0, 0.0])
        .unwrap()
        .unwrap();
    assert!(hit.fraction > 0.0 && hit.fraction < 1.0);

    let closest = world.shape_closest_point(shape, [3.0, 0.0, 0.0]).unwrap();
    assert!((closest.x - 1.5).abs() < 0.02, "{closest:?}");

    let expected_mass = boxddd::compute_sphere_mass(&sphere, 2.0).unwrap();
    let shape_mass = world.shape_mass_data(shape).unwrap();
    assert!((shape_mass.mass - expected_mass.mass).abs() < 0.001);
}

#[test]
fn scoped_shape_queries_reject_destroyed_and_foreign_shapes() {
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
        .create_sphere_shape(body, &foundation.shape_def(), &Sphere::new(Vec3::ZERO, 0.5))
        .unwrap();

    let mut other_world = foundation.create_world(foundation.world_def()).unwrap();
    let other_body = other_world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Static)
                .build()
                .unwrap(),
        )
        .unwrap();
    let foreign_shape = other_world
        .create_sphere_shape(
            other_body,
            &foundation.shape_def(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();

    assert_eq!(world.contains_shape(foreign_shape), Ok(false));
    assert_eq!(other_world.contains_shape(foreign_shape), Ok(true));
    assert_eq!(
        world
            .shape_closest_point(foreign_shape, Vec3::ZERO)
            .unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Shape,
        }
    );

    world.destroy_shape(shape, true).unwrap();
    assert_eq!(world.contains_shape(shape), Ok(false));
    assert_eq!(
        world
            .shape_cast_ray(shape, [-1.0, 0.0, 0.0], [2.0, 0.0, 0.0])
            .unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Shape,
        }
    );
}

#[test]
fn query_filter_excludes_shapes_by_mask() {
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
    let include = foundation
        .shape_def_builder()
        .filter(Filter {
            category_bits: 0b10,
            mask_bits: u64::MAX,
            group_index: 0,
        })
        .build()
        .unwrap();
    let shape = world
        .create_sphere_shape(body, &include, &Sphere::new(Vec3::ZERO, 0.5))
        .unwrap();
    let aabb = Aabb {
        lower_bound: [-1.0, -1.0, -1.0].into(),
        upper_bound: [1.0, 1.0, 1.0].into(),
    };

    let included = world
        .overlap_aabb(aabb, QueryFilter::default().mask_bits(0b10))
        .unwrap();
    assert!(included.iter().any(|hit| hit.shape_id == shape));
    let excluded = world
        .overlap_aabb(aabb, QueryFilter::default().mask_bits(0b100))
        .unwrap();
    assert!(excluded.is_empty());
}

#[test]
fn invalid_aabb_is_rejected_before_world_query() {
    let (world, _) = query_world();
    let inverted = Aabb {
        lower_bound: [1.0, 0.0, 0.0].into(),
        upper_bound: [-1.0, 0.0, 0.0].into(),
    };
    assert_eq!(
        inverted.validate(),
        Err(Error::InvalidValue {
            context: "aabb.bounds",
            reason: InvalidValueReason::InvalidCombination,
        })
    );
    assert_eq!(
        world
            .overlap_aabb(inverted, QueryFilter::default())
            .unwrap_err(),
        Error::InvalidValue {
            context: "aabb.bounds",
            reason: InvalidValueReason::InvalidCombination,
        }
    );

    let nan = Aabb {
        lower_bound: [f32::NAN, 0.0, 0.0].into(),
        upper_bound: [1.0, 1.0, 1.0].into(),
    };
    assert_eq!(
        world
            .visit_overlap_aabb(nan, QueryFilter::default(), |_| true)
            .unwrap_err(),
        Error::InvalidValue {
            context: "aabb.lower_bound",
            reason: InvalidValueReason::NonFinite,
        }
    );
}

#[test]
fn pure_defaults_are_callback_safe_and_native_metadata_is_rejected() {
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    let filter = QueryFilter::default();
    assert_eq!(filter.category_bits, u64::MAX);
    assert_eq!(filter.mask_bits, u64::MAX);
    assert_eq!(filter.id, 0);
    assert_eq!(boxddd::version().unwrap_err(), Error::InCallback);
    let _ = boxddd::is_double_precision();
}
