use boxddd::{BodyType, DistanceJointDef, Sphere, Vec3};

fn main() -> boxddd::Result<()> {
    let foundation = boxddd::Foundation::initialize_default()?;
    let mut world =
        foundation.create_world(foundation.world_def_builder().gravity(Vec3::ZERO).build()?)?;
    let anchor = world.create_body(
        foundation
            .body_def_builder()
            .position([0.0, 0.0, 0.0])
            .build()?,
    )?;
    let body = world.create_body(
        foundation
            .body_def_builder()
            .body_type(BodyType::Dynamic)
            .position([1.0, 0.0, 0.0])
            .build()?,
    )?;
    world.create_sphere_shape(
        body,
        &foundation.shape_def_builder().density(1.0).build()?,
        &Sphere::new([0.0, 0.0, 0.0], 0.25),
    )?;

    let joint = world.create_distance_joint(DistanceJointDef::new(anchor, body).length(1.0))?;
    world.apply_force_to_center(body, [25.0, 0.0, 0.0], true)?;
    for _ in 0..60 {
        world.step(1.0 / 60.0, 4)?;
    }

    println!(
        "distance joint length after one second: {:.3}",
        world.distance_joint_current_length(joint)?
    );
    Ok(())
}
