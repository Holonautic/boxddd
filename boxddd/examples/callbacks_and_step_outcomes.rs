use boxddd::{BodyType, BoxHull, Error, Foundation, Sphere, Vec3, World};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Default)]
struct CallbackCounts {
    custom_filter: AtomicUsize,
    pre_solve: AtomicUsize,
    friction: AtomicUsize,
    restitution: AtomicUsize,
}

fn main() -> boxddd::Result<()> {
    let foundation = Foundation::initialize_default()?;
    let mut world = foundation.create_world(
        foundation
            .world_def_builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()?,
    )?;

    let ground = world.create_body(
        foundation
            .body_def_builder()
            .position([0.0, -0.5, 0.0])
            .build()?,
    )?;
    let ground_shape = world.create_hull_shape(
        ground,
        &foundation
            .shape_def_builder()
            .friction(0.4)
            .user_material_id(11)
            .enable_custom_filtering(true)
            .build()?,
        &BoxHull::new(8.0, 0.5, 8.0)?,
    )?;

    let ball = world.create_body(
        foundation
            .body_def_builder()
            .body_type(BodyType::Dynamic)
            .position([0.0, 2.0, 0.0])
            .build()?,
    )?;
    let ball_shape = world.create_sphere_shape(
        ball,
        &foundation
            .shape_def_builder()
            .density(1.0)
            .friction(0.9)
            .restitution(0.1)
            .user_material_id(7)
            .enable_custom_filtering(true)
            .enable_pre_solve_events(true)
            .build()?,
        &Sphere::new(Vec3::ZERO, 0.5),
    )?;

    let counts = Arc::new(CallbackCounts::default());
    world.set_custom_filter({
        let counts = Arc::clone(&counts);
        move |shape_a, shape_b| {
            if is_pair(shape_a, shape_b, ground_shape, ball_shape) {
                counts.custom_filter.fetch_add(1, Ordering::Relaxed);
            }
            true
        }
    })?;
    world.set_pre_solve({
        let counts = Arc::clone(&counts);
        move |shape_a, shape_b, _point, _normal| {
            if is_pair(shape_a, shape_b, ground_shape, ball_shape) {
                counts.pre_solve.fetch_add(1, Ordering::Relaxed);
            }
            true
        }
    })?;
    world.set_friction_callback({
        let counts = Arc::clone(&counts);
        move |material_a, material_b| {
            counts.friction.fetch_add(1, Ordering::Relaxed);
            (material_a.coefficient * material_b.coefficient).sqrt()
        }
    })?;
    world.set_restitution_callback({
        let counts = Arc::clone(&counts);
        move |material_a, material_b| {
            counts.restitution.fetch_add(1, Ordering::Relaxed);
            material_a.coefficient.max(material_b.coefficient)
        }
    })?;

    for _ in 0..120 {
        advance_and_project(&mut world)?;
    }

    let observed = [
        counts.custom_filter.load(Ordering::Relaxed),
        counts.pre_solve.load(Ordering::Relaxed),
        counts.friction.load(Ordering::Relaxed),
        counts.restitution.load(Ordering::Relaxed),
    ];
    println!(
        "callbacks: filter={}, pre_solve={}, friction={}, restitution={}",
        observed[0], observed[1], observed[2], observed[3]
    );
    assert!(
        observed.into_iter().all(|count| count > 0),
        "expected every callback family to observe the contact"
    );

    world.clear_custom_filter()?;
    world.clear_pre_solve()?;
    world.clear_friction_callback()?;
    world.clear_restitution_callback()?;

    println!("injecting one intentional custom-filter panic; the panic-hook message is expected");
    world.set_custom_filter({
        let panic_injected = AtomicBool::new(false);
        move |_, _| {
            if !panic_injected.swap(true, Ordering::Relaxed) {
                panic!("intentional callback panic injected by the example");
            }
            true
        }
    })?;

    let probe = world.create_body(
        foundation
            .body_def_builder()
            .body_type(BodyType::Dynamic)
            .position([0.0, 2.0, 0.0])
            .build()?,
    )?;
    world.create_sphere_shape(
        probe,
        &foundation
            .shape_def_builder()
            .density(1.0)
            .enable_custom_filtering(true)
            .build()?,
        &Sphere::new(Vec3::ZERO, 0.5),
    )?;

    let mut contained_failure = None;
    for _ in 0..90 {
        let outcome = world.step_outcome(1.0 / 60.0, 4)?;
        if outcome.post_step_error().is_some() {
            contained_failure = Some(outcome);
            break;
        }
    }
    let contained_failure =
        contained_failure.expect("the probe should invoke the intentional callback panic");
    assert_eq!(
        contained_failure.post_step_error(),
        Some(&Error::CallbackPanicked)
    );
    println!(
        "intentional callback panic was contained after native advancement: {}",
        contained_failure
            .post_step_error()
            .expect("the failure was checked above")
    );
    assert_eq!(
        contained_failure.into_result(),
        Err(Error::CallbackPanicked)
    );

    world.clear_custom_filter()?;
    let recovered = world.step_outcome(1.0 / 60.0, 4)?;
    assert_eq!(recovered.post_step_error(), None);
    recovered.into_result()?;
    println!("callback cleared; the next native step completed without a post-step error");

    match world.step_outcome(-1.0, 4) {
        Err(error @ Error::InvalidValue { .. }) => {
            println!("step rejected before native advancement: {error}");
        }
        Err(error) => return Err(error),
        Ok(outcome) => panic!("negative time step unexpectedly advanced: {outcome:?}"),
    }

    Ok(())
}

fn advance_and_project(world: &mut World) -> boxddd::Result<()> {
    // The outer Result covers admission before Box3D advances. StepOutcome
    // retains callback or task failures found after the irreversible native step.
    let outcome = world.step_outcome(1.0 / 60.0, 4)?;
    if let Some(error) = outcome.post_step_error() {
        eprintln!("native simulation advanced before reporting: {error}");
    }
    outcome.into_result()
}

fn is_pair(
    shape_a: boxddd::ShapeId,
    shape_b: boxddd::ShapeId,
    expected_a: boxddd::ShapeId,
    expected_b: boxddd::ShapeId,
) -> bool {
    (shape_a == expected_a && shape_b == expected_b)
        || (shape_a == expected_b && shape_b == expected_a)
}
