# Migrating boxddd 0.3 to 0.4

`0.4.0` is an intentionally breaking release. It replaces ambient Box3D process
state with an explicit process-lifetime `Foundation`, makes replay exclusive,
and makes native creation failures transactional and fail-closed. There are no
compatibility shims for the removed `0.3` APIs.

The shortest migration is:

1. Initialize `Foundation` before any safe operation that can enter Box3D.
2. Create scale-aware definitions and Worlds from that Foundation.
3. Create or validate replay through the Foundation and keep replay separate
   from ordinary native owners.
4. Pass `FoundationConfig` to the Bevy plugin.
5. Handle the new Foundation, callback, identity, and poison errors.

## Initialize Foundation First

### Before (0.3)

```rust
use boxddd::{World, WorldDef};

let world = World::new(WorldDef::builder().build()?)?;
```

Applications could also change process globals at runtime:

```rust
boxddd::raw::set_length_units_per_meter(100.0)?;
boxddd::raw::set_stall_threshold(0.25)?;
```

### After (0.4)

```rust
use boxddd::{Foundation, FoundationConfig, StallThreshold};

let foundation = Foundation::initialize(FoundationConfig {
    length_units_per_meter: 100.0,
    stall_threshold: StallThreshold::Seconds(0.25),
})?;
let world = foundation.create_world(foundation.world_def())?;
```

Use `Foundation::initialize_default()` for one unit per meter with stall
reporting disabled. Initialization is explicit and must happen before safe
native work. The first successful normalized configuration is frozen until
process exit. Repeating the same configuration returns the same Foundation;
requesting different settings returns `Error::FoundationConflict` without
changing native state.

`Foundation::get()` only reads the published Foundation. It returns
`Error::FoundationUninitialized` instead of selecting defaults implicitly.

## Create Definitions From Foundation

### Before (0.3)

```rust
let world_def = boxddd::WorldDef::builder().gravity([0.0, -10.0, 0.0]).build()?;
let body_def = boxddd::BodyDef::builder().build()?;
let shape_def = boxddd::ShapeDef::default();
```

`WorldDef`, `BodyDef`, `ShapeDef`, and their builders implemented `Default` or
provided static constructors even though some defaults depend on process scale.

### After (0.4)

```rust
let world_def = foundation
    .world_def_builder()
    .gravity([0.0, -10.0, 0.0])
    .build()?;
let body_def = foundation.body_def();
let shape_def = foundation
    .shape_def_builder()
    .density(1.0)
    .friction(0.3)
    .build()?;
```

Use these Foundation factories:

| Value | Factory | Builder |
|---|---|---|
| `WorldDef` | `foundation.world_def()` | `foundation.world_def_builder()` |
| `BodyDef` | `foundation.body_def()` | `foundation.body_def_builder()` |
| `ShapeDef` | `foundation.shape_def()` | `foundation.shape_def_builder()` |

Scale-independent defaults such as `SurfaceMaterial::default()` and
`QueryFilter::default()` remain available.

## Create Worlds From Foundation

### Before (0.3)

```rust
let world = boxddd::World::new(world_def)?;
```

### After (0.4)

```rust
let world = foundation.create_world(world_def)?;
```

Every World retains ordinary Foundation activity through native destruction.
Other configuration-dependent native owners do the same internally. A
`&'static Foundation` is only a configuration handle; keeping the handle does
not itself block replay.

Independent Worlds may step concurrently on separate owner threads. World
creation and destruction remain narrowly serialized because Box3D uses a
process-wide World slot table. `World` itself remains `!Send + !Sync`.

## Migrate Recording And Replay

### Before (0.3)

```rust
let valid = recording.validate_replay(1)?;
let player = recording.create_player(1)?;
let player_from_bytes = boxddd::RecPlayer::from_bytes(&bytes, 1)?;
```

The `0.3` contract allowed differently scaled replay players, ordinary Worlds,
and runtime scale changes to coexist by applying a temporary scale override
around replay calls.

### After (0.4)

```rust
let bytes = recording.to_vec()?;
let valid = foundation.validate_replay(&bytes, 1)?;
let mut player = foundation.create_replay_player(&bytes, 1)?;

while player.step_frame()? {}
player.close()?;
```

Replay now owns exclusive Foundation activity from before its first native call
through native destruction and verified restoration of the configured scale.
While an ordinary owner or transient native call exists, replay creation and
validation return `Error::FoundationBusy`. While replay exists, ordinary owner
creation, worldless native helpers, and a second player return the same error.

Prefer `RecPlayer::close()` when teardown errors must be reported. `Drop`
performs the same native teardown but cannot return an error; an unverifiable
restoration poisons the Foundation so the next safe admission fails closed.

## Migrate Bevy Initialization

### Before (0.3)

```rust
App::new().add_plugins(BoxdddPhysicsPlugin::new(
    BoxdddPhysicsSettings::default(),
));
```

`BoxdddPhysicsPlugin::default()` also selected all process defaults implicitly,
and `BoxdddPhysicsContext::from_world` could bypass plugin-owned initialization.

### After (0.4)

```rust
App::new().add_plugins(BoxdddPhysicsPlugin::new(FoundationConfig::default()));
```

The plugin initializes Foundation before it creates its World. Apps using the
same normalized `FoundationConfig` share the process Foundation and own
independent Worlds. A conflicting later plugin emits a
`BoxdddErrorMessage` with `BoxdddOperation::InitializeFoundation` and
`Error::FoundationConflict`, then installs a disabled context. Dropping every
Bevy App does not reset the process configuration.

`BoxdddPhysicsPlugin::default()` has been removed. Every plugin instance must
call `BoxdddPhysicsPlugin::new` and choose its process configuration explicitly.
`new` uses `BoxdddPhysicsSettings::default()` for per-App settings; use
`with_settings` when an App needs custom gravity, timing, or error policy.

`BoxdddPhysicsContext::new` now also takes the Foundation explicitly, and
`BoxdddPhysicsContext::from_world` has been removed. `PhysicsMaterial::shape_def`
requires a Foundation reference so its density default uses the same frozen
scale as the World.

## Handle The New Failure Boundaries

`0.4` adds explicit errors for process and native-identity state:

| Error | Meaning |
|---|---|
| `FoundationUninitialized` | Safe native work was requested before explicit initialization. |
| `FoundationConflict` | A different process configuration was requested after initialization. |
| `FoundationBusy` | Ordinary activity and exclusive replay would overlap. |
| `FoundationPoisoned` | Process state can no longer be proven consistent after a partial write or replay teardown failure. |
| `FoundationActivityExhausted` | An activity counter cannot represent another lease. |
| `ObjectIdentityExhausted` | Native creation returned no usable new identity without mutating the Rust ledger. |
| `OwnerPoisoned` | One World or standalone owner can no longer prove Rust/native correspondence. |
| `InCallback` | A fallible safe API was called reentrantly from Box3D callback execution. |

Callback rejection has priority: safe reentry returns `InCallback` before
Foundation lookup, activity admission, locking, ledger mutation, or FFI.
Infallible owner destruction inside a callback is deferred to the outermost
owner call frame and runs exactly once after Box3D returns.

The following existing queries now return `Result` because they must enforce
Foundation or owner health before answering:

```rust
let runtime_version = boxddd::version()?;
let bounded = aabb.is_bounded()?;
let sane = aabb.is_sane()?;
let has_body = world.contains_body(body_id)?;
let has_shape = world.contains_shape(shape_id)?;
let has_joint = world.contains_joint(joint_id)?;
let has_contact = world.contains_contact(contact_id)?;
let has_proxy = tree.contains_proxy(proxy_id)?;
```

Membership queries return `Ok(false)` only for stale or foreign IDs. Callback
reentry and terminal owner poison remain visible as `Err(InCallback)` and
`Err(OwnerPoisoned)` rather than being collapsed into absence.

## Understand Creation Transactions

Body, shape, joint, dynamic-tree proxy, and World bootstrap creation now share a
strict transaction. Rust bookkeeping is reserved before FFI. A newly returned
native identity is classified before publication, bound to its World and parent,
postflight-checked, and then published atomically.

If a proven-owned unpublished object fails later validation, `boxddd`
compensates by destroying it before retained backing is released. If native
output aliases an active or foreign object, or compensation cannot be verified,
the affected owner is poisoned instead of destroying an identity that may
belong to existing state. Applications should treat `OwnerPoisoned` and
`FoundationPoisoned` as terminal for new safe work on that owner or process.

## Replace cgmath Interop

The `cgmath` feature and every `cgmath` conversion implementation were removed.
`cgmath 0.18` is unmaintained and has an unsoundness advisory, so `boxddd` no
longer carries it as an optional dependency.

### Before (0.3)

```toml
[dependencies]
boxddd = { version = "0.3", features = ["cgmath"] }
```

```rust
let value: cgmath::Vector3<f32> = boxddd::Vec3::new(1.0, 2.0, 3.0).into();
```

### After (0.4)

Use `mint` for math-crate-neutral value types, or select the maintained direct
interop feature that matches the application:

```toml
[dependencies]
boxddd = { version = "0.4", features = ["glam"] }
glam = "0.33"
```

```rust
let value: glam::Vec3 = boxddd::Vec3::new(1.0, 2.0, 3.0).into();
```

`nalgebra` is also supported directly. If an application must retain `cgmath`,
perform the conversion in its own integration boundary rather than enabling a
`boxddd` feature.

## Raw And Sys Interop

`boxddd_sys::ffi` remains available for explicitly unsafe integration. Direct
FFI bypasses Foundation initialization and activity, callback rejection,
World-slot ordering, definition lifetime carriers, creation transactions,
provenance ledgers, compensation, and poison containment. Do not mutate native
objects behind a live safe owner unless the application assumes every one of
those invariants itself.

The safe runtime configuration setters were removed. Reading the normalized
safe configuration uses `foundation.config()`; there is no supported safe path
to change it after initialization.

## Migration Checklist

- Initialize one Foundation before any safe Box3D entry.
- Replace static scale-aware defaults and builders with Foundation factories.
- Replace direct World construction with `foundation.create_world`.
- Move replay validation and creation to Foundation and sequence it apart from
  ordinary native owners.
- Pass `FoundationConfig` to `BoxdddPhysicsPlugin::new`.
- Propagate the newly fallible version, AABB, World membership, and DynamicTree
  membership queries.
- Handle Foundation busy, conflict, uninitialized, poison, and owner poison
  errors at application boundaries.
- Replace `cgmath` interop with `mint`, `glam`, or `nalgebra`.
- Keep raw/sys calls isolated from live safe ownership.
- Run core, Bevy, provider/WASI, semver, rustdoc, and package checks before
  shipping the migrated application.
