use boxddd::error::InvalidValueReason;
use boxddd::{
    BodyDef, BodyId, BodyType, BoxHull, Capacity, Compound, DistanceJointDef, Error, ExplosionDef,
    Filter, FilterJointDef, Foundation, JointType, MeshData, MotionLocks, MotorJointDef,
    ParallelJointDef, Pos, PrismaticJointDef, Quat, RevoluteJointDef, ShapeDef, Sphere,
    SphericalJointDef, SurfaceMaterial, Vec3, WeldJointDef, WheelJointDef, World, WorldDef,
};
use boxddd_sys::ffi;
use std::fmt::Debug;

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

fn assert_invalid<T: Debug>(
    result: boxddd::Result<T>,
    context: &'static str,
    reason: InvalidValueReason,
) {
    assert_eq!(result.unwrap_err(), Error::InvalidValue { context, reason });
}

fn body_pair() -> (World, BodyId, BodyId) {
    let mut world = foundation().create_world(foundation().world_def()).unwrap();

    let body_a = BodyDef {
        body_type: BodyType::Dynamic,
        position: [-0.5, 0.0, 0.0].into(),
        ..foundation().body_def()
    };

    let body_b = BodyDef {
        body_type: BodyType::Dynamic,
        position: [0.5, 0.0, 0.0].into(),
        ..foundation().body_def()
    };

    let body_a = world.create_body(body_a).unwrap();
    let body_b = world.create_body(body_b).unwrap();
    (world, body_a, body_b)
}

#[test]
fn pure_definition_defaults_match_box3d_defaults() {
    let body = foundation().body_def();
    let raw_body = unsafe { ffi::b3DefaultBodyDef() };
    assert_eq!(body.body_type, BodyType::from_raw(raw_body.type_).unwrap());
    assert_eq!(body.position, Pos::from_raw(raw_body.position));
    assert_eq!(body.rotation, Quat::from_raw(raw_body.rotation));
    assert_eq!(
        body.linear_velocity,
        Vec3::from_raw(raw_body.linearVelocity)
    );
    assert_eq!(
        body.angular_velocity,
        Vec3::from_raw(raw_body.angularVelocity)
    );
    assert_eq!(body.linear_damping, raw_body.linearDamping);
    assert_eq!(body.angular_damping, raw_body.angularDamping);
    assert_eq!(body.gravity_scale, raw_body.gravityScale);
    assert_eq!(body.sleep_threshold, raw_body.sleepThreshold);
    assert_eq!(body.name, None);
    assert_eq!(
        body.motion_locks,
        MotionLocks::from_raw(raw_body.motionLocks)
    );
    assert_eq!(body.enable_sleep, raw_body.enableSleep);
    assert_eq!(body.awake, raw_body.isAwake);
    assert_eq!(body.enabled, raw_body.isEnabled);
    assert_eq!(body.bullet, raw_body.isBullet);
    assert_eq!(body.allow_fast_rotation, raw_body.allowFastRotation);
    assert_eq!(
        body.enable_contact_recycling,
        raw_body.enableContactRecycling
    );

    let material = SurfaceMaterial::default();
    let raw_material = unsafe { ffi::b3DefaultSurfaceMaterial() };
    assert_eq!(material, SurfaceMaterial::from_raw(raw_material));

    let shape = foundation().shape_def();
    let raw_shape = unsafe { ffi::b3DefaultShapeDef() };
    assert_eq!(shape.name, None);
    assert!(shape.materials.is_empty());
    assert_eq!(
        shape.base_material,
        SurfaceMaterial::from_raw(raw_shape.baseMaterial)
    );
    assert_eq!(shape.density, raw_shape.density);
    assert_eq!(shape.explosion_scale, raw_shape.explosionScale);
    assert_eq!(shape.filter, Filter::from_raw(raw_shape.filter));
    assert_eq!(
        shape.enable_custom_filtering,
        raw_shape.enableCustomFiltering
    );
    assert_eq!(shape.sensor, raw_shape.isSensor);
    assert_eq!(shape.enable_sensor_events, raw_shape.enableSensorEvents);
    assert_eq!(shape.enable_contact_events, raw_shape.enableContactEvents);
    assert_eq!(shape.enable_hit_events, raw_shape.enableHitEvents);
    assert_eq!(
        shape.enable_pre_solve_events,
        raw_shape.enablePreSolveEvents
    );
    assert_eq!(
        shape.invoke_contact_creation,
        raw_shape.invokeContactCreation
    );
    assert_eq!(shape.update_body_mass, raw_shape.updateBodyMass);
    assert_eq!(
        shape.enable_speculative_contact,
        raw_shape.enableSpeculativeContact
    );

    let world = foundation().world_def();
    let raw_world = unsafe { ffi::b3DefaultWorldDef() };
    assert_eq!(world.gravity, Vec3::from_raw(raw_world.gravity));
    assert_eq!(world.restitution_threshold, raw_world.restitutionThreshold);
    assert_eq!(world.hit_event_threshold, raw_world.hitEventThreshold);
    assert_eq!(world.contact_hertz, raw_world.contactHertz);
    assert_eq!(world.contact_damping_ratio, raw_world.contactDampingRatio);
    assert_eq!(world.contact_speed, raw_world.contactSpeed);
    assert_eq!(world.maximum_linear_speed, raw_world.maximumLinearSpeed);
    assert_eq!(world.enable_sleep, raw_world.enableSleep);
    assert_eq!(world.enable_continuous, raw_world.enableContinuous);
    assert_eq!(world.worker_count, raw_world.workerCount);
    assert_eq!(world.capacity, Capacity::from_raw(raw_world.capacity));
    assert!(world.task_system.is_none());

    let explosion = ExplosionDef::default();
    let raw_explosion = unsafe { ffi::b3DefaultExplosionDef() };
    assert_eq!(explosion.mask_bits, raw_explosion.maskBits);
    assert_eq!(explosion.position, Pos::from_raw(raw_explosion.position));
    assert_eq!(explosion.radius, raw_explosion.radius);
    assert_eq!(explosion.falloff, raw_explosion.falloff);
    assert_eq!(explosion.impulse_per_area, raw_explosion.impulsePerArea);
}

#[test]
fn pure_definition_defaults_preserve_box3d_creation_behavior() {
    let material = SurfaceMaterial::default();
    material.validate().unwrap();

    let shape_def = foundation().shape_def();
    shape_def.validate().unwrap();

    let body_def = foundation().body_def();
    body_def.validate().unwrap();

    let world_def = foundation().world_def();
    world_def.validate().unwrap();

    let mut world = foundation().create_world(world_def).unwrap();
    let body = world.create_body(body_def).unwrap();
    assert_eq!(world.body_type(body).unwrap(), BodyType::Static);

    let shape = world
        .create_sphere_shape(body, &shape_def, &Sphere::new(Vec3::ZERO, 0.5))
        .unwrap();
    assert_eq!(world.shape_name(shape).unwrap(), "");

    let mut explosion = ExplosionDef::default();
    assert_eq!(explosion.mask_bits, u64::MAX);
    assert_eq!(explosion.radius, 0.0);
    explosion.validate().unwrap();

    explosion.radius = 1.0;
    explosion.impulse_per_area = 1.0;
    explosion.validate().unwrap();
    world.explode(&explosion).unwrap();
}

#[test]
fn pure_definitions_reject_invalid_numeric_values() {
    let body = BodyDef {
        linear_damping: -0.1,
        ..foundation().body_def()
    };
    assert_invalid(
        body.validate(),
        "body.linear_damping",
        InvalidValueReason::OutOfRange,
    );

    let material = SurfaceMaterial {
        friction: f32::NAN,
        ..SurfaceMaterial::default()
    };
    assert_invalid(
        material.validate(),
        "surface_material.friction",
        InvalidValueReason::NonFinite,
    );

    let shape = ShapeDef {
        density: -1.0,
        ..foundation().shape_def()
    };
    assert_invalid(
        shape.validate(),
        "shape.density",
        InvalidValueReason::OutOfRange,
    );

    let world = WorldDef {
        maximum_linear_speed: 0.0,
        ..foundation().world_def()
    };
    assert_invalid(
        world.validate(),
        "world.maximum_linear_speed",
        InvalidValueReason::OutOfRange,
    );

    let explosion = ExplosionDef {
        radius: 1.0,
        falloff: f32::INFINITY,
        ..ExplosionDef::default()
    };
    assert_invalid(
        explosion.validate(),
        "explosion.falloff",
        InvalidValueReason::NonFinite,
    );
}

#[test]
fn definition_names_with_interior_nul_are_recoverable_errors() {
    let mut world = foundation().create_world(foundation().world_def()).unwrap();

    let body_def = BodyDef {
        name: Some("invalid\0body".to_owned()),
        ..foundation().body_def()
    };
    assert_invalid(
        world.create_body(body_def),
        "body.name",
        InvalidValueReason::InteriorNul,
    );

    let body = world.create_body(foundation().body_def()).unwrap();
    let shape_def = ShapeDef {
        name: Some("invalid\0shape".to_owned()),
        ..foundation().shape_def()
    };
    assert_invalid(
        world.create_sphere_shape(body, &shape_def, &Sphere::new(Vec3::ZERO, 0.5)),
        "shape.name",
        InvalidValueReason::InteriorNul,
    );
}

#[test]
fn builders_and_cross_field_validation_report_typed_errors() {
    assert_invalid(
        foundation()
            .body_def_builder()
            .name("invalid\0body")
            .build(),
        "body.name",
        InvalidValueReason::InteriorNul,
    );
    assert_invalid(
        foundation()
            .shape_def_builder()
            .name("invalid\0shape")
            .build(),
        "shape.name",
        InvalidValueReason::InteriorNul,
    );

    let world = WorldDef {
        worker_count: ffi::B3_MAX_WORKERS + 1,
        ..foundation().world_def()
    };
    assert_invalid(
        world.validate(),
        "world.worker_count",
        InvalidValueReason::OutOfRange,
    );

    let mut world = foundation().world_def();
    world.capacity.static_body_count = i32::MAX;
    world.capacity.dynamic_body_count = 1;
    assert_invalid(
        world.validate(),
        "world.capacity.body_count",
        InvalidValueReason::OutOfRange,
    );

    let explosion = ExplosionDef {
        radius: f32::MAX,
        falloff: f32::MAX,
        ..Default::default()
    };
    assert_invalid(
        explosion.validate(),
        "explosion.extent",
        InvalidValueReason::NonFinite,
    );
}

#[test]
fn shape_material_lowering_is_geometry_aware() {
    let mut world = foundation().create_world(foundation().world_def()).unwrap();
    let body = world.create_body(foundation().body_def()).unwrap();
    let mut def = foundation().shape_def();
    def.base_material.user_material_id = 11;
    def.materials = vec![SurfaceMaterial {
        user_material_id: 22,
        ..SurfaceMaterial::default()
    }];

    let hull = world
        .create_hull_shape(body, &def, &BoxHull::cube(0.5).unwrap())
        .unwrap();
    assert_eq!(
        world.shape_surface_material(hull).unwrap().user_material_id,
        11
    );

    let mesh = world
        .create_mesh_shape(
            body,
            &def,
            MeshData::box_mesh(Vec3::ZERO, [0.5, 0.5, 0.5], true).unwrap(),
            [1.0, 1.0, 1.0],
        )
        .unwrap();
    assert_eq!(world.shape_mesh_material_count(mesh).unwrap(), 1);
    assert_eq!(
        world
            .shape_mesh_surface_material(mesh, 0)
            .unwrap()
            .user_material_id,
        22
    );
}

#[test]
fn compound_sensor_is_rejected_before_native_creation() {
    let mut world = foundation().create_world(foundation().world_def()).unwrap();
    let body = world.create_body(foundation().body_def()).unwrap();
    let def = ShapeDef {
        sensor: true,
        ..foundation().shape_def()
    };
    let compound =
        Compound::single_sphere(Sphere::new(Vec3::ZERO, 0.5), SurfaceMaterial::default()).unwrap();

    assert_invalid(
        world.create_compound_shape(body, &def, compound),
        "compound_shape.sensor",
        InvalidValueReason::InvalidCombination,
    );
}

#[test]
fn cloned_definition_names_survive_source_and_definition_drop() {
    let mut world = foundation().create_world(foundation().world_def()).unwrap();

    let body_def = {
        let source = BodyDef {
            name: Some("owned-body".to_owned()),
            ..foundation().body_def()
        };
        source.clone()
    };
    let body = world.create_body(body_def).unwrap();
    assert_eq!(
        world.body_name(body).unwrap().as_deref(),
        Some("owned-body")
    );

    let shape = {
        let shape_def = {
            let source = ShapeDef {
                name: Some("owned-shape".to_owned()),
                ..foundation().shape_def()
            };
            source.clone()
        };
        world
            .create_sphere_shape(body, &shape_def, &Sphere::new(Vec3::ZERO, 0.5))
            .unwrap()
    };
    assert_eq!(world.shape_name(shape).unwrap(), "owned-shape");
}

#[test]
fn every_joint_definition_creates_its_expected_family_with_defaults() {
    let (mut world, body_a, body_b) = body_pair();

    let joints = [
        (
            world
                .create_parallel_joint(ParallelJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Parallel,
        ),
        (
            world
                .create_distance_joint(DistanceJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Distance,
        ),
        (
            world
                .create_filter_joint(FilterJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Filter,
        ),
        (
            world
                .create_motor_joint(MotorJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Motor,
        ),
        (
            world
                .create_prismatic_joint(PrismaticJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Prismatic,
        ),
        (
            world
                .create_revolute_joint(RevoluteJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Revolute,
        ),
        (
            world
                .create_spherical_joint(SphericalJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Spherical,
        ),
        (
            world
                .create_weld_joint(WeldJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Weld,
        ),
        (
            world
                .create_wheel_joint(WheelJointDef::new(body_a, body_b))
                .unwrap(),
            JointType::Wheel,
        ),
    ];

    for (joint, expected) in joints {
        assert_eq!(world.joint_type(joint).unwrap(), expected);
    }
}

#[test]
fn every_joint_definition_rejects_invalid_numeric_values() {
    let (mut world, body_a, body_b) = body_pair();

    assert_invalid(
        world.create_parallel_joint(ParallelJointDef::new(body_a, body_b).spring(-1.0, 1.0, 1.0)),
        "parallel_joint.hertz",
        InvalidValueReason::OutOfRange,
    );
    assert_invalid(
        world.create_distance_joint(DistanceJointDef::new(body_a, body_b).length(0.0)),
        "distance_joint.length",
        InvalidValueReason::OutOfRange,
    );
    assert_invalid(
        world.create_filter_joint(FilterJointDef::new(body_a, body_b).draw_scale(f32::NAN)),
        "joint.draw_scale",
        InvalidValueReason::NonFinite,
    );
    assert_invalid(
        world.create_motor_joint(MotorJointDef::new(body_a, body_b).linear_velocity([
            f32::NAN,
            0.0,
            0.0,
        ])),
        "motor_joint.linear_velocity",
        InvalidValueReason::NonFinite,
    );
    assert_invalid(
        world.create_prismatic_joint(PrismaticJointDef::new(body_a, body_b).limit(true, 1.0, -1.0)),
        "prismatic_joint.translation_range",
        InvalidValueReason::InvalidCombination,
    );
    assert_invalid(
        world.create_revolute_joint(
            RevoluteJointDef::new(body_a, body_b).target_angle(f32::INFINITY),
        ),
        "revolute_joint.target_angle",
        InvalidValueReason::NonFinite,
    );
    assert_invalid(
        world.create_revolute_joint(
            RevoluteJointDef::new(body_a, body_b).target_angle(std::f32::consts::PI + 0.01),
        ),
        "revolute_joint.target_angle",
        InvalidValueReason::OutOfRange,
    );
    assert_invalid(
        world.create_spherical_joint(
            SphericalJointDef::new(body_a, body_b)
                .cone_limit(true, std::f32::consts::FRAC_PI_2 + 0.01),
        ),
        "spherical_joint.cone_angle",
        InvalidValueReason::OutOfRange,
    );
    assert_invalid(
        world.create_weld_joint(WeldJointDef::new(body_a, body_b).angular_tuning(f32::NAN, 1.0)),
        "weld_joint.angular_hertz",
        InvalidValueReason::NonFinite,
    );
    assert_invalid(
        world.create_wheel_joint(WheelJointDef::new(body_a, body_b).spin_motor(true, 0.0, -1.0)),
        "wheel_joint.max_spin_torque",
        InvalidValueReason::OutOfRange,
    );
}
