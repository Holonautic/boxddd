use boxddd::error::HandleKind;
use boxddd::{
    BodyId, BodyType, DistanceJointDef, Error, Foundation, JointId, ShapeId, Sphere, Vec3, World,
    raw,
};
use static_assertions::assert_not_impl_any;
use std::ffi::c_void;

assert_not_impl_any!(raw::WorldRawParts: Send, Sync);
assert_not_impl_any!(raw::WorldRawGuard<'static>: Send, Sync);

fn foundation() -> &'static Foundation {
    Foundation::initialize_default().unwrap()
}

fn scene() -> (World, BodyId, BodyId, ShapeId, JointId) {
    let mut world = foundation().create_world(foundation().world_def()).unwrap();
    let a = world
        .create_body(
            foundation()
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([-0.5, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let b = world
        .create_body(
            foundation()
                .body_def_builder()
                .body_type(BodyType::Dynamic)
                .position([0.5, 0.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let shape = world
        .create_sphere_shape(a, &foundation().shape_def(), &Sphere::new(Vec3::ZERO, 0.25))
        .unwrap();
    let joint = world
        .create_distance_joint(DistanceJointDef::new(a, b).length(1.0))
        .unwrap();
    (world, a, b, shape, joint)
}

#[test]
fn raw_user_data_round_trips_for_world_body_shape_and_joint() {
    let (mut world, body, _, shape, joint) = scene();

    let mut world_marker = 1_i32;
    let mut body_marker = 2_i32;
    let mut shape_marker = 3_i32;
    let mut joint_marker = 4_i32;

    let world_ptr = (&mut world_marker as *mut i32).cast::<c_void>();
    let body_ptr = (&mut body_marker as *mut i32).cast::<c_void>();
    let shape_ptr = (&mut shape_marker as *mut i32).cast::<c_void>();
    let joint_ptr = (&mut joint_marker as *mut i32).cast::<c_void>();

    unsafe {
        raw::set_world_raw_user_data(&mut world, world_ptr).unwrap();
        raw::set_body_raw_user_data(&mut world, body, body_ptr).unwrap();
        raw::set_shape_raw_user_data(&mut world, shape, shape_ptr).unwrap();
        raw::set_joint_raw_user_data(&mut world, joint, joint_ptr).unwrap();

        assert_eq!(raw::world_raw_user_data(&world).unwrap(), world_ptr);
        assert_eq!(raw::body_raw_user_data(&world, body).unwrap(), body_ptr);
        assert_eq!(raw::shape_raw_user_data(&world, shape).unwrap(), shape_ptr);
        assert_eq!(raw::joint_raw_user_data(&world, joint).unwrap(), joint_ptr);
    }
}

#[test]
fn raw_user_data_rejects_foreign_handles() {
    let (mut world, _, _, _, _) = scene();
    let (mut other_world, other_body, _, other_shape, other_joint) = scene();
    let mut marker = 7_i32;
    let ptr = (&mut marker as *mut i32).cast::<c_void>();

    assert_eq!(
        unsafe { raw::set_body_raw_user_data(&mut world, other_body, ptr).unwrap_err() },
        Error::ForeignHandle {
            kind: HandleKind::Body,
        }
    );
    assert_eq!(
        unsafe { raw::shape_raw_user_data(&world, other_shape).unwrap_err() },
        Error::ForeignHandle {
            kind: HandleKind::Shape,
        }
    );
    assert_eq!(
        unsafe { raw::set_joint_raw_user_data(&mut world, other_joint, ptr).unwrap_err() },
        Error::ForeignHandle {
            kind: HandleKind::Joint,
        }
    );

    unsafe { raw::set_world_raw_user_data(&mut other_world, ptr).unwrap() };
    assert_eq!(
        unsafe { raw::world_raw_user_data(&other_world).unwrap() },
        ptr
    );
}

#[test]
fn raw_interop_obeys_callback_guard() {
    let (mut world, body, _, _, _) = scene();
    let mut marker = 9_i32;
    let ptr = (&mut marker as *mut i32).cast::<c_void>();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(
        unsafe { raw::set_world_raw_user_data(&mut world, ptr).unwrap_err() },
        Error::InCallback
    );
    assert_eq!(
        unsafe { raw::set_body_raw_user_data(&mut world, body, ptr).unwrap_err() },
        Error::InCallback
    );
}

#[test]
fn scoped_guard_and_owning_parts_preserve_safe_handle_authority() {
    let (mut world, body, _, shape, joint) = scene();
    let (other_world, foreign_body, _, _, _) = scene();

    unsafe {
        let guard = raw::world_raw_guard(&mut world).unwrap();
        let raw_world = guard.world_id();
        let raw_body = guard.body_id(body).unwrap();
        let raw_shape = guard.shape_id(shape).unwrap();
        let raw_joint = guard.joint_id(joint).unwrap();
        assert_ne!(raw_world.index1, 0);
        assert_ne!(raw_body.index1, 0);
        assert_ne!(raw_shape.index1, 0);
        assert_ne!(raw_joint.index1, 0);
        assert_eq!(
            guard.body_id(foreign_body).unwrap_err(),
            Error::ForeignHandle {
                kind: HandleKind::Body,
            }
        );
    }

    let mut parts = world.into_raw_parts().unwrap();
    unsafe {
        let guard = parts.world_guard().unwrap();
        assert_ne!(guard.world_id().index1, 0);
        assert_ne!(guard.shape_id(shape).unwrap().index1, 0);
    }
    let world = unsafe { parts.into_world().unwrap() };
    assert_eq!(world.contains_body(body), Ok(true));
    assert_eq!(world.contains_shape(shape), Ok(true));
    assert_eq!(world.contains_joint(joint), Ok(true));
    drop(other_world);
}
