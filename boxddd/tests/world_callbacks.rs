use boxddd::{BodyDef, BodyType, BoxHull, Error, Quat, ShapeDef, Sphere, Vec3, World, WorldDef};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct DropProbe(Arc<AtomicUsize>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn contact_world() -> (World, boxddd::ShapeId, boxddd::ShapeId) {
    let mut world = World::new(
        WorldDef::builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()
            .unwrap(),
    )
    .unwrap();
    let ground = world
        .create_body(
            BodyDef::builder()
                .position([0.0, -0.5, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let ground_shape = world
        .create_hull_shape(
            ground,
            &ShapeDef::builder()
                .enable_custom_filtering(true)
                .build()
                .unwrap(),
            &BoxHull::new(10.0, 0.5, 10.0).unwrap(),
        )
        .unwrap();
    let sphere = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    let sphere_shape = world
        .create_sphere_shape(
            sphere,
            &ShapeDef::builder()
                .density(1.0)
                .enable_contact_events(true)
                .enable_pre_solve_events(true)
                .enable_custom_filtering(true)
                .build()
                .unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();
    (world, ground_shape, sphere_shape)
}

#[test]
fn custom_filter_can_disable_contacts() {
    let (mut world, ground_shape, sphere_shape) = contact_world();
    let calls = Arc::new(AtomicUsize::new(0));
    let saw_expected_pair = Arc::new(AtomicBool::new(false));

    world
        .set_custom_filter({
            let calls = Arc::clone(&calls);
            let saw_expected_pair = Arc::clone(&saw_expected_pair);
            move |a, b| {
                calls.fetch_add(1, Ordering::Relaxed);
                if [a, b].contains(&ground_shape) && [a, b].contains(&sphere_shape) {
                    saw_expected_pair.store(true, Ordering::Relaxed);
                }
                false
            }
        })
        .unwrap();

    for _ in 0..90 {
        world.step(1.0 / 60.0, 4).unwrap();
    }

    assert!(calls.load(Ordering::Relaxed) > 0);
    assert!(saw_expected_pair.load(Ordering::Relaxed));
    assert!(world.contact_events().unwrap().begin.is_empty());

    world.clear_custom_filter().unwrap();
}

#[test]
fn pre_solve_callback_is_invoked_and_can_disable_contacts_for_step() {
    let (mut world, ground_shape, sphere_shape) = contact_world();
    let calls = Arc::new(AtomicUsize::new(0));
    let saw_expected_pair = Arc::new(AtomicBool::new(false));

    world
        .set_pre_solve({
            let calls = Arc::clone(&calls);
            let saw_expected_pair = Arc::clone(&saw_expected_pair);
            move |a, b, point, normal| {
                calls.fetch_add(1, Ordering::Relaxed);
                if [a, b].contains(&ground_shape) && [a, b].contains(&sphere_shape) {
                    saw_expected_pair.store(true, Ordering::Relaxed);
                }
                assert!(point.validate().is_ok());
                assert!(normal.is_valid());
                false
            }
        })
        .unwrap();

    for _ in 0..90 {
        world.step(1.0 / 60.0, 4).unwrap();
    }

    assert!(calls.load(Ordering::Relaxed) > 0);
    assert!(saw_expected_pair.load(Ordering::Relaxed));
    world.clear_pre_solve().unwrap();
}

#[test]
fn clearing_pre_solve_keeps_the_ccd_fallback_installed() {
    let mut world = World::new(WorldDef::builder().gravity(Vec3::ZERO).build().unwrap()).unwrap();
    let wall = world.create_body(BodyDef::default()).unwrap();
    world
        .create_hull_shape(
            wall,
            &ShapeDef::default(),
            &BoxHull::new(0.05, 4.0, 4.0).unwrap(),
        )
        .unwrap();
    let bullet = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([-5.0, 0.0, 0.0])
                .linear_velocity([600.0, 0.0, 0.0])
                .gravity_scale(0.0)
                .bullet(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            bullet,
            &ShapeDef::builder()
                .density(1.0)
                .enable_pre_solve_events(true)
                .build()
                .unwrap(),
            &Sphere::new(Vec3::ZERO, 0.2),
        )
        .unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    world
        .set_pre_solve({
            let calls = Arc::clone(&calls);
            move |_, _, _, _| {
                calls.fetch_add(1, Ordering::Relaxed);
                true
            }
        })
        .unwrap();
    world.step(1.0 / 60.0, 4).unwrap();
    assert!(calls.load(Ordering::Relaxed) > 0);

    world.clear_pre_solve().unwrap();
    world
        .set_body_transform(bullet, [-5.0, 0.0, 0.0], Quat::IDENTITY)
        .unwrap();
    world
        .set_body_linear_velocity(bullet, [600.0, 0.0, 0.0])
        .unwrap();
    world.step(1.0 / 60.0, 4).unwrap();
}

#[test]
fn callback_panic_is_caught_and_reported_after_step() {
    let (mut world, _, _) = contact_world();
    world
        .set_custom_filter(|_, _| panic!("custom filter panic"))
        .unwrap();

    let mut result = Ok(());
    for _ in 0..90 {
        result = world.step(1.0 / 60.0, 4);
        if result == Err(Error::CallbackPanicked) {
            break;
        }
    }

    assert_eq!(result, Err(Error::CallbackPanicked));
}

#[test]
fn material_mix_callbacks_receive_user_material_ids() {
    let mut world = World::new(
        WorldDef::builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()
            .unwrap(),
    )
    .unwrap();
    let friction_calls = Arc::new(AtomicUsize::new(0));
    let restitution_calls = Arc::new(AtomicUsize::new(0));
    let saw_materials = Arc::new(AtomicBool::new(false));

    world
        .set_friction_callback({
            let friction_calls = Arc::clone(&friction_calls);
            let saw_materials = Arc::clone(&saw_materials);
            move |a, b| {
                friction_calls.fetch_add(1, Ordering::Relaxed);
                if [a.user_material_id, b.user_material_id].contains(&7)
                    && [a.user_material_id, b.user_material_id].contains(&11)
                {
                    saw_materials.store(true, Ordering::Relaxed);
                }
                0.25
            }
        })
        .unwrap();
    world
        .set_restitution_callback({
            let restitution_calls = Arc::clone(&restitution_calls);
            move |a, b| {
                restitution_calls.fetch_add(1, Ordering::Relaxed);
                a.coefficient.max(b.coefficient)
            }
        })
        .unwrap();

    let ground = world
        .create_body(
            BodyDef::builder()
                .position([0.0, -0.5, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_hull_shape(
            ground,
            &ShapeDef::builder()
                .friction(0.4)
                .user_material_id(11)
                .build()
                .unwrap(),
            &BoxHull::new(10.0, 0.5, 10.0).unwrap(),
        )
        .unwrap();
    let sphere = world
        .create_body(
            BodyDef::builder()
                .body_type(BodyType::Dynamic)
                .position([0.0, 2.0, 0.0])
                .build()
                .unwrap(),
        )
        .unwrap();
    world
        .create_sphere_shape(
            sphere,
            &ShapeDef::builder()
                .density(1.0)
                .friction(0.9)
                .restitution(0.1)
                .user_material_id(7)
                .build()
                .unwrap(),
            &Sphere::new([0.0, 0.0, 0.0], 0.5),
        )
        .unwrap();

    for _ in 0..90 {
        world.step(1.0 / 60.0, 4).unwrap();
    }

    assert!(friction_calls.load(Ordering::Relaxed) > 0);
    assert!(restitution_calls.load(Ordering::Relaxed) > 0);
    assert!(saw_materials.load(Ordering::Relaxed));

    world.clear_friction_callback().unwrap();
    world.clear_restitution_callback().unwrap();
}

#[test]
fn material_mix_callback_panic_is_reported_after_step() {
    let (mut world, _, _) = contact_world();
    world
        .set_friction_callback(|_, _| panic!("friction mix panic"))
        .unwrap();

    let mut result = Ok(());
    for _ in 0..90 {
        result = world.step(1.0 / 60.0, 4);
        if result == Err(Error::CallbackPanicked) {
            break;
        }
    }

    assert_eq!(result, Err(Error::CallbackPanicked));
    world.clear_friction_callback().unwrap();
}

#[test]
fn non_finite_material_mix_returns_fallback_without_panic() {
    let (mut world, _, _) = contact_world();
    let friction_calls = Arc::new(AtomicUsize::new(0));
    let restitution_calls = Arc::new(AtomicUsize::new(0));

    world
        .set_friction_callback({
            let friction_calls = Arc::clone(&friction_calls);
            move |_, _| {
                friction_calls.fetch_add(1, Ordering::Relaxed);
                f32::NAN
            }
        })
        .unwrap();
    world
        .set_restitution_callback({
            let restitution_calls = Arc::clone(&restitution_calls);
            move |_, _| {
                restitution_calls.fetch_add(1, Ordering::Relaxed);
                f32::INFINITY
            }
        })
        .unwrap();

    for _ in 0..90 {
        world.step(1.0 / 60.0, 4).unwrap();
    }

    assert!(friction_calls.load(Ordering::Relaxed) > 0);
    assert!(restitution_calls.load(Ordering::Relaxed) > 0);
    world.clear_friction_callback().unwrap();
    world.clear_restitution_callback().unwrap();
}

#[test]
fn callback_registration_respects_callback_guard() {
    let mut world = World::new(WorldDef::default()).unwrap();
    let _guard = boxddd::__private::enter_callback_guard_for_test();

    assert_eq!(
        world.set_custom_filter(|_, _| true).unwrap_err(),
        Error::InCallback
    );
    assert_eq!(
        world.set_pre_solve(|_, _, _, _| true).unwrap_err(),
        Error::InCallback
    );
    assert_eq!(
        world
            .set_friction_callback(|a, _| a.coefficient)
            .unwrap_err(),
        Error::InCallback
    );
    assert_eq!(
        world
            .set_restitution_callback(|a, _| a.coefficient)
            .unwrap_err(),
        Error::InCallback
    );
}

#[test]
fn replacing_and_clearing_callbacks_retires_closures_immediately() {
    let mut world = World::new(WorldDef::default()).unwrap();
    let first_drops = Arc::new(AtomicUsize::new(0));
    let second_drops = Arc::new(AtomicUsize::new(0));

    world
        .set_custom_filter({
            let probe = DropProbe(Arc::clone(&first_drops));
            move |_, _| {
                let _ = &probe;
                true
            }
        })
        .unwrap();
    assert_eq!(first_drops.load(Ordering::Relaxed), 0);

    world
        .set_custom_filter({
            let probe = DropProbe(Arc::clone(&second_drops));
            move |_, _| {
                let _ = &probe;
                true
            }
        })
        .unwrap();
    assert_eq!(first_drops.load(Ordering::Relaxed), 1);
    assert_eq!(second_drops.load(Ordering::Relaxed), 0);

    world.clear_custom_filter().unwrap();
    assert_eq!(second_drops.load(Ordering::Relaxed), 1);
}

#[test]
fn world_drop_retires_every_registered_callback() {
    let custom_drops = Arc::new(AtomicUsize::new(0));
    let pre_solve_drops = Arc::new(AtomicUsize::new(0));
    let friction_drops = Arc::new(AtomicUsize::new(0));
    let restitution_drops = Arc::new(AtomicUsize::new(0));

    {
        let mut world = World::new(WorldDef::default()).unwrap();
        world
            .set_custom_filter({
                let probe = DropProbe(Arc::clone(&custom_drops));
                move |_, _| {
                    let _ = &probe;
                    true
                }
            })
            .unwrap();
        world
            .set_pre_solve({
                let probe = DropProbe(Arc::clone(&pre_solve_drops));
                move |_, _, _, _| {
                    let _ = &probe;
                    true
                }
            })
            .unwrap();
        world
            .set_friction_callback({
                let probe = DropProbe(Arc::clone(&friction_drops));
                move |a, _| {
                    let _ = &probe;
                    a.coefficient
                }
            })
            .unwrap();
        world
            .set_restitution_callback({
                let probe = DropProbe(Arc::clone(&restitution_drops));
                move |a, _| {
                    let _ = &probe;
                    a.coefficient
                }
            })
            .unwrap();
    }

    assert_eq!(custom_drops.load(Ordering::Relaxed), 1);
    assert_eq!(pre_solve_drops.load(Ordering::Relaxed), 1);
    assert_eq!(friction_drops.load(Ordering::Relaxed), 1);
    assert_eq!(restitution_drops.load(Ordering::Relaxed), 1);
}

#[test]
fn cleared_material_callbacks_release_their_registry_slot() {
    let mut world = World::new(WorldDef::default()).unwrap();

    for _ in 0..64 {
        world.set_friction_callback(|a, _| a.coefficient).unwrap();
        world.clear_friction_callback().unwrap();
    }
}
