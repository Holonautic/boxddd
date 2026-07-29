# Changelog

This project contains three crates:

- `boxddd`: safe, ergonomic Rust wrapper over the Box3D C API.
- `boxddd-sys`: low-level FFI bindings plus vendored Box3D sources.
- `bevy_boxddd`: Bevy integration for authoring and visualizing Box3D scenes.

The format is based on Keep a Changelog, and this project follows Semantic Versioning.

## [Unreleased]

### Added

- The `0.3.0` development line pins Box3D to `781673be`, with a reproducible source manifest, an audited local patch set, and provider ABI v2 metadata shared by native and browser builds.
- `World::step_outcome` exposes whether native simulation advanced separately from a callback or task failure reported after the step; the new `callbacks_and_step_outcomes` example demonstrates custom-filter, pre-solve, friction, and restitution callbacks, a contained post-step failure with recovery, and the pre-native rejection boundary.

### Changed

- GitHub Pages WASM demos now default to the size-focused `wasm-release` profile, run `wasm-opt -Oz` when Binaryen is available, and show byte-level download progress while loading the Box3D provider and Bevy wasm assets.
- Pages pull requests and deployments now run a Chromium smoke that launches the Falling Stack demo, verifies both WASM modules load, and observes a changing rendered canvas before publishing.
- `generate-pages` now owns the Bevy testbed entry page and loader script, so `validate-pages` catches stale runtime markup before publishing.
- The C-backed `wasm32-wasip1` runtime smoke now runs under Wasmtime in both default and double-precision modes, keeping vendored Box3D C and Rust bindings aligned across precision configurations.
- CI now executes a representative headless core example set so validation, owner-scoped handles, callback recovery, recording/replay, and dedicated-thread integration are checked as runnable teaching paths rather than compile-only artifacts.
- Public definitions are now Rust-owned values with private FFI lowering; callers no longer construct or depend on native definition layouts.
- Public body, shape, joint, contact, dynamic-tree proxy, and replay IDs are opaque provenance handles. Worlds authorize resource access through an internal ledger, and recording now uses an explicit `RecordingSession` ownership boundary.
- Callback, traversal, debug-draw, task-system, and browser-provider paths now share one callback safety model, including panic containment and provider ABI v2 validation.
- The safe API is Result-first: recoverable operations use one unprefixed fallible method. Redundant `try_*` duplicates, panic convenience wrappers, and the `ApiError` / `ApiResult` aliases have been removed.
- Error reporting now uses typed validation, ownership, staleness, and native-failure categories instead of coarse invalid-ID and invalid-argument variants.
- Active recording conflicts now return `Error::RecordingInUse`; world and recording owners share the native attachment state even if a `RecordingSession` is deliberately forgotten.
- `bevy_boxddd` now uses direct conversion and integration APIs; forwarding extension traits and the configurable panic policy have been removed.
- `BoxHull` constructors now validate before entering FFI and return `Result`; the legacy `DebugShape` metadata shim has been removed in favor of `DebugShapeAsset` and `DebugShapeHandle`.
- Replay players apply their recording length scale only while each native call holds the Box3D lock, so differently scaled players, ordinary worlds, and ambient raw setters can coexist.
- Event `*_into` APIs use World-owned transactional scratch buffers. After capacity warmup they allocate no Rust heap memory, preserve caller buffers exactly on conversion failure, and never shrink caller capacity.
- Bevy publishes events and synchronizes transforms for an advanced step before emitting its callback or task error; pre-native failures still suppress publication.

### Fixed

- Continuous collision detection no longer attempts to invoke a null pre-solve callback.
- Forgetting a live `RecordingSession` can no longer leave a world pointing at freed recording storage or expose a byte slice that later native steps invalidate.
- Resource and contact provenance now spans the exact current and destruction-generated end-event windows, while retired IDs remain stale for world operations and expire on the next rotation.
- World callbacks, material callbacks, and worker tasks now share one per-step first-winner failure state that is drained once and recreated for the next invocation.
- Provider teardown drains callback errors before unregistering Rust tokens, and provider smoke now covers malformed replay recovery, mixed replay scales, destruction events, complete teardown, and two same-process instantiations.
- Release preflight always audits `0.3.0` against `v0.2.0` with an exact fail-closed breaking-API inventory and runs strict Clippy without extending generated-code allowances into handwritten modules.
- Pregenerated binding refreshes no longer depend on an ambient `rustfmt` or libclang's treatment of `UINT64_MAX` and the double-precision world-entry macro; default filter bits are now exported as `u64::MAX` on every host.

### Migration Notes

- Pages builders that need a different Rust profile can set `BOXDDD_PAGES_WASM_PROFILE=debug`, `release`, or `wasm-release`; set `BOXDDD_PAGES_WASM_OPT=0` to skip optional `wasm-opt` post-processing.
- `0.3.0` is a breaking architectural release. Replace `try_*` calls and panic wrappers with the corresponding unprefixed `Result` method, use `RecordingSession` for recording lifetimes, use `step_outcome` when advancement matters, and match typed `Error` variants rather than raw IDs or `InvalidArgument`.
- See the [0.2 to 0.3 migration guide](docs/migrating-0.2-to-0.3.md) for the complete API and Bevy integration migration checklist.

## [0.2.0] - 2026-07-06

This release focuses on browser demo support, Bevy teaching examples, debug draw renderer integration, release preflight coverage, and the first breaking API cleanup after `0.1.0`.

### Added

- Browser demo pages now expose direct Bevy + egui examples from the shared `testbed_3d` scene registry at <https://frankorz.com/boxddd/>.
- The Bevy testbed now includes `boxddd` showcase scenes for Query Lab, Debug Draw Inspector, Material Lab, and Stats Dashboard.
- Query Lab visualizes ray casts, AABB overlaps, sphere shape casts, and capsule mover casts from one Bevy scene; browser provider mode currently supports the ray and AABB visitor paths and labels the remaining visitor paths as unavailable.
- Added a `stats_profile` core example and a Bevy Stats Dashboard scene for world counters, awake body counts, capacity, and per-step profile diagnostics.
- `bevy_boxddd` now exposes Bevy math adapters and prelude extension traits for converting Bevy `Vec3`, `Quat`, and `Transform` values to and from `boxddd` math types.
- Debug draw can now collect lifecycle-aware frames with persistent shape assets, shape create/destroy events, and diagnostics, making renderer integration more practical than consuming command lists alone.
- The Bevy testbed now covers more official Box3D teaching scenarios, including dominoes, arch stacks, wind fields, ragdoll-style joint chains, and cylinder stacks.
- The official Box3D sample matrix is now case-level. Every vendored upstream sample registration, including `Replay / Viewer`, is classified as a faithful port, teaching adaptation, test-only proof, deferred case, or upstream reference.
- `bevy_boxddd` now supports procedural cylinder hull colliders through `HullDescriptor::cylinder` and `Collider::cylinder_hull`.

### Changed

- The root GitHub Pages URL is now the examples index rather than a marketing homepage.
- Example pages and the Bevy testbed now label official Box3D sample coverage separately from `boxddd` showcase entries.
- README and example docs now describe sample support as case-level tracking instead of implying every official sample is a one-to-one clone.
- Release checks now catch stale example pages, official sample matrix drift, and package omissions before publishing.

### Fixed

- WASM provider-mode debug draw can bridge Box3D debug callbacks for the browser demo bundle instead of reporting all debug draw collection as unsupported.
- WASM provider-mode world ray-cast and AABB-overlap visitors now work in browser demos instead of reporting misleading zero-hit results.
- The browser demo path now handles Box3D timer portability correctly.
- Procedural cylinder hull validation now rejects invalid side counts outside Box3D's supported `3..=32` range before reaching FFI.

### Migration Notes

- `DebugDrawCommand::Shape { shape, .. }` has changed to `DebugDrawCommand::Shape { handle, .. }`. Look up owned geometry through `DebugDrawFrame` events and cached `DebugShapeAsset` values. The old `boxddd::DebugShape` metadata type remains as a migration aid for stored metadata, but it is no longer emitted by frame commands.
- Prefer `World::debug_draw_frame` or `World::debug_draw_frame_into` when a renderer needs stable shape geometry across frames. The older command collection helpers remain for simpler command-only consumers.
- `Error::ProviderCallbackFailed` is a new public error variant. Exhaustive matches on `boxddd::Error` need a new arm; existing variant order is otherwise preserved for compatibility.
- `HullDescriptor` gained the `Cylinder` variant and is now non-exhaustive. Exhaustive matches in `bevy_boxddd` apps need to add a wildcard arm.

## [0.1.0] - 2026-07-04

### Added

- Initial Rust binding workspace for Box3D `v0.1.0`, including `boxddd-sys`, `boxddd`, and `bevy_boxddd`.
- `boxddd-sys` builds vendored Box3D C sources by default and uses pregenerated bindings for normal builds, including both single-precision and double-precision binding sets.
- `boxddd` provides a safe Rust API for worlds, bodies, shapes, joints, queries, events, debug draw, task-system callbacks, recording, and replay.
- Shape support covers spheres, capsules, hulls, transformed hulls, meshes, height fields, and compounds, with Rust-owned resources kept alive for native shape lifetimes.
- Collision and query helpers cover world/body/shape queries, allocation-aware `*_into` APIs, visitor callbacks, mover helpers, shape casts, time of impact, manifolds, and related geometry utilities.
- Event APIs support owned snapshots, reusable buffers, zero-copy closure-scoped views, and explicit raw escape hatches for bodies, contacts, sensors, and joints.
- Callback APIs cover custom filtering, pre-solve, friction mixing, restitution mixing, debug draw, and task scheduling with Rust panic containment across the C ABI.
- Optional interop features support `mint`, `glam`, `nalgebra`, `cgmath`, and `serde` for crate-owned value types.
- `bevy_boxddd` adds Bevy 0.19 ECS components, fixed-step systems, physics messages, query helpers, optional debug gizmos, optional physics picking, and windowed teaching examples.
- Example coverage includes core headless examples, an egui debug viewer, async/threading examples, math interop examples, and a switchable Bevy 3D testbed.

### Platform Support

- Native Windows, Linux, and macOS are the supported runtime targets for this release.
- Normal builds need a platform C compiler for the vendored Box3D C sources, but do not need CMake, LLVM, libclang, or bindgen.
- WASM support is experimental: compile-only and smoke-test paths exist, but browser apps and Bevy Web are not yet supported runtime targets.

### Known Boundaries

- `World`, native resources, dynamic trees, recordings, and replay players are intentionally `!Send` and `!Sync`.
- Raw `void*` user data, process-global hooks, file helpers, and selected diagnostics stay behind explicit raw APIs or `boxddd_sys::ffi`.
- Mobile targets are compile-only checks today, not supported runtime targets.
