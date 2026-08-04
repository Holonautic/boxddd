use boxddd::prelude::*;
use mint::{Point3, Quaternion, Vector3};

fn main() -> boxddd::Result<()> {
    let foundation = boxddd::Foundation::initialize_default()?;
    let gravity = Vector3 {
        x: 0.0,
        y: -9.8,
        z: 0.0,
    };
    let mut world =
        foundation.create_world(foundation.world_def_builder().gravity(gravity).build()?)?;

    let position = Point3 {
        x: 0.0,
        y: 2.0,
        z: 0.0,
    };
    let body = world.create_body(
        foundation
            .body_def_builder()
            .body_type(BodyType::Dynamic)
            .position(position)
            .build()?,
    )?;
    world.create_sphere_shape(
        body,
        &foundation.shape_def_builder().density(1.0).build()?,
        &Sphere::new([0.0, 0.0, 0.0], 0.25),
    )?;

    world.step(1.0 / 60.0, 4)?;

    let body_position: Point3<boxddd::types::PosScalar> = world.body_position(body)?.into();
    let rotation: Quaternion<f32> = world.body_rotation(body)?.into();
    let recovered = Quat::try_from(rotation)?;

    println!(
        "mint_interop: y={:.3}, quaternion_s={:.3}",
        body_position.y, recovered.s
    );
    Ok(())
}
