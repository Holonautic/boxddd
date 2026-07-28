use boxddd::{BodyDef, BodyType, BoxHull, Error, ShapeDef, Sphere, Vec3, World, WorldDef};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct CallbackCounts {
    custom_filter: AtomicUsize,
    pre_solve: AtomicUsize,
    friction: AtomicUsize,
    restitution: AtomicUsize,
}

fn main() -> boxddd::Result<()> {
    let mut world = World::new(
        WorldDef::builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()?,
    )?;

    let ground = world.create_body(BodyDef::builder().position([0.0, -0.5, 0.0]).build()?)?;
    let ground_shape = world.create_hull_shape(
        ground,
        &ShapeDef::builder()
            .friction(0.4)
            .user_material_id(11)
            .enable_custom_filtering(true)
            .build()?,
        &BoxHull::new(8.0, 0.5, 8.0)?,
    )?;

    let ball = world.create_body(
        BodyDef::builder()
            .body_type(BodyType::Dynamic)
            .position([0.0, 2.0, 0.0])
            .build()?,
    )?;
    let ball_shape = world.create_sphere_shape(
        ball,
        &ShapeDef::builder()
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
