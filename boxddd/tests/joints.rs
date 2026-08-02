use boxddd::error::{HandleKind, InvalidValueReason};
use boxddd::{
    BodyId, BodyType, DistanceJointDef, Error, FilterJointDef, JointId, JointType, MotorJointDef,
    ParallelJointDef, PrismaticJointDef, RevoluteJointDef, Sphere, SphericalJointDef, WeldJointDef,
    WheelJointDef, World,
};

fn foundation() -> &'static boxddd::Foundation {
    boxddd::Foundation::initialize_default().unwrap()
}

fn body_pair() -> (World, BodyId, BodyId) {
    let foundation = foundation();
    let mut world = foundation.create_world(foundation.world_def()).unwrap();
    let a = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([-0.5, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let b = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.5, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    (world, a, b)
}

fn contains_joint(joints: &[JointId], joint: JointId) -> bool {
    joints.contains(&joint)
}

#[test]
fn creates_every_joint_family_and_enumerates_body_joints() {
    let (mut world, a, b) = body_pair();

    let joints = [
        (
            world
                .create_parallel_joint(ParallelJointDef::new(a, b))
                .unwrap(),
            JointType::Parallel,
        ),
        (
            world
                .create_distance_joint(DistanceJointDef::new(a, b).length(1.0))
                .unwrap(),
            JointType::Distance,
        ),
        (
            world
                .create_filter_joint(FilterJointDef::new(a, b))
                .unwrap(),
            JointType::Filter,
        ),
        (
            world.create_motor_joint(MotorJointDef::new(a, b)).unwrap(),
            JointType::Motor,
        ),
        (
            world
                .create_prismatic_joint(PrismaticJointDef::new(a, b))
                .unwrap(),
            JointType::Prismatic,
        ),
        (
            world
                .create_revolute_joint(RevoluteJointDef::new(a, b))
                .unwrap(),
            JointType::Revolute,
        ),
        (
            world
                .create_spherical_joint(SphericalJointDef::new(a, b))
                .unwrap(),
            JointType::Spherical,
        ),
        (
            world.create_weld_joint(WeldJointDef::new(a, b)).unwrap(),
            JointType::Weld,
        ),
        (
            world.create_wheel_joint(WheelJointDef::new(a, b)).unwrap(),
            JointType::Wheel,
        ),
    ];

    for (joint, expected_type) in joints {
        assert_eq!(world.contains_joint(joint), Ok(true));
        assert_eq!(world.joint_type(joint).unwrap(), expected_type);
        assert_eq!(world.joint_body_a(joint).unwrap(), a);
        assert_eq!(world.joint_body_b(joint).unwrap(), b);
    }

    let body_a_joints = world.body_joints(a).unwrap();
    let body_b_joints = world.body_joints(b).unwrap();
    for (joint, _) in joints {
        assert!(contains_joint(&body_a_joints, joint));
        assert!(contains_joint(&body_b_joints, joint));
    }

    let removed = joints[0].0;
    world.destroy_joint(removed, true).unwrap();
    assert_eq!(world.contains_joint(removed), Ok(false));
    assert_eq!(
        world.joint_type(removed).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Joint,
        }
    );
    assert!(!contains_joint(&world.body_joints(a).unwrap(), removed));
}

#[test]
fn create_joint_rejects_invalid_stale_and_wrong_world_bodies() {
    let foundation = foundation();
    let (mut world, a, b) = body_pair();
    let stale = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .build()
                .unwrap(),
        )
        .unwrap();
    world.destroy_body(stale).unwrap();

    assert_eq!(
        world
            .create_distance_joint(DistanceJointDef::new(stale, b).length(1.0))
            .unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        world
            .create_distance_joint(DistanceJointDef::new(a, a).length(1.0))
            .unwrap_err(),
        Error::InvalidValue {
            context: "joint.bodies",
            reason: InvalidValueReason::InvalidCombination,
        }
    );

    let mut other_world = foundation.create_world(foundation.world_def()).unwrap();
    let other_body = other_world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .build()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        world
            .create_distance_joint(DistanceJointDef::new(a, other_body).length(1.0))
            .unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
}

#[test]
fn world_rejects_foreign_joint_handles() {
    let (mut world, _, _) = body_pair();
    let (mut other_world, a, b) = body_pair();
    let foreign_joint = other_world
        .create_distance_joint(DistanceJointDef::new(a, b).length(1.0))
        .unwrap();

    assert_eq!(
        world.joint_type(foreign_joint).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Joint,
        }
    );
    assert_eq!(
        world.destroy_joint(foreign_joint, true).unwrap_err(),
        Error::ForeignHandle {
            kind: HandleKind::Joint,
        }
    );
    assert_eq!(world.contains_joint(foreign_joint), Ok(false));
    assert_eq!(other_world.contains_joint(foreign_joint), Ok(true));
    assert_eq!(
        other_world.joint_type(foreign_joint).unwrap(),
        JointType::Distance
    );
}

#[test]
fn distance_joint_keeps_a_simple_scene_within_a_coarse_bound() {
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
    let anchor = world
        .create_body(
            foundation
                .body_def_builder()
                .position([0.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let body = world
        .create_body(
            foundation
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([1.0, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            body,
            &foundation.shape_def_builder().density(1.0).build().unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.25),
        )
        .unwrap();
    let joint = world
        .create_distance_joint(DistanceJointDef::new(anchor, body).length(1.0))
        .unwrap();

    world
        .apply_force_to_center(body, [250.0, 0.0, 0.0], true)
        .unwrap();
    for _ in 0..120 {
        world.step(1.0 / 60.0, 4).unwrap();
    }

    let length = world.distance_joint_current_length(joint).unwrap();
    assert!(length < 1.5, "distance joint stretched too far: {length}");
}
