use boxddd::{BodyType, Foundation, Recording, Sphere};

fn main() -> boxddd::Result<()> {
    let foundation = Foundation::initialize_default()?;
    let mut world = foundation.create_world(
        foundation
            .world_def_builder()
            .gravity([0.0, -9.8, 0.0])
            .build()?,
    )?;
    let mut recording = Recording::new()?;
    let mut session = world.record(&mut recording)?;
    let body = session.world().create_body(
        foundation
            .body_def_builder()
            .body_type(BodyType::Dynamic)
            .position([0.0, 2.0, 0.0])
            .build()?,
    )?;
    session.world().create_sphere_shape(
        body,
        &foundation.shape_def_builder().density(1.0).build()?,
        &Sphere::new([0.0, 0.0, 0.0], 0.25),
    )?;
    for _ in 0..30 {
        session.world().step(1.0 / 60.0, 4)?;
    }
    session.finish()?;
    drop(world);

    let ok = foundation.validate_replay(recording.bytes()?, 1)?;
    println!("serial replay validation: {ok}");
    Ok(())
}
