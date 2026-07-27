mod common;

use anyhow::{Context, Result};
use boxddd::prelude::*;

fn main() -> Result<()> {
    let mut scene = common::falling_stack_scene().context("failed to create the demo scene")?;

    match ShapeDef::builder().density(f32::NAN).build() {
        Err(Error::InvalidValue { .. }) => {
            println!("invalid shape definition was rejected during build");
        }
        Err(error) => return Err(error).context("unexpected builder error"),
        Ok(_) => anyhow::bail!("invalid shape definition unexpectedly built"),
    }

    for _ in 0..90 {
        scene
            .step(1.0 / 60.0, 4)
            .context("world step should remain recoverable")?;
    }

    for snapshot in scene.snapshots()? {
        println!(
            "{:<6} position = ({:>6.2}, {:>6.2}, {:>6.2})",
            snapshot.label, snapshot.position.x, snapshot.position.y, snapshot.position.z
        );
    }

    Ok(())
}
