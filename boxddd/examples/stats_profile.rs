use boxddd::prelude::*;

const TIME_STEP: f32 = 1.0 / 60.0;
const SUB_STEPS: i32 = 4;

fn main() -> boxddd::Result<()> {
    let foundation = Foundation::initialize_default()?;
    let mut world = foundation.create_world(
        foundation
            .world_def_builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .build()?,
    )?;

    spawn_scene(foundation, &mut world)?;

    for frame in 1..=120 {
        world.step(TIME_STEP, SUB_STEPS)?;

        if frame % 30 == 0 {
            print_snapshot(&world, frame)?;
        }
    }

    let capacity = world.max_capacity()?;
    println!(
        "max capacity: static_bodies={} dynamic_bodies={} contacts={}",
        capacity.static_body_count, capacity.dynamic_body_count, capacity.contact_count
    );

    Ok(())
}

fn spawn_scene(foundation: &Foundation, world: &mut World) -> boxddd::Result<()> {
    let ground = world.create_body(
        foundation
            .body_def_builder()
            .position([0.0, -0.5, 0.0])
            .build()?,
    )?;
    world.create_hull_shape(
        ground,
        &foundation.shape_def(),
        &BoxHull::new(10.0, 0.5, 10.0)?,
    )?;

    let shape_def = foundation
        .shape_def_builder()
        .density(1.0)
        .friction(0.55)
        .build()?;
    let cube = BoxHull::cube(0.35)?;
    for layer in 0..6 {
        for column in 0..6 {
            let x = -2.1 + column as f32 * 0.72 + layer as f32 * 0.02;
            let y = 0.45 + layer as f32 * 0.72;
            let z = -0.8 + (column % 3) as f32 * 0.55;
            let body = world.create_body(
                foundation
                    .body_def_builder()
                    .body_type(BodyType::Dynamic)
                    .position([x, y, z])
                    .build()?,
            )?;
            world.create_hull_shape(body, &shape_def, &cube)?;
        }
    }
    Ok(())
}

fn print_snapshot(world: &World, frame: u32) -> boxddd::Result<()> {
    let counters = world.counters()?;
    let profile = world.profile()?;
    let awake_bodies = world.awake_body_count()?;
    let sleeping_enabled = world.sleeping_enabled()?;
    let worker_count = world.worker_count()?;

    println!(
        concat!(
            "frame={:03} bodies={} awake={} shapes={} contacts={} islands={} ",
            "tree_height={} tasks={} sleeping={} workers={}"
        ),
        frame,
        counters.body_count,
        awake_bodies,
        counters.shape_count,
        counters.contact_count,
        counters.island_count,
        counters.tree_height,
        counters.task_count,
        sleeping_enabled,
        worker_count,
    );
    println!(
        "  profile: step={:.3} collide={:.3} solve={:.3} pairs={:.3} transforms={:.3}",
        profile.step, profile.collide, profile.solve, profile.pairs, profile.transforms
    );

    Ok(())
}
