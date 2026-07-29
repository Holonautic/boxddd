# Migrating from 0.2 to 0.3

`0.3` is a breaking release. It removes compatibility shims in favor of a
single safe ownership model, fallible operations, and explicit platform
boundaries. Update an application as a coordinated migration rather than
mixing `0.2` and `0.3` idioms.

## 1. Rebuild native and provider integrations

The bundled Box3D source is pinned to upstream commit
`781673be801569a94be7942848755fc0baad3653`. Generated bindings and the WASM
provider contract changed with that source. The provider module is now
`box3d-sys-v2` with bridge revision `2`.

Rebuild every native artifact and WASM provider artifact with `0.3`. Do not
link a `0.3` Rust crate against a cached `0.2` provider, generated binding, or
vendored Box3D library. Custom provider implementations must expose the
capabilities required by the current provider contract, including debug draw,
raw-symbol access, AABB overlap, and ray casting.

Provider mode intentionally does not implement every callback surface. In
particular, standalone geometry visitors, dynamic-tree visitors, task-system
callbacks, contact/material callbacks, replay debug drawing, mover collision,
shape cast, and shape overlap visitors can return `Error::UnsupportedOnWasm`.
Treat that error as a feature boundary, not as an empty query result.

## 2. Use owned definitions and builders

Definitions passed to Box3D are now ordinary owned Rust values. Build and
validate them before mutating a world. Do not retain pointers to a native
definition or rely on a C struct remaining borrowed after a creation call.

```rust
// 0.2: fallible operation used a try_ prefix.
let body_id = world.try_create_body(body_def)?;

// 0.3: create owned definitions with builders, then use the result-first name.
let body_def = boxddd::BodyDef::builder()
    .body_type(boxddd::BodyType::Dynamic)
    .position(boxddd::Pos::new(0.0, 2.0, 0.0))
    .build()?;
let body_id = world.create_body(body_def)?;
```

The same approach applies to `WorldDef`, `ShapeDef`, joint definitions,
geometry builders, task-system configuration, and other values passed into
native code. A builder's `build()` returns `Result`; propagate it rather than
assuming an invalid value will be ignored by Box3D.

`BoxHull::cube`, `BoxHull::new`, `BoxHull::offset`,
`BoxHull::transformed`, and `BoxHull::scaled` now return `Result` so invalid
geometry cannot reach Box3D assertions. Propagate the result before borrowing
the hull for shape creation:

```rust
let hull = boxddd::BoxHull::cube(0.5)?;
world.create_hull_shape(body_id, &shape_def, &hull)?;
```

## 3. Treat IDs as opaque, owner-scoped handles

`BodyId`, `ShapeId`, `JointId`, `ContactId`, and replay-world IDs are no longer
interchangeable native integer/struct IDs. They carry provenance for the owner
that created them. A handle from another `World` returns `Error::ForeignHandle`;
a handle that was destroyed or invalidated returns `Error::StaleHandle`.

Do not serialize, construct, compare by raw fields, or pass a handle to another
world as a way to share native ownership. Keep the owning `World` alongside
the safe ID and recreate handles after loading or replaying state.

Destruction events preserve identity without restoring authority. An ID copied
from the currently readable body/contact/joint window, or from the next
destruction-generated contact/sensor end window, still identifies the resource
that produced the event. It is nevertheless stale for world operations. Read
the event as data, do not use it to mutate the destroyed resource. Repeated
event getters are stable within a window; retired provenance expires when the
following completed step rotates that window.

Raw interop is explicit and unsafe. Use a scoped `WorldRawGuard` for
observational/non-structural native calls; it validates safe handles, holds the
Box3D lock, and keeps Rust sidecars alive for the guard lifetime.

```rust
// 0.2: application code commonly retained a raw native body ID.
// let raw_body: boxddd_sys::ffi::b3BodyId = ...;

// 0.3: resolve a live safe handle only for the duration of raw access.
let mut raw = unsafe { boxddd::raw::world_raw_guard(&mut world)? };
let raw_body = raw.body_id(body_id)?;
// Call only native operations whose safety contract you can uphold here.
```

For structural native mutation, consume the safe owner with
`World::into_raw_parts()`. `WorldRawParts` owns the world and every Rust
sidecar required to destroy it correctly. Returning to safe APIs requires the
unsafe `WorldRawParts::into_world()` contract: no unreconciled structural
mutation, no outstanding callbacks/tasks, and no later use of copied raw IDs.

## 4. Make world and recording lifetimes explicit

`World` owns its native world, native resource shapes, callback state, and
task/debug sidecars. It is intentionally `!Send` and `!Sync`. Drop it normally
unless you deliberately use the raw ownership transfer described above.

Recording is now a scoped mutable borrow of both the world and recording:

```rust
// 0.2: recording start/stop could be managed separately from the borrowed world.
// world.try_start_recording(&mut recording)?;
// world.try_step(dt, sub_steps)?;
// world.try_stop_recording()?;

// 0.3: the session prevents concurrent world or recording use.
let mut session = world.record(&mut recording)?;
session.world().step(dt, sub_steps)?;
session.finish()?;
```

Dropping an unfinished `RecordingSession` stops recording, but call `finish()`
when finalization failure must be observed. A session deliberately prevents a
second recording session and direct use of either borrowed value until it is
finished or dropped. The owners also retain a shared native attachment marker:
if a session is deliberately forgotten, dropping either owner still detaches
the native recording first, and recording accessors return `RecordingInUse`
while the world remains attached.

Replay players no longer hold an exclusive lease on Box3D's process-global
length-unit setting. Each `RecPlayer` operation applies the recording's scale
only while it holds the Box3D global lock, then restores the scale observed at
call entry. Multiple players with different scales can coexist with ordinary
worlds, and `boxddd::raw::set_length_units_per_meter` remains available between
player calls. The obsolete `LengthUnitsInUse` error variant was removed.

## 5. Distinguish advancement from post-step failure

`World::step` remains the concise result-first operation, but a callback or task
failure can only be reported after Box3D has already advanced. Code that must
publish the completed step before handling that failure should use
`World::step_outcome`:

```rust
match world.step_outcome(dt, sub_steps) {
    Err(error) => {
        // Validation or admission failed. Box3D did not advance.
        return Err(error);
    }
    Ok(outcome) => {
        publish_events_and_transforms(&world)?;
        outcome.into_result()?;
    }
}
```

An outer `Err` means native simulation was not called. An `Ok(StepOutcome)`
proves native simulation and Rust provenance finalization completed;
`post_step_error()` may still expose the first callback or task failure.
`World::step` consumes the same outcome and returns that post-step error for
callers that do not need the distinction.

## 6. Rename to result-first methods

Fallible safe APIs use their canonical operation name and return `Result`.
The `try_` twins and panic-oriented counterparts were removed. Replace
`try_create_body`, `try_step`, `try_body_transform`, `try_set_body_transform`,
`try_destroy_shape`, and similar calls with `create_body`, `step`,
`body_transform`, `set_body_transform`, and `destroy_shape`.

```rust
// 0.2
let transform = world.try_body_transform(body_id)?;
world.try_set_body_transform(body_id, position, rotation)?;

// 0.3
let transform = world.body_transform(body_id)?;
world.set_body_transform(body_id, position, rotation)?;
```

Apply this rule to body, shape, joint, query, event, debug-draw, recording,
and global runtime APIs. In particular, `boxddd::allocated_byte_count()` is now
fallible:

```rust
let bytes: i32 = boxddd::allocated_byte_count()?;
```

## 7. Match canonical errors

Use `boxddd::Error` variants instead of matching a generic failure, parsing an
error string, or relying on a panic. The important categories are:

- `InvalidValue { context, reason }` for rejected public input. Match
  `InvalidValueReason` such as `NonFinite`, `OutOfRange`, `InvalidCombination`,
  `Malformed`, or `InteriorNul` when behavior depends on the cause.
- `ForeignHandle { kind }` and `StaleHandle { kind }` for provenance/liveness.
- `InCallback` for a safe API call attempted while Box3D executes a callback.
- `RecordingInUse` for a native recording resource that is still active.
- `CallbackPanicked`, `ProviderCallbackFailed`, `NativeFailure`, and
  `UnsupportedOnWasm` for callback/provider/native boundaries.

Update assertions to match the structured variant:

```rust
assert!(matches!(
    result,
    Err(boxddd::Error::InvalidValue {
        context: "world.gravity",
        reason: boxddd::error::InvalidValueReason::NonFinite,
    })
));
```

The coarse invalid-argument, invalid-ID, wrong-joint-family, string-NUL, index,
and resource-lifetime variants have been removed. Native failures that cannot
be attributed to caller input use `NativeFailure`; do not infer an input field
from that category.

## 8. Update callback and provider code

Visitor callbacks remain synchronous. A panic inside a Rust callback is
contained and returned as `Error::CallbackPanicked`; it must not unwind through
Box3D, a provider trampoline, or JavaScript. A callback cannot re-enter normal
safe `boxddd` APIs and receives `Error::InCallback` when it tries.

Keep callback state in Rust-owned closures and copy data needed after the call.
Do not retain borrowed callback data, native pointers, or raw IDs beyond the
callback. Preserve each API's stop value: boolean visitors use `false` to stop;
ray-style callbacks use their documented fraction sentinel.

For WASM provider builds, handle `UnsupportedOnWasm` explicitly. Do not map it
to an empty collection, a zero-hit result, or a successful no-op. Native and
provider callback failure are ordinary `Result` errors, not panic signals.

All callbacks and worker tasks invoked by one world step share one first-winner
failure slot. A failure is drained once after the native call, and the next step
starts with fresh invocation state. Do not infer that the world remained
unchanged merely because `World::step` returned a callback or task error; use
`step_outcome` when that distinction matters.

## 9. Handle compound byte ownership as a transfer

`Compound::into_bytes` consumes the compound and now returns a fallible owned
`CompoundBytes` value. `CompoundBytes::into_compound` consumes those bytes to
recreate an owned `Compound`.

```rust
// 0.2: byte conversion was treated as an infallible pointer handoff.
// let bytes = compound.into_bytes();

// 0.3: ownership transfer and native allocation failure are explicit.
let bytes = compound.into_bytes()?;
let restored = bytes.into_compound()?;
```

Only feed `into_compound` bytes obtained from `into_bytes`. Arbitrary byte
buffers remain a raw-interop concern because Box3D owns the allocation and
destruction contract.

## 10. Update Bevy integration code

`bevy_boxddd` no longer exports math extension traits. Replace method syntax
with free functions from `bevy_boxddd::math`:

```rust
// 0.2
// let position = bevy_position.to_boxddd_pos();

// 0.3
use bevy_boxddd::math::{to_boxddd_pos, to_boxddd_quat};

let position = to_boxddd_pos(bevy_position);
let rotation = to_boxddd_quat(bevy_rotation)?;
```

The temporary `DebugShape` metadata compatibility type was removed. Cache
`DebugShapeAsset` values by `DebugShapeHandle` when consuming lifecycle-aware
debug draw frames.

`BoxdddErrorPolicy::Panic` was removed. The remaining policies always emit a
`BoxdddErrorMessage`; `MessageAndLog` additionally logs it. Read error
messages in Bevy systems instead of using a panic policy as application control
flow.

The Bevy plugin now publishes events and synchronizes transforms for every
advanced step before emitting its `BoxdddErrorMessage`. A pre-native validation
or admission error still suppresses that step's publication. Destruction end
messages keep the surviving entity mapping; the destroyed side may correctly be
`None` and is not resurrected in the ECS map.

## Migration checklist

- Rebuild native and provider artifacts from the `0.3` source tree.
- Replace raw/pointer-backed definitions with owned values and builders.
- Replace `try_*` calls with the canonical result-first method names.
- Propagate or handle every returned `Result`.
- Keep safe IDs with their owner; use `boxddd::raw` only for explicit unsafe
  interop.
- Convert recording code to `World::record`, `RecordingSession::world`, and
  `RecordingSession::finish`.
- Use `World::step_outcome` when post-step publication must precede callback or
  task error handling.
- Update error matches and tests to canonical structured errors.
- Audit callback closures for panic containment, re-entry, copied data, and
  provider-mode `UnsupportedOnWasm` handling.
- Change compound byte conversion to `compound.into_bytes()?`.
- Replace Bevy math trait calls, remove `BoxdddErrorPolicy::Panic` usage, and
  preserve advanced-step publication before handling Bevy error messages.
