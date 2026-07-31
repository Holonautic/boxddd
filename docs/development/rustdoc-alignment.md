# Rustdoc Alignment Status

This document tracks whether `boxddd` public rustdoc is semantically aligned with
the vendored Box3D C API documentation. It is intentionally separate from API
coverage: an API can be covered and still have weak or misleading documentation.
For `0.4.0`, recoverable public operations use one unprefixed Result-first
interface rooted in explicit Foundation initialization.

## Reference Sources

- `boxddd-sys/third-party/box3d/include/box3d/box3d.h`
- `boxddd-sys/third-party/box3d/include/box3d/types.h`
- `boxddd-sys/third-party/box3d/include/box3d/collision.h`
- `boxddd-sys/third-party/box3d/docs/*.md`

## Alignment Criteria

- Public rustdoc should explain the Rust API behavior, not mechanically restate
  the function name.
- Result-returning APIs should document the operation, the important typed error
  cases, and which validation occurs before entering Box3D.
- Rustdoc and examples should use the unprefixed Result-first API directly and
  represent recoverable failures as typed errors.
- Unsafe APIs should include a precise `# Safety` contract covering ownership,
  aliasing, pointer validity, and required lifetimes, in addition to their typed
  error behavior.
- Callback and event APIs should document transient lifetimes, callback return
  semantics, panic containment, and reentrant-call errors.
- FFI-backed resource APIs should document ownership and dependency lifetimes.
- Foundation-rooted factories should document process configuration, activity
  conflicts, callback rejection, and poison behavior.
- Units and coordinate spaces should be called out when they affect correct use.

## Module Status

| Module | Files | Status | Notes |
| --- | --- | --- | --- |
| Errors | `error.rs` | Aligned | Removed placeholder comments and kept concrete wrapper error semantics. |
| Foundation | `core/foundation.rs` | Aligned | Documents one-time normalized configuration, idempotence, conflicts, ordinary/replay activity, and permanent poison after uncertain initialization. |
| Collision helpers | `collision.rs` | Mostly aligned | Missing-docs clean; proxy docs aligned to GJK/shape-cast terminology. |
| Debug draw | `debug_draw.rs` | Aligned | Callback sink docs cover world-coordinate data, `draw_shape` early termination, callback panic mapping, and reentrant safe-API rejection. |
| World definition/runtime | `world.rs`, `world/runtime.rs`, `world/creation.rs`, `world/creation_transaction.rs` | Aligned | Foundation-rooted defaults, step, gravity, explosion, bounds, profile, counters, simulation toggles, worker count, transactional creation, and poison boundaries describe behavior and units. |
| Body API | `world/body_api.rs`, `body.rs` | Mostly aligned | Method-level docs no longer repeat method names; force, transform, mass, query, and list APIs now describe behavior. |
| Shape API | `world/shape_api.rs`, `shapes.rs` | Aligned | Method and type docs distinguish owned native allocations from borrowed shape/compound views tied to `&World` or `&Compound`. |
| Query API | `query.rs` | Aligned | World query and cast visitors document traversal control values, unordered hits, panic mapping, and reentrant safe-API rejection. |
| Recording/replay | `recording.rs` | Aligned | Recording byte lifetime, active-recording restrictions, Foundation-exclusive replay creation and validation, restoration, close failures, replay stepping, divergence, frame queries, and query drawing are documented. |
| Events/callbacks | `events.rs`, `callbacks.rs` | Aligned | Event docs distinguish owned snapshots, closure-scoped borrowed views, and unsafe raw slices; callback docs cover type-level constraints, panic mapping, default material mixes, and WASM limitations. |
| Joints | `joints/*.rs` | Aligned | Builders and runtime methods now document body-origin frames, units, event thresholds, and wrong-family errors. |
| Dynamic tree | `dynamic_tree.rs` | Aligned | Proxy generation, callback semantics, invalid clip fractions, panic mapping, and reentrant safe-API rejection are documented. |
| Task system | `core/task_system.rs` | Aligned | Docs preserve the `finishTask` blocking requirement and explain why non-blocking job systems can deadlock. |

## Related Audits

- `docs/development/ffi-lifetime-audit.md` records the current ownership,
  borrowed-view, callback, recording, compound-byte, and task-system lifetime
  decisions.

## Current Quality Gate

Run these before considering the documentation pass complete:

```bash
cargo fmt --all --check
cargo test -p boxddd --doc
cargo rustdoc -p boxddd --all-features -- -D missing_docs
RUSTDOCFLAGS="-D warnings" cargo doc -p boxddd --all-features --no-deps
```
