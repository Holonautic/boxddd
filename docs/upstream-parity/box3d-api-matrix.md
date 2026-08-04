# Box3D API Parity Matrix

This matrix tracks the vendored Box3D C API slice used by `boxddd-sys` and the Foundation-rooted, Result-first `boxddd` surface for the `0.4.0` release line.

Status legend:

- **Wrapped**: available on the safe `boxddd` API.
- **Raw-only**: intentionally exposed through `boxddd_sys::ffi` only for this release.
- **Unsafe raw**: available through explicit `unsafe`/raw-named `boxddd` APIs, not part of the safe API contract.
- **Deferred**: known upstream API that needs a focused safe design before it should be wrapped.
- **Not applicable**: internal/package concern, not a safe wrapper target.

The matrix and its automation prove classification and upstream correspondence,
not semantic safety. A wrapped claim is supported by owned-definition lowering,
owner-before-FFI provenance checks, ledger transaction finalizers, lifetime and
recording-session tests, shared callback/task protocols, and typed-error tests.
Calling `boxddd_sys::ffi` directly bypasses those guarantees and makes the caller
responsible for native ownership, synchronization, callback, and destruction
invariants; raw mutation must not silently share ownership with a live safe
wrapper.

The low-level source contract is manifest-driven: it pins one exact upstream
commit, applies only digest-checked declarative patches, and names the provider
v2 module, bridge revision, and capability subset from the same generated
contract.

## Summary

| Upstream area | Representative symbols | 0.4 status | Notes |
| --- | --- | --- | --- |
| Raw bindings/build | all `b3*` symbols in vendored headers | Wrapped in `boxddd-sys` | The exact source, declarative patches, provider v2 contract, and generated artifacts are manifest-controlled. Pregenerated default and double-precision bindings are checked in; `bindgen` refresh is explicit. |
| Version/build metadata | `b3GetVersion`, `b3IsDoublePrecision`, `b3GetByteCount` | Wrapped | `version`, `is_double_precision`, `allocated_byte_count`. |
| Allocator/assert/log hooks | `b3SetAllocator`, `b3SetAssertFcn`, `b3SetLogFcn` | Raw-only | Process-global hooks need a separate initialization/ownership policy. |
| Process configuration | `b3SetLengthUnitsPerMeter`, `b3GetLengthUnitsPerMeter`, `b3SetStallThreshold`, `b3GetStallThreshold` | Wrapped | `Foundation::initialize` validates, writes, reads back, and freezes both settings before any safe native work. Identical normalized initialization is idempotent; conflicting or uncertain initialization fails closed. |
| Timing/files/platform helpers | `b3GetTicks`, `b3Sleep`, `b3Hash`, `b3ReadBinaryFile`, `b3WriteBinaryFile` | Raw-only | Not required for the safe physics model; use Rust std/time/io unless a Box3D-specific reason appears. |
| Scalar/vector validation | `b3IsValidFloat`, `b3IsValidVec3`, `b3IsValidQuat`, `b3IsValidTransform`, `b3IsValidPosition`, `b3IsValidWorldTransform` | Wrapped | Value types expose `is_valid`/`validate` where applicable. |
| Extra math helpers | `b3PointToSegmentDistance`, `b3LineDistance`, `b3SegmentDistance`, `b3Atan2`, `b3ComputeCosSin`, `b3MakeQuatFromMatrix`, `b3ComputeQuatBetweenUnitVectors`, `b3Steiner` | Wrapped | Segment and line distance helpers, deterministic scalar helpers, quaternion constructors, and Steiner inertia are wrapped with validation. |
| World lifecycle | `b3CreateWorld`, `b3DestroyWorld`, `b3World_IsValid`, `b3GetWorldCount`, `b3GetMaxWorldCount` | Wrapped/Not applicable | `Foundation::create_world` creates a single-owner World with an opaque owner token and retained ordinary activity; global world-count diagnostics are omitted because they do not fit the safe ownership model. |
| World stepping/drawing | `b3World_Step`, `b3World_Draw` | Wrapped | Result-returning `step`, debug draw callback, and collection APIs. |
| World runtime metrics/tuning | bounds, gravity, sleeping, continuous, warm starting, speculative, thresholds, contact tuning, worker count, profile, counters, capacity, rebuild static tree | Wrapped | Dump/debug-print functions remain raw-only. |
| World user data | `b3World_SetUserData`, `b3World_GetUserData` | Unsafe raw | Exposed through `boxddd::raw`; raw pointer ownership remains entirely caller-defined. |
| World callbacks | custom filter, pre-solve, friction, restitution | Wrapped | Tokenized registration and shared callback execution state contain panics, preserve the first failure, retire replaced closures, and block reentrant safe access. |
| World task callbacks | `b3EnqueueTaskCallback`, `b3FinishTaskCallback`, `userTaskContext` | Wrapped | `TaskSystem::blocking_threads()` provides a Rust-owned path using the shared callback-failure protocol and Box3D's required blocking `finishTask` semantics. Tokio, Rayon, Bevy Tasks, and arbitrary executor adapters are application integration work, not unclassified Box3D symbols. |
| World queries | `b3World_OverlapAABB`, `b3World_OverlapShape`, `b3World_CastRay`, `b3World_CastRayClosest`, `b3World_CastShape`, `b3World_CastMover` | Wrapped | Owned, reusable-buffer, and visitor variants exist where the upstream callback path supports them. |
| Character mover planes | `b3World_CollideMover`, `b3Body_CollideMover`, `b3SolvePlanes`, `b3ClipVector` | Wrapped | World/body collide-mover plane collection, plane solving, and clipping helpers are wrapped with owned values. |
| Explosion | `b3World_Explode`, `b3DefaultExplosionDef` | Wrapped | `ExplosionDef` validates finite position/radius/falloff/impulse values before applying the world explosion. |
| Memory/stat dump helpers | `b3World_DumpMemoryStats`, `b3World_DumpShapeBounds`, `b3World_DumpAwake`, `b3World_Dump` | Raw-only | Diagnostic printing is not wrapped in the safe API. |
| Recording/replay | `b3CreateRecording`, `b3World_StartRecording`, `b3World_StopRecording`, load, validate, `b3RecPlayer_*` except debug-shape callbacks | Wrapped/Raw-only | `World::record` returns an exclusive `RecordingSession` that stops on `finish` or drop. `Recording::load_from_file` returns an owned recording, and `save_to_file` writes validated bytes through Rust IO; the upstream native save helper stays raw-only. Replay world IDs carry separate provenance and are intentionally read-only. `Foundation::create_replay_player` owns exclusive process activity through native destruction and verified configured-scale restoration; ordinary owners, transient helpers, and additional players fail with `FoundationBusy` during that interval. |
| Replay debug-shape callbacks | `b3RecPlayer_SetDebugShapeCallbacks` | Raw-only | Safe replay query drawing reuses the debug draw adapter; custom replay debug-shape lifetime callbacks stay raw-only for this release. |
| Body lifecycle/type/name/transform | `b3CreateBody`, `b3DestroyBody`, type, name, position, rotation, transforms, local/world point/vector conversion, redundant world getter | Wrapped/Omitted | `BodyDef` is an owned Rust value and lowers to its pointer-bearing native form only inside the crate. Opaque body IDs are authorized against the owning `WorldLedger` before FFI. The redundant raw body-to-world getter is omitted. |
| Body user data | `b3Body_SetUserData`, `b3Body_GetUserData` | Unsafe raw | Exposed through `boxddd::raw`; raw pointer ownership remains entirely caller-defined. |
| Body velocity/forces/impulses/mass/damping/sleep/enabled/bullet/motion locks | `b3Body_*` runtime methods | Wrapped | The unprefixed Result-first methods perform provenance, validity, and scalar checks before FFI and return typed errors. |
| Body attached resources/events/queries | body shapes, joints, contacts, AABB, closest point, ray/shape overlap/casts, collide mover | Wrapped | Shapes/joints/contacts/AABB, body-local closest point, ray casts, shape casts, overlap, and collide mover are wrapped. |
| Shape creation | sphere, capsule, hull, transformed hull, mesh, height field, compound | Wrapped | Owned definitions lower privately; one typed creation transaction reserves, claims, binds, postflights, and atomically publishes native identity. Proven-owned failures compensate, while untrusted identities or unverifiable compensation poison the World. |
| Shape lifecycle/runtime | destroy, valid, type, body, redundant world getter, sensor, density, friction, restitution, material, filter, event toggles/getters, AABB, mass data, geometry getters/setters | Wrapped/Omitted | Shape IDs are authorized before FFI, and ledger retirement/finalizers keep body relations, callback provenance, contact epochs, and backing resources synchronized. Mesh material indexing is bounds-checked; hull, mesh, and height-field readback returns lifetime-bound views. The redundant raw world getter is omitted. |
| Shape user data/wind | `b3Shape_SetUserData`, `b3Shape_GetUserData`, `b3Shape_ApplyWind` | Wrapped/Unsafe raw | Pointer user data is exposed through `boxddd::raw`; wind is wrapped with finite/non-negative input validation. |
| Shape contacts/sensors/query helpers | `b3Shape_GetContactData`, `b3Shape_GetSensorData`, `b3Shape_RayCast`, closest point | Wrapped | Contact/sensor buffers, AABB, mass data, direct ray-cast, closest-point, and geometry view helpers are wrapped. |
| Geometry resources | hull/mesh/height field/compound create/destroy helpers | Wrapped/Raw-only | RAII constructors, hull clone/transform helpers, box scaling, arbitrary mesh creation, wave/torus/hollow/platform meshes, custom height fields, compound builders, compound child/material introspection, compound child queries, and `CompoundBytes` owner round-trips are wrapped. File-backed height fields and arbitrary caller-owned compound byte buffers stay raw-only. |
| Dynamic tree | `b3DynamicTree_*` except file save/load | Wrapped/Raw-only | `DynamicTree` owns create/destroy, proxy lifecycle, metrics, rebuild/validation, AABB/closest/ray/box cast visitors, and panic containment. File save/load stays raw file IO. |
| Standalone collision | sphere/capsule/hull/mesh/height field/compound mass/AABB/overlap/ray/shape cast; GJK distance, TOI, sweep transforms, plane solving, clipping, and sphere/capsule/hull/triangle manifolds | Wrapped | The value-returning collision helpers are wrapped with validation and owned outputs. Mesh queries support early-stop visitors; height-field triangle queries return owned hits without promising native early stop. |
| Joints common runtime | destroy, valid, type, bodies, redundant world getter, local frames, collide connected, wake, forces/torques, separations, tuning, thresholds | Wrapped/Omitted | Joint definitions lower privately, pending graph relations publish after successful creation, and owner/resource tokens are checked before FFI. Wrong-family calls return `Error::InvalidValue` with the `joint.type` context and `InvalidCombination` reason; the redundant raw world getter is omitted. |
| Joint user data | `b3Joint_SetUserData`, `b3Joint_GetUserData` | Unsafe raw | Exposed through `boxddd::raw`; definition/event fields use `raw_user_data` naming where pointer values can surface. |
| Parallel joint | create, spring, damping, max torque | Wrapped | Typed definition and runtime APIs. |
| Distance joint | create, length, spring/range, limit/range, motor/speed/force/current length | Wrapped | Typed definition and runtime APIs. |
| Motor joint | create, linear/angular velocity, max velocity force/torque, spring tuning | Wrapped | Typed definition and runtime APIs. |
| Filter joint | create | Wrapped | No runtime family-specific state beyond common joint APIs. |
| Prismatic joint | create, spring, target translation, limits, motor, translation/speed | Wrapped | Typed definition and runtime APIs. |
| Revolute joint | create, spring, target/angle, limits, motor speed/torque | Wrapped | Typed definition and runtime APIs. |
| Spherical joint | create, cone/twist limits, spring, target rotation, motor velocity/torque | Wrapped | Typed definition and runtime APIs. |
| Weld joint | create, linear/angular spring tuning | Wrapped | Typed definition and runtime APIs. |
| Wheel joint | create, suspension, spin motor, steering and limits | Wrapped | Typed definition and runtime APIs. |
| Contact id/data | `b3Contact_IsValid`, `b3Contact_GetData` | Wrapped | Safe `ContactId`, `ContactData`, and `Manifold` value types. |
| Default definition values | `b3DefaultWorldDef`, `b3DefaultBodyDef`, `b3DefaultShapeDef`, `b3DefaultSurfaceMaterial`, joint defaults, query filter, debug draw | Wrapped | Foundation factories create scale-aware World, body, and shape definitions; private lowering keeps raw pointer fields out of public construction and preserves upstream defaults. `SurfaceMaterial::default` and `QueryFilter::default` remain scale-independent. |
| Debug draw default/color | `b3DefaultDebugDraw`, `b3GetGraphColor` | Wrapped/Raw-only | Default debug draw is used internally; graph color helper remains a raw low-level diagnostic helper. |

## Release Policy For Future Deferred APIs

No vendored upstream `B3_API` symbols are currently classified as deferred. Future deferred entries are not hidden unknowns; they must stay outside the safe claim until they have one of:

- raw pointer/user-data ownership policy
- callback/threading design
- allocation and lifetime tests for callback-returned buffers
- representative examples proving the intended workflow

Until then, downstream users can access the underlying C API explicitly through `boxddd_sys::ffi`.
