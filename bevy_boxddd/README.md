# bevy_boxddd

`bevy_boxddd` integrates [`boxddd`](https://github.com/Latias94/boxddd) with Bevy 0.19. It keeps the core `boxddd` crate engine-agnostic while providing ECS authoring, fixed-step systems, messages, queries, debug drawing, and teaching examples for Bevy users.

## Quick Facts

| Item | Value |
|---|---|
| Bevy version | `0.19.0` |
| Rust version | `1.95.0` or newer |
| Physics owner | `NonSend<BoxdddPhysicsContext>` |
| Foundation | Explicit, process-wide configuration shared by every physics context |
| Schedule | `FixedUpdate` |
| Default rendering deps | None |
| Debug rendering | Optional `debug-gizmos` feature |
| Picking example | Optional `physics-picking` feature |
| Testbed UI | `bevy_egui` in the `testbed_3d` example only |
| Web examples | Direct Bevy + egui example pages published through `cargo run -p xtask -- build-pages-wasm` with provider ABI v2 |
| Threading | Keep `boxddd::World` on the Bevy main thread; move snapshots across threads |

## Quickstart

```rust
use bevy::prelude::*;
use bevy_boxddd::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BoxdddPhysicsPlugin::new(FoundationConfig::default()))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    let body = commands
        .spawn((
            RigidBody::Dynamic,
            Transform::from_xyz(0.0, 4.0, 0.0),
        ))
        .id();

    commands.spawn((Collider::sphere(0.35), ChildOf(body)));
    commands.spawn((Collider::cube(0.4), ChildOf(body)));
}
```

Child collider entities are plugin-owned shapes attached to the nearest parent entity with `BoxdddBody`. A body entity can also carry its own `Collider`.

The first successful Foundation initialization freezes the process-wide length
units and stall threshold. Plugins requesting the same normalized
`FoundationConfig` share that Foundation while owning independent Worlds. A
different configuration emits `BoxdddOperation::InitializeFoundation` with
`Error::FoundationConflict` and installs a disabled context; dropping every App
does not reset the process configuration.

## Components

- `RigidBody`: `Static`, `Kinematic`, or `Dynamic`.
- `BodySettings`: gravity scale, damping, sleeping, bullet CCD, and motion locks applied to the native body.
- `Collider`: basic `cuboid`, `cube`, `sphere`, and `capsule_y` descriptors plus advanced mesh, height-field, compound, created-hull, and transformed-hull descriptors.
- `PhysicsMaterial`: density, friction, restitution, sensor/contact flags, and Box3D filter data.
- `JointTarget` + `Joint`: declarative joint authoring between two Bevy body entities.
- `TransformSyncMode`: dynamic bodies default to physics-authored transforms; static and kinematic bodies default to Bevy-authored transforms.
- `LinearVelocity`, `AngularVelocity`, `ExternalForce`, `ExternalImpulse`: control inputs applied before stepping.
- `BoxdddBody`, `BoxdddShape`, `BoxdddJoint`: native id components inserted by the plugin after successful creation.

Mesh, height-field, and compound collider descriptors are static-body only. Attaching those descriptors to dynamic bodies emits `BoxdddErrorMessage { operation: CreateShape, error: InvalidValue { context: "collider.body_type", reason: InvalidCombination }, .. }`. Procedural hull descriptors, including rock and cylinder hulls, may be attached to dynamic bodies.

## Queries And Picking

Default library helpers are renderer-free:

```rust
let context = world.get_non_send::<BoxdddPhysicsContext>().unwrap();
let hit = cast_ray_closest(
    context,
    Vec3::new(-2.0, 0.0, 0.0),
    Vec3::new(5.0, 0.0, 0.0),
    boxddd::QueryFilter::default(),
)?;
```

Hits include both the Box3D `ShapeId` and the mapped Bevy entity when the shape is plugin-owned.

## Math Adapters

`bevy_boxddd::math` provides free conversion helpers; it no longer exports conversion extension traits, so Bevy systems do not need local conversion functions:

```rust
use bevy::prelude::*;
use bevy_boxddd::boxddd;
use bevy_boxddd::math::{to_bevy_pos, to_boxddd_pos, to_boxddd_quat, to_boxddd_vec3};

let origin = to_boxddd_pos(Vec3::new(-2.0, 1.0, 0.0));
let translation = to_boxddd_vec3(Vec3::X);
let rotation = to_boxddd_quat(Quat::IDENTITY)?;

let bevy_point = to_bevy_pos(boxddd::Pos::new(1.0, 2.0, 3.0));
```

Transform adapters intentionally ignore Bevy scale when converting into Box3D, and preserve Bevy scale when applying a Box3D transform back onto an existing Bevy `Transform`.

## Debug Draw

The plugin always exposes `BoxdddDebugDrawSettings` and `BoxdddDebugDrawFrame`. By default collection is disabled. Enable collection without rendering dependencies:

```rust
commands.insert_resource(BoxdddDebugDrawSettings {
    enabled: true,
    options: boxddd::DebugDrawOptions::default(),
});
```

Enable `debug-gizmos` to render collected commands through Bevy `Gizmos`:

```bash
cargo run -p bevy_boxddd --features debug-gizmos --example debug_draw_overlay_3d
```

## Messages And Errors

The plugin registers Bevy messages for errors, body moves, contact begin/end/hit, and sensor begin/end. Every recoverable plugin error emits `BoxdddErrorMessage`. `BoxdddPhysicsSettings.error_policy` selects message only or message plus log; there is no panic policy. Core operations used by the plugin are result-first, and their errors are preserved in the message's `error` field.

If Box3D advanced before a callback or task failed, the plugin publishes that
step's events and synchronizes transforms before emitting the error message. A
validation or admission failure before native stepping publishes neither. End
messages caused by destruction retain any surviving entity mapping; the
destroyed side may be `None` and is not reinserted into the ECS maps.

## Examples

The visible examples complement the core headless examples and follow the case-level official Box3D sample matrix in [`docs/upstream-parity/box3d-sample-matrix.md`](https://github.com/Latias94/boxddd/blob/main/docs/upstream-parity/box3d-sample-matrix.md). The complete catalog lives in [`examples/README.md`](examples/README.md).

| Example | Run command | Purpose |
|---|---|---|
| `falling_stack_3d` | `cargo run -p bevy_boxddd --example falling_stack_3d` | Basic windowed stack with plugin-driven transforms. |
| `contact_messages_3d` | `cargo run -p bevy_boxddd --example contact_messages_3d` | Reads contact messages and updates Bevy materials. |
| `debug_gizmos_3d` | `cargo run -p bevy_boxddd --example debug_gizmos_3d` | App-authored collider gizmos without Box3D debug draw collection. |
| `advanced_colliders_3d` | `cargo run -p bevy_boxddd --example advanced_colliders_3d` | Static mesh, height-field, compound, hull-backed colliders, and dynamic bodies falling onto them. |
| `joint_gallery_3d` | `cargo run -p bevy_boxddd --example joint_gallery_3d` | Visible connected bodies using every public declarative joint variant. |
| `debug_draw_overlay_3d` | `cargo run -p bevy_boxddd --features debug-gizmos --example debug_draw_overlay_3d` | Box3D debug draw commands rendered through Bevy `Gizmos`. |
| `physics_picking_3d` | `cargo run -p bevy_boxddd --features physics-picking --example physics_picking_3d` | Camera/cursor ray mapped through Box3D queries, not mesh picking. |
| `testbed_3d` | `cargo run -p bevy_boxddd --features "debug-gizmos physics-picking" --example testbed_3d` | Egui-driven scene browser for official Box3D teaching scenes plus Query Lab, Debug Draw Inspector, Material Lab, and Stats Dashboard showcases. |

The native testbed is the primary local teaching surface. Use the left panel to switch scenes and adjust pause, single-step, gravity, solver rate, substeps, sleeping, warm starting, continuous collision, and debug draw presets. The static demo hub at <https://frankorz.com/boxddd/> mirrors the same scene registry as direct Bevy + egui Web example pages, so a user can click straight into a scene without using the testbed scene switcher.

## Platform And Concurrency Boundaries

Native desktop targets are the main supported runtime target for the Bevy plugin. CI checks Windows, Linux, and macOS native builds through the workspace test suite, and checks the Bevy examples on Linux with the required windowing/audio packages installed.

`wasm32-unknown-unknown` has two tiers: the minimal library surface remains compile-checked with `--no-default-features`, and the `testbed_3d` wasm bundle is built for GitHub Pages through provider mode, `wasm-bindgen`, and the `box3d-sys-v2` Emscripten provider at bridge revision 2. Pages exposes that bundle as direct per-scene Bevy examples. Other windowed examples are native teaching examples today. Mobile targets are compile-only at the core `boxddd` layer and are not yet runtime targets for `bevy_boxddd`. Callback-heavy surfaces without a provider bridge return `Error::UnsupportedOnWasm`.

`boxddd::World` is intentionally non-send and lives in `BoxdddPhysicsContext`. Do not move it into Bevy worker systems. The core crate has `TaskSystem::blocking_threads()` for Box3D's native task callbacks, but Bevy task-pool integration is deferred until that contract has more usage.

## Development Checks

```bash
cargo fmt --all --check
cargo check -p bevy_boxddd --no-default-features
cargo nextest run -p bevy_boxddd
cargo check -p bevy_boxddd --examples
cargo check -p bevy_boxddd --features debug-gizmos --example debug_draw_overlay_3d
cargo check -p bevy_boxddd --features physics-picking --example physics_picking_3d
cargo check -p bevy_boxddd --features "debug-gizmos physics-picking" --example testbed_3d
cargo check -p bevy_boxddd --target wasm32-unknown-unknown --no-default-features
BOXDDD_SYS_WASM_MODE=provider RUSTFLAGS="-C link-arg=--import-memory" cargo check -p bevy_boxddd --features "debug-gizmos physics-picking" --example testbed_3d --target wasm32-unknown-unknown
```

Windowed teaching examples:

```bash
cargo run -p bevy_boxddd --example falling_stack_3d
cargo run -p bevy_boxddd --example advanced_colliders_3d
cargo run -p bevy_boxddd --example joint_gallery_3d
cargo run -p bevy_boxddd --features debug-gizmos --example debug_draw_overlay_3d
cargo run -p bevy_boxddd --features physics-picking --example physics_picking_3d
cargo run -p bevy_boxddd --features "debug-gizmos physics-picking" --example testbed_3d
```

On Windows, the teaching examples default to the DX12 backend because some NVIDIA/Vulkan driver and validation-layer combinations emit noisy swapchain validation errors even when the app continues to run. Set `WGPU_BACKEND=vulkan` before running an example if you want to test Vulkan explicitly.
