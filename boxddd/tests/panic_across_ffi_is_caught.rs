use boxddd::{
    Aabb, BodyType, Compound, Error, HeightField, MeshData, QueryFilter, Sphere, SurfaceMaterial,
};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn query_callback_panic_is_caught_before_crossing_ffi_boundary() {
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
            &foundation.shape_def(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();
    let aabb = Aabb {
        lower_bound: [-1.0, -1.0, -1.0].into(),
        upper_bound: [1.0, 1.0, 1.0].into(),
    };

    let err = world
        .visit_overlap_aabb(aabb, QueryFilter::default(), |_| panic!("visitor panic"))
        .unwrap_err();
    assert_eq!(err, Error::CallbackPanicked);
}

#[test]
fn compound_query_callback_panic_is_caught_before_crossing_ffi_boundary() {
    foundation();
    let compound = Compound::single_sphere(
        Sphere::new([0.0, 0.0, 0.0], 0.5),
        SurfaceMaterial::default(),
    )
    .unwrap();
    let aabb = Aabb {
        lower_bound: [-1.0, -1.0, -1.0].into(),
        upper_bound: [1.0, 1.0, 1.0].into(),
    };

    let err = compound
        .visit_query_aabb(aabb, |_| panic!("visitor panic"))
        .unwrap_err();
    assert_eq!(err, Error::CallbackPanicked);
}

#[test]
fn mesh_query_callback_panic_is_caught_before_crossing_ffi_boundary() {
    foundation();
    let mesh = MeshData::box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], true).unwrap();
    let aabb = Aabb {
        lower_bound: [-1.25, -1.25, -1.25].into(),
        upper_bound: [1.25, 1.25, 1.25].into(),
    };

    let err = mesh
        .visit_triangles(aabb, [1.0, 1.0, 1.0], |_| panic!("visitor panic"))
        .unwrap_err();
    assert_eq!(err, Error::CallbackPanicked);
}

#[test]
fn height_field_query_callback_panic_is_caught_before_crossing_ffi_boundary() {
    foundation();
    let height_field = HeightField::grid(3, 3, [1.0, 1.0, 1.0], false).unwrap();
    let aabb = Aabb {
        lower_bound: [-0.25, -1.0, -0.25].into(),
        upper_bound: [2.25, 1.0, 2.25].into(),
    };

    let err = height_field
        .visit_triangles(aabb, |_| panic!("visitor panic"))
        .unwrap_err();
    assert_eq!(err, Error::CallbackPanicked);
}
