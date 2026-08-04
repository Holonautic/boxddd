use boxddd::{BodyType, BoxHull, TaskSystem, Vec3};

fn main() -> boxddd::Result<()> {
    let task_system = TaskSystem::blocking_threads()?;
    let foundation = boxddd::Foundation::initialize_default()?;
    let mut world = foundation.create_world(
        foundation
            .world_def_builder()
            .gravity(Vec3::new(0.0, -10.0, 0.0))
            .worker_count(2)
            .task_system(task_system.clone())
            .build()?,
    )?;

    let ground = world.create_body(
        foundation
            .body_def_builder()
            .position([0.0, -1.0, 0.0])
            .build()?,
    )?;
    world.create_hull_shape(
        ground,
        &foundation.shape_def(),
        &BoxHull::new(30.0, 0.5, 30.0)?,
    )?;

    let shape_def = foundation
        .shape_def_builder()
        .density(1.0)
        .friction(0.4)
        .build()?;
    for x in 0..10 {
        for z in 0..10 {
            let body = world.create_body(
                foundation
                    .body_def_builder()
                    .body_type(BodyType::Dynamic)
                    .position([(x as f32 - 5.0) * 0.8, 3.0, (z as f32 - 5.0) * 0.8])
                    .build()?,
            )?;
            world.create_hull_shape(body, &shape_def, &BoxHull::cube(0.35)?)?;
        }
    }

    for _ in 0..30 {
        world.step(1.0 / 60.0, 4)?;
    }

    let stats = task_system.stats();
    let counters = world.counters()?;
    println!(
        "scheduler: enqueued={} started={} completed={} finished={} panicked={}",
        stats.enqueued, stats.started, stats.completed, stats.finished, stats.panicked
    );
    println!("box3d task_count={}", counters.task_count);

    Ok(())
}
