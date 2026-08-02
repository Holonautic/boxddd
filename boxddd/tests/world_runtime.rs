use boxddd::error::HandleKind;
use boxddd::{
    BodyType, DistanceJointDef, Error, Foundation, MotionLocks, QueryFilter, ShapeProxy, Sphere,
    Vec3,
};

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

#[test]
fn world_runtime_tuning_and_metrics_are_safe() {
    let foundation = foundation();
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity([0.0, -9.8, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();

    world.enable_sleeping(false).unwrap();
    assert!(!world.sleeping_enabled().unwrap());
    world.enable_sleeping(true).unwrap();
    assert!(world.sleeping_enabled().unwrap());

    world.enable_continuous(false).unwrap();
    assert!(!world.continuous_enabled().unwrap());
    world.enable_continuous(true).unwrap();
    assert!(world.continuous_enabled().unwrap());

    world.enable_warm_starting(false).unwrap();
    assert!(!world.warm_starting_enabled().unwrap());
    world.enable_warm_starting(true).unwrap();
    assert!(world.warm_starting_enabled().unwrap());

    world.enable_speculative(true).unwrap();
    world.set_restitution_threshold(2.0).unwrap();
    assert_eq!(world.restitution_threshold().unwrap(), 2.0);
    world.set_hit_event_threshold(3.0).unwrap();
    assert_eq!(world.hit_event_threshold().unwrap(), 3.0);
    world.set_contact_tuning(60.0, 0.7, 10.0).unwrap();
    world.set_contact_recycle_distance(0.02).unwrap();
    assert_eq!(world.contact_recycle_distance().unwrap(), 0.02);
    world.set_maximum_linear_speed(120.0).unwrap();
    assert_eq!(world.maximum_linear_speed().unwrap(), 120.0);
    world.set_worker_count(0).unwrap();
    assert!(world.worker_count().unwrap() >= 0);

    let counters = world.counters().unwrap();
    assert_eq!(counters.body_count, 0);
    let profile = world.profile().unwrap();
    assert!(profile.step >= 0.0);
    let capacity = world.max_capacity().unwrap();
    assert!(capacity.dynamic_body_count >= 0);
    world.rebuild_static_tree().unwrap();
}

#[test]
fn body_runtime_setters_getters_and_buffers_work() {
    let foundation = foundation();
    let mut world = foundation
        .create_world(
            foundation
                .world_def_builder()
                .gravity([0.0, -9.8, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 3.0, 0.0])
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

    assert_eq!(world.body_type(body).unwrap(), BodyType::Dynamic);
    world.set_body_name(body, "runtime-body").unwrap();
    assert_eq!(
        world.body_name(body).unwrap().as_deref(),
        Some("runtime-body")
    );
    world.set_body_fast_rotation_allowed(body, true).unwrap();
    assert!(world.body_fast_rotation_allowed(body).unwrap());
    world.set_shape_name(shape, "runtime-shape").unwrap();
    assert_eq!(world.shape_name(shape).unwrap(), "runtime-shape");

    world
        .set_body_transform(body, [1.0, 4.0, 2.0], Default::default())
        .unwrap();
    assert_eq!(world.body_position(body).unwrap().x, 1.0);
    assert_eq!(world.body_transform(body).unwrap().p.z, 2.0);
    assert_eq!(world.body_rotation(body).unwrap(), Default::default());

    world
        .set_body_linear_velocity(body, [1.0, 2.0, 3.0])
        .unwrap();
    assert_eq!(
        world.body_linear_velocity(body).unwrap(),
        Vec3::new(1.0, 2.0, 3.0)
    );
    assert_eq!(
        world.body_local_point_velocity(body, Vec3::ZERO).unwrap(),
        world.body_linear_velocity(body).unwrap()
    );
    assert_eq!(
        world
            .body_world_point_velocity(body, [1.0, 4.0, 2.0])
            .unwrap(),
        world.body_linear_velocity(body).unwrap()
    );
    world
        .set_body_angular_velocity(body, [0.1, 0.2, 0.3])
        .unwrap();
    assert_eq!(
        world.body_angular_velocity(body).unwrap(),
        Vec3::new(0.1, 0.2, 0.3)
    );

    world.set_body_linear_damping(body, 0.4).unwrap();
    assert_eq!(world.body_linear_damping(body).unwrap(), 0.4);
    world.set_body_angular_damping(body, 0.5).unwrap();
    assert_eq!(world.body_angular_damping(body).unwrap(), 0.5);
    world.set_body_gravity_scale(body, 0.25).unwrap();
    assert_eq!(world.body_gravity_scale(body).unwrap(), 0.25);

    let locks = MotionLocks::new(true, false, true, false, true, false);
    world.set_body_motion_locks(body, locks).unwrap();
    assert_eq!(world.body_motion_locks(body).unwrap(), locks);
    world
        .set_body_motion_locks(body, MotionLocks::default())
        .unwrap();
    world.set_body_bullet(body, true).unwrap();
    assert!(world.body_bullet(body).unwrap());
    world.enable_body_contact_recycling(body, false).unwrap();
    assert!(!world.body_contact_recycling_enabled(body).unwrap());
    world.enable_body_hit_events(body, true).unwrap();

    assert!(world.body_mass(body).unwrap() > 0.0);
    assert!(world.body_inverse_mass(body).unwrap() > 0.0);
    let mass_data = world.body_mass_data(body).unwrap();
    world.set_body_mass_data(body, mass_data).unwrap();
    world.apply_mass_from_shapes(body).unwrap();
    let _ = world.body_local_rotational_inertia(body).unwrap();
    let _ = world.body_world_inverse_rotational_inertia(body).unwrap();
    let _ = world.body_local_center_of_mass(body).unwrap();
    let _ = world.body_world_center_of_mass(body).unwrap();
    let _ = world.body_aabb(body).unwrap();

    world
        .apply_force_to_center(body, [1.0, 0.0, 0.0], true)
        .unwrap();
    world
        .apply_force(
            body,
            [0.0, 1.0, 0.0],
            world.body_position(body).unwrap(),
            true,
        )
        .unwrap();
    world.apply_torque(body, [0.0, 0.0, 1.0], true).unwrap();
    world
        .apply_linear_impulse_to_center(body, [1.0, 0.0, 0.0], true)
        .unwrap();
    world
        .apply_linear_impulse(
            body,
            [0.0, 1.0, 0.0],
            world.body_position(body).unwrap(),
            true,
        )
        .unwrap();
    world
        .apply_angular_impulse(body, [0.0, 0.0, 0.1], true)
        .unwrap();
    world.step(1.0 / 60.0, 4).unwrap();
    assert!(world.body_linear_velocity(body).unwrap().x > 0.0);

    let mut shapes = vec![shape];
    world.body_shapes_into(body, &mut shapes).unwrap();
    assert_eq!(shapes, vec![shape]);
    assert_eq!(world.body_shapes(body).unwrap(), vec![shape]);

    let contact_body = world.create_body(foundation.body_def()).unwrap();
    world
        .create_sphere_shape(
            contact_body,
            &foundation.shape_def(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
    let touching_body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.75, 0.0, 0.0])
                .gravity_scale(0.0)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            touching_body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();
    let mut contacts = Vec::new();
    for _ in 0..8 {
        world.step(1.0 / 60.0, 4).unwrap();
        contacts = world.body_contacts(contact_body).unwrap();
        if !contacts.is_empty() {
            break;
        }
    }
    assert!(!contacts.is_empty());

    let unrelated_joint = world
        .create_distance_joint(DistanceJointDef::new(contact_body, touching_body).length(0.75))
        .unwrap();
    let mut joints = vec![unrelated_joint];
    world.body_joints_into(body, &mut joints).unwrap();
    assert!(joints.is_empty());

    world.body_contacts_into(body, &mut contacts).unwrap();
    assert!(contacts.is_empty());
}

#[test]
fn body_runtime_respects_callback_guard() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let body = world.create_body(foundation.body_def()).unwrap();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(
        world.body_linear_velocity(body).unwrap_err(),
        boxddd::Error::InCallback
    );
}

#[test]
fn world_rejects_foreign_body_and_shape_handles() {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let local = world.create_body(foundation.body_def()).unwrap();

    let mut other = foundation.create_world(foundation.world_def()).unwrap();
    let foreign_body = other.create_body(foundation.body_def()).unwrap();
    let foreign_shape = other
        .create_sphere_shape(
            foreign_body,
            &foundation.shape_def(),
            &Sphere::new(Vec3::ZERO, 0.5),
        )
        .unwrap();

    assert_eq!(world.contains_body(foreign_body), Ok(false));
    assert_eq!(other.contains_body(foreign_body), Ok(true));
    assert_eq!(
        world.body_position(foreign_body).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        world
            .body_closest_point(foreign_body, Vec3::ZERO)
            .unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        world
            .body_overlap_shape(
                foreign_body,
                Vec3::ZERO,
                &ShapeProxy::sphere(0.5).unwrap(),
                QueryFilter::default()
            )
            .unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        world.destroy_body(foreign_body).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(other.contains_body(foreign_body), Ok(true));
    assert_eq!(
        world
            .create_sphere_shape(
                foreign_body,
                &foundation.shape_def(),
                &Sphere::new(Vec3::ZERO, 0.5)
            )
            .unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(world.contains_shape(foreign_shape), Ok(false));
    assert_eq!(other.contains_shape(foreign_shape), Ok(true));
    assert_eq!(
        world.shape_body(foreign_shape).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Shape,
        }
    );
    assert_eq!(
        world.destroy_shape(foreign_shape, true).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Shape,
        }
    );
    assert_eq!(other.contains_shape(foreign_shape), Ok(true));

    world.destroy_body(local).unwrap();
}
