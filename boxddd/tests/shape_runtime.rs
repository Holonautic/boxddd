use boxddd::error::InvalidValueReason;
use boxddd::{BodyType, Capsule, Error, Filter, Hull, MeshData, Sphere, SurfaceMaterial};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

#[test]
fn shape_runtime_properties_and_geometry_can_be_updated() {
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
            &foundation
                .shape_def_builder()
                .density(1.0)
                .friction(0.3)
                .build()
                .unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    assert_eq!(world.shape_body(shape).unwrap(), body);
    assert!(!world.shape_sensor(shape).unwrap());
    world.set_shape_density(shape, 2.0, true).unwrap();
    assert_eq!(world.shape_density(shape).unwrap(), 2.0);
    world.set_shape_friction(shape, 0.7).unwrap();
    assert_eq!(world.shape_friction(shape).unwrap(), 0.7);
    world.set_shape_restitution(shape, 0.4).unwrap();
    assert_eq!(world.shape_restitution(shape).unwrap(), 0.4);

    let material = SurfaceMaterial {
        friction: 0.2,
        restitution: 0.1,
        ..Default::default()
    };
    world.set_shape_surface_material(shape, material).unwrap();
    assert_eq!(world.shape_surface_material(shape).unwrap(), material);
    assert_eq!(
        world
            .set_shape_surface_material(
                shape,
                SurfaceMaterial {
                    rolling_resistance: -0.1,
                    ..Default::default()
                }
            )
            .unwrap_err(),
        Error::InvalidValue {
            context: "surface_material.rolling_resistance",
            reason: InvalidValueReason::OutOfRange,
        }
    );

    let filter = Filter {
        category_bits: 2,
        mask_bits: 4,
        group_index: -1,
    };
    world.set_shape_filter(shape, filter, false).unwrap();
    assert_eq!(world.shape_filter(shape).unwrap(), filter);
    world.enable_shape_sensor_events(shape, true).unwrap();
    assert!(world.shape_sensor_events_enabled(shape).unwrap());
    world.enable_shape_contact_events(shape, true).unwrap();
    assert!(world.shape_contact_events_enabled(shape).unwrap());
    world.enable_shape_pre_solve_events(shape, true).unwrap();
    assert!(world.shape_pre_solve_events_enabled(shape).unwrap());
    world.enable_shape_hit_events(shape, true).unwrap();
    assert!(world.shape_hit_events_enabled(shape).unwrap());

    let replacement_sphere = Sphere::new([1.0, 0.0, 0.0], 0.25);
    world.set_shape_sphere(shape, &replacement_sphere).unwrap();
    assert_eq!(world.shape_sphere(shape).unwrap(), replacement_sphere);
    let _ = world.shape_aabb(shape).unwrap();

    let capsule_shape = world
        .create_capsule_shape(
            body,
            &foundation.shape_def(),
            &Capsule::new([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.25),
        )
        .unwrap();
    let replacement_capsule = Capsule::new([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2);
    world
        .set_shape_capsule(capsule_shape, &replacement_capsule)
        .unwrap();
    assert_eq!(
        world.shape_capsule(capsule_shape).unwrap(),
        replacement_capsule
    );

    let hull_shape = world
        .create_created_hull_shape(body, &foundation.shape_def(), &Hull::rock(0.5).unwrap())
        .unwrap();
    let new_hull = Hull::cylinder(1.0, 0.25, 0.0, 8).unwrap();
    world.set_shape_hull(hull_shape, &new_hull).unwrap();

    let mesh_shape = world
        .create_mesh_shape(
            body,
            &foundation.shape_def(),
            MeshData::box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0], true).unwrap(),
            [1.0, 1.0, 1.0],
        )
        .unwrap();
    assert_eq!(
        world
            .set_shape_mesh_material(mesh_shape, 999, SurfaceMaterial::default())
            .unwrap_err(),
        Error::InvalidValue {
            context: "shape.material_index",
            reason: InvalidValueReason::OutOfRange,
        }
    );
    let mesh_material = SurfaceMaterial {
        friction: 0.9,
        restitution: 0.1,
        user_material_id: 42,
        ..Default::default()
    };
    world
        .set_shape_mesh_material(mesh_shape, 0, mesh_material)
        .unwrap();
    assert_eq!(
        world.shape_mesh_surface_material(mesh_shape, 0).unwrap(),
        mesh_material
    );
    assert_eq!(
        world
            .shape_mesh_surface_material(mesh_shape, 999)
            .unwrap_err(),
        Error::InvalidValue {
            context: "shape.material_index",
            reason: InvalidValueReason::OutOfRange,
        }
    );
    world
        .set_shape_mesh(
            mesh_shape,
            MeshData::box_mesh([0.0, 0.0, 0.0], [0.5, 0.5, 0.5], true).unwrap(),
            [1.0, 1.0, 1.0],
        )
        .unwrap();

    world.destroy_shape(shape, true).unwrap();
    assert_eq!(world.contains_shape(shape), Ok(false));
}

#[test]
fn apply_shape_wind_changes_dynamic_body_velocity_and_validates_inputs() {
    let foundation = foundation();
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity([0.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let shape = world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    world
        .apply_shape_wind(shape, [8.0, 0.0, 0.0], 1.0, 0.0, 10.0, true)
        .unwrap();
    world.step(1.0 / 60.0, 4).unwrap();
    assert!(world.body_linear_velocity(body).unwrap().x > 0.0);

    assert_eq!(
        world
            .apply_shape_wind(shape, [f32::NAN, 0.0, 0.0], 1.0, 0.0, 10.0, true)
            .unwrap_err(),
        Error::InvalidValue {
            context: "shape.wind",
            reason: InvalidValueReason::NonFinite,
        }
    );
    assert_eq!(
        world
            .apply_shape_wind(shape, [1.0, 0.0, 0.0], -1.0, 0.0, 10.0, true)
            .unwrap_err(),
        Error::InvalidValue {
            context: "shape.wind_drag",
            reason: InvalidValueReason::OutOfRange,
        }
    );
}
