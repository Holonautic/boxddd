use boxddd::error::{HandleKind, InvalidValueReason};
use boxddd::{
    BodyId, BodyType, DistanceJointDef, Error, JointTuning, PrismaticJointDef, Quat, Transform,
    Vec3, World, raw,
};
use std::ffi::c_void;

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

fn assert_close(a: f32, b: f32) {
    assert!((a - b).abs() < 1.0e-5, "{a} != {b}");
}

#[test]
fn common_joint_runtime_setters_getters_and_raw_user_data_work() {
    let (mut world, a, b) = body_pair();
    let joint = world
        .create_distance_joint(
            DistanceJointDef::new(a, b)
                .length(1.0)
                .collide_connected(true)
                .force_threshold(5.0)
                .torque_threshold(6.0)
                .constraint_tuning(JointTuning::new(30.0, 1.25)),
        )
        .unwrap();

    assert_eq!(world.joint_body_a(joint).unwrap(), a);
    assert_eq!(world.joint_body_b(joint).unwrap(), b);
    assert!(world.joint_collide_connected(joint).unwrap());
    world.set_joint_collide_connected(joint, false).unwrap();
    assert!(!world.joint_collide_connected(joint).unwrap());

    let frame_a = Transform::new(Vec3::new(0.1, 0.2, 0.3), Quat::IDENTITY);
    let frame_b = Transform::new(Vec3::new(-0.1, 0.0, 0.25), Quat::IDENTITY);
    world.set_joint_local_frame_a(joint, frame_a).unwrap();
    world.set_joint_local_frame_b(joint, frame_b).unwrap();
    assert_eq!(world.joint_local_frame_a(joint).unwrap(), frame_a);
    assert_eq!(world.joint_local_frame_b(joint).unwrap(), frame_b);

    world
        .set_joint_constraint_tuning(joint, JointTuning::new(12.0, 0.75))
        .unwrap();
    let tuning = world.joint_constraint_tuning(joint).unwrap();
    assert_close(tuning.hertz, 12.0);
    assert_close(tuning.damping_ratio, 0.75);
    world.set_joint_force_threshold(joint, 9.0).unwrap();
    world.set_joint_torque_threshold(joint, 10.0).unwrap();
    assert_close(world.joint_force_threshold(joint).unwrap(), 9.0);
    assert_close(world.joint_torque_threshold(joint).unwrap(), 10.0);

    let mut marker = 7_i32;
    let ptr = (&mut marker as *mut i32).cast::<c_void>();
    unsafe { raw::set_joint_raw_user_data(&mut world, joint, ptr).unwrap() };
    assert_eq!(
        unsafe { raw::joint_raw_user_data(&world, joint).unwrap() },
        ptr
    );

    world.wake_joint_bodies(joint).unwrap();
    assert!(world.joint_constraint_force(joint).unwrap().is_valid());
    assert!(world.joint_constraint_torque(joint).unwrap().is_valid());
    assert!(world.joint_linear_separation(joint).unwrap().is_finite());
    assert!(world.joint_angular_separation(joint).unwrap().is_finite());
}

#[test]
fn typed_joint_runtime_reports_result_errors_for_wrong_family_and_destroyed_ids() {
    let (mut world, a, b) = body_pair();
    let distance = world
        .create_distance_joint(DistanceJointDef::new(a, b).length(1.0))
        .unwrap();
    assert_eq!(
        world.prismatic_joint_translation(distance).unwrap_err(),
        Error::InvalidValue {
            context: "joint.type",
            reason: InvalidValueReason::InvalidCombination,
        }
    );

    let prismatic = world
        .create_prismatic_joint(PrismaticJointDef::new(a, b))
        .unwrap();
    world.destroy_joint(prismatic, true).unwrap();
    assert_eq!(
        world.prismatic_joint_translation(prismatic).unwrap_err(),
        Error::StaleHandle {
            kind: HandleKind::Joint,
        }
    );
}
