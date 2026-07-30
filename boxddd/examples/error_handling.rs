mod common;

use anyhow::{Context, Result};
use boxddd::error::HandleKind;
use boxddd::prelude::*;

fn main() -> Result<()> {
    let foundation = Foundation::initialize_default().context("failed to initialize Box3D")?;
    let mut scene = common::falling_stack_scene().context("failed to create the demo scene")?;

    match foundation.shape_def_builder().density(f32::NAN).build() {
        Err(Error::InvalidValue { .. }) => {
            println!("invalid shape definition was rejected during build");
        }
        Err(error) => return Err(error).context("unexpected builder error"),
        Ok(_) => anyhow::bail!("invalid shape definition unexpectedly built"),
    }

    demonstrate_owner_scoped_handles(foundation)?;

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

fn demonstrate_owner_scoped_handles(foundation: &'static Foundation) -> Result<()> {
    let mut owner = foundation
        .create_world(foundation.world_def())
        .context("failed to create the owner world")?;
    let other = foundation
        .create_world(foundation.world_def())
        .context("failed to create the other world")?;
    let body = owner
        .create_body(foundation.body_def())
        .context("failed to create the owner-scoped body")?;

    match other.body_position(body) {
        Err(Error::ForeignHandle {
            kind: HandleKind::Body,
        }) => {
            println!("a body ID is rejected as foreign when used with another world");
        }
        Err(error) => return Err(error).context("unexpected cross-world body error"),
        Ok(_) => anyhow::bail!("another world unexpectedly accepted a foreign body ID"),
    }

    owner
        .destroy_body(body)
        .context("failed to destroy the owner-scoped body")?;
    match owner.body_position(body) {
        Err(Error::StaleHandle {
            kind: HandleKind::Body,
        }) => {
            println!("the same body ID is rejected as stale after its owner destroys it");
        }
        Err(error) => return Err(error).context("unexpected destroyed-body error"),
        Ok(_) => anyhow::bail!("the owner unexpectedly accepted a destroyed body ID"),
    }

    Ok(())
}
