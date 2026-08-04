# FFI Lifetime Audit

This document tracks ownership and lifetime decisions for the safe `boxddd`
wrapper over the vendored Box3D C API.

## Current Verdict

The audited `0.4.0` architecture has the required ownership, provenance, and
failure boundaries for the safe wrapper:

- `WorldDef`, `BodyDef`, `ShapeDef`, and joint definitions are owned Rust values.
  Pointer-bearing native definitions exist only inside carrier modules; crate
  callers request a complete native create operation and cannot copy out the
  raw definition. Backing remains alive through the exact native call.
- Public body, shape, joint, contact, tree-proxy, and replay identifiers are
  opaque provenance handles. Owner plus resource or contact-epoch tokens are
  checked before an ID is converted to native bits or passed to FFI, so foreign
  and stale handles return typed errors without probing the wrong native owner.
- `WorldLedger` stages allocation with pending resources and publishes IDs only
  after native creation succeeds. Destructive mutations prepare cascades or
  contact epochs before FFI and apply their `finish_*` finalizers only after the
  native mutation, keeping the Rust graph and Box3D state in the same order.
- Resource provenance rotates through active, pending, and visible event
  generations. Destruction-generated end events retain inert identities for the
  native readable window without making those IDs valid world capabilities.
- Public recoverable operations use one unprefixed Result-first API. Validation,
  provenance, callback-state, allocation, and native failures remain explicit.
- `World`, `Recording`, `RecPlayer`, `DynamicTree`, and native geometry owners
  are intentionally `!Send` and `!Sync` through `PhantomData<Rc<()>>`. Every
  configuration-dependent native owner retains its Foundation lease through
  native destruction.
- Foundation admission permits any number of ordinary lifetime or transient
  leases, or one exclusive replay lease. Admission is non-blocking and returns
  `Error::FoundationBusy` on conflict. Foundation poison is an absorbing bit in
  the same atomic activity state, so no stale health observation can admit a
  native call after replay teardown poisons and releases exclusivity.
- Ordinary owner calls validate callback state, Foundation health, owner
  provenance, and native identity before entering Box3D. They retain an owner
  call frame through native return and Rust-side finalization, but independent
  owners share no operation mutex. Worldless calls retain a transient lease for
  their complete native interval.
- One narrow World-slot mutex covers only native World creation/destruction and
  replay paths that create or destroy an internal World. Every path acquires its
  ordinary or exclusive Foundation activity before that mutex. Separate Worlds
  can therefore step concurrently, while native slot mutation remains ordered.
- Safe APIs reject calls made from Box3D callbacks before Foundation lookup,
  activity admission, or any native call. Callback-time owner destruction moves
  complete cleanup into the enclosing owner call frame and drains it after the
  native boundary and any World-slot guard have ended.
- Local and shared callback execution state contains panics and carries the first
  failure back across the FFI return boundary. Tokenized pending/registered
  callback state makes replacement, snapshots, and retirement explicit; the task
  adapter uses the same shared failure protocol plus Box3D's blocking
  `finishTask` contract.
- `World::step_outcome` separates pre-native admission failure from an advanced
  step that carries a post-step callback or task failure. Irreversible ledger and
  event finalizers run before that failure is exposed.
- Body/shape ledger sidecars keep `MeshData`, `HeightField`, and `Compound` alive
  while Box3D shapes refer to them.
- Body, shape, joint, tree-proxy, and World creation use typed pending, claimed,
  bound, and committed states. Empty output is recoverable, untrusted output is
  never destroyed speculatively, and every trusted unpublished candidate has a
  verified compensation path.
- `RecordingSession` borrows both `World` and `Recording`, while a shared
  attachment marker lets either owner stop the native recording even if the
  session is deliberately forgotten.

## Foundation Activity Classification

| Rust surface | Native ownership | Foundation activity | Transfer and teardown rule |
|---|---|---|---|
| `World` | `b3WorldId` plus callbacks, tasks, ledger, and resource sidecars | Ordinary lifetime lease | Acquired before World creation. `WorldRawParts` moves the complete World owner, including the same lease. Native World destruction and sidecar retirement finish before release. |
| `DynamicTree` | Inline `b3DynamicTree` allocation | Ordinary lifetime lease | Acquired before `b3DynamicTree_Create`; the complete inner capsule, proxy ledger, and lease survive through native destroy or deferred callback cleanup. |
| `Hull` | `b3HullData*` | Ordinary lifetime lease | Acquired before native creation and released only after `b3DestroyHull`. |
| `MeshData` | `b3MeshData*` | Ordinary lifetime lease | Acquired before native creation and retained while a World shape sidecar borrows the mesh. |
| `HeightField` | `b3HeightFieldData*` | Ordinary lifetime lease | Acquired before native creation and retained while a World shape sidecar borrows the height field. |
| `Compound` / `CompoundBytes` | One allocation in live or serialized representation | One ordinary lifetime lease | `into_bytes` and `into_compound` move the same non-cloneable lease with the allocation; neither conversion opens an unleased interval. |
| `RecPlayer` | `b3RecPlayer*`, its replay World, registry, and keyframes | Exclusive replay lifetime lease | Acquired before `b3RecPlayer_Create`. Creation compensation, `close`, and Drop destroy the internal World under the World-slot mutex, restore and read back the configured scale, then release exclusivity. |
| Detached `Recording` | Scale-independent byte storage | Storage-only; no idle lifetime lease | Creation, byte/file access, and mutation use transient admission. Its scale-independent cleanup may coexist with an idle replay owner. While attached, `RecordingSession` and the attachment state cannot outlive the World's ordinary lease. |
| Inline shapes, definitions, and opaque IDs | No standalone native allocation | None while idle | Any worldless helper that enters Box3D takes a transient lease for that native call. IDs borrow authority from their owning World, tree, or replay player. |
| Collision, math, query-preparation, version, and allocation helpers | No retained native owner | Transient ordinary lease | Admission covers the complete native call and rejects while replay is active. |

`ReplayLease`, `OrdinaryLease`, and `TransientLease` are crate-private and not
cloneable. Owner capsules move them rather than reconstructing them. Cleanup
that runs from a callback is transferred whole to the nearest owner call frame;
if no frame can retain it, the complete owner is intentionally retained rather
than releasing a lease before uncertain native state is destroyed.

## Native Creation Classification

This table is pinned to the vendored Box3D source. A Box3D revision that changes
one of these call graphs must re-audit the transaction adapter before the pin is
updated.

| Rust family | Pinned native entry | Retained input | Synchronous safe callback classification |
|---|---|---|---|
| Body | `b3CreateBody` | No borrowed input; the name is copied | Callback-free |
| Sphere, capsule, and hull variants | `b3Create*Shape` | Geometry is copied or cloned; the name and material array are copied | Callback-free. A capsule shorter than linear slop is legitimately published as a sphere. |
| Mesh shape | `b3CreateMeshShape` | `MeshData` must outlive the native shape | Callback-free |
| Height-field shape | `b3CreateHeightFieldShape` | `HeightField` must outlive the native shape | Callback-free |
| Baked compound shape | `b3CreateBakedCompoundShape` | `Compound` must outlive the native shape | Callback-free |
| Nine joint families | `b3Create*Joint` | No borrowed input | Callback-free |
| Dynamic-tree proxy | `b3DynamicTree_CreateProxy` | AABB, category bits, and user data are copied | Callback-free |
| World | `b3CreateWorld` / `b3CreateWorldDoublePrecision` | Task and debug contexts are retained by the World | Callback-free during bootstrap |

Shape `invokeContactCreation` creates a broad-phase proxy and buffers movement;
it does not run the custom-filter callback synchronously. The custom filter is
first eligible during a later broad-phase update, after the shape provenance
entry has committed. Process allocator and assertion hooks remain raw-only and
are outside the safe callback classification.

Creation follows `prepare -> reserve -> native create -> structural claim ->
bind -> postflight -> publish -> disarm`:

- Resource IDs are classified before publication: null sentinel, defensive
  native validity, embedded World slot, exact World generation, then active or
  observable-retired ledger state. Getters run only after validity succeeds.
- Active and foreign outputs are untrusted. They are not destroyed, the owner
  is poisoned, and pointer backing is quarantined through World destruction.
- A target-World candidate is armed only after its identity is available.
  Observable-retired collisions and later failures destroy that candidate and
  verify invalidity before backing can drop.
- Shape compensation preserves the creating definition's `updateBodyMass`
  value. Shape parent, ordered joint endpoints, resource family, tree node
  fields, and native counts are exact postflight checks.
- Ledger publication consumes a non-copyable bound witness. All vacancy and
  relation checks happen before the logical commit; an impossible later
  conflict poisons the owner and keeps shape backing in either the transaction
  guard or ledger until native teardown.
- World bootstrap uses a fixed-capacity active-slot registry under the World-slot
  mutex. Capacity exhaustion is rejected before FFI, and only a newly claimed
  valid slot with an exact `world_count + 1` observation may arm
  `b3DestroyWorld`; this is required because upstream decrements the global
  World count before validating the ID passed to destruction.
- World teardown destroys and verifies the native World, retires the active slot,
  and only then releases callbacks, provider state, ledger resources, and the
  quarantine. Unverifiable teardown poisons Foundation and retains the complete
  owner capsule.

## Audited Areas

### Event Views

Official Box3D docs say world event buffers are transient and must not be stored.
The Rust API exposes owned snapshots plus `with_*_events_view` closure APIs.

Status: acceptable.

Reasoning:

- The borrowed event views are only constructed inside a closure call.
- A minimal Rust lifetime probe confirmed that `FnOnce(View<'_>) -> T` does not
  allow returning the borrowed view from the closure in safe Rust.
- Raw event slice APIs are marked `unsafe` and document that callers must not
  store the slices or dereference raw `userData` without upholding validity.
- The no-escape contract is covered by rustdoc `compile_fail` examples for body,
  sensor, contact, and joint closure-scoped event views.
- Owned snapshot methods allocate independent values and never borrow the
  World's reusable event scratch.
- Each `*_events_into` family owns a separate `RefCell` scratch buffer inside the
  World. Sensor and contact groups fill every member before one atomic commit;
  any conversion failure leaves caller elements, length, pointer, and capacity
  unchanged.
- A successful commit retains a sufficiently large caller allocation and adopts
  scratch only when scratch is larger. Measured body, sensor, contact, and joint
  fixtures prove zero Rust allocations across smaller and larger non-empty
  windows after warmup.
- Destroyed resource entries remain pending while the current snapshot is still
  readable, become visible for the next destruction end-event window, and then
  expire. Same-slot generation reuse and ambiguous raw keys fail closed.

### Recording Bytes

Official Box3D docs say `b3Recording_GetData` is valid until the buffer is
modified or destroyed. The Rust API returns `&[u8]` tied to `&Recording` and
blocks byte access while the recording is active on a live world.

Status: acceptable.

Reasoning:

- Borrowed bytes prevent mutable use of the same `Recording`.
- `World::record` returns `RecordingSession<'_>`, which mutably borrows both the
  world and recording. Safe Rust therefore cannot inspect, mutate, move, or drop
  either owner while recording is active.
- `Recording::load_from_file` returns a fresh owned recording buffer. It does
  not expose borrowed native storage or attach the recording to a world.
- `Recording::save_to_file` copies bytes under transient Foundation admission.
  The active session's mutable borrow prevents safe callers from saving
  concurrently. The
  upstream native save helper remains raw-only because the safe API already
  writes validated bytes through Rust IO.
- `RecordingSession::finish` stops and finalizes the stream; dropping an active
  session performs the same native stop before releasing either mutable borrow.
- `World` and `Recording` share a stable attachment marker. `mem::forget` can
  skip the session destructor but cannot make the native pointer outlive the
  recording: either owner detaches first, and byte access returns
  `Error::RecordingInUse` until detachment.
- Replay creation and validation are rooted in `Foundation`. A detached
  `Recording` may remain alive because it is storage-only, but accessing its
  native buffer is transient activity and therefore rejects while replay owns
  exclusivity.
- `RecPlayer` validates the fixed replay header, acquires the exclusive lease
  before native creation, and rejects scales that fail Foundation's complete
  derived-default range contract before entering native code. The recorded
  scale remains installed for the complete player lifetime. A live player
  excludes ordinary Worlds, trees, owned geometry, transient helpers, and other
  players.
- Replay create, destroy, and opaque validation take the World-slot mutex before
  the native path can create or destroy its internal World. A creation guard
  destroys any unpublished player and explicitly restores the Foundation
  baseline if native creation fails after changing scale.
- `RecPlayer::close` destroys the native player, explicitly restores and reads
  back the configured scale, drains deferred cleanup, and reports restoration
  failure. Infallible Drop performs the same teardown and poisons Foundation on
  an unverifiable result.

### Shape Resource Views

`ShapeHull<'_>`, `ShapeMesh<'_>`, and `ShapeHeightField<'_>` borrow native
geometry owned by a live shape/world.

Status: acceptable.

Reasoning:

- The returned views are tied to `&World`.
- Safe Rust cannot mutably destroy or replace the same shape while those views
  are live.
- Resource-backed mesh, height-field, and compound shapes keep their owned Rust
  resources in the `World` sidecar map until shape/body destruction or shape
  replacement.

Raw bypass boundary:

- `Foundation::initialize` is the only safe writer for the native length-unit and
  stall-threshold globals. Calling their sys functions directly bypasses frozen
  configuration, activity admission, and read-back verification.
- Direct `boxddd_sys::ffi` calls bypass definition validation, owner-before-FFI
  authorization, ledger publication/retirement, resource sidecars, callback
  guards, and transaction finalizers. Mixing such mutations with a live safe
  owner can invalidate borrowed views or desynchronize provenance state. The
  caller must isolate raw ownership and uphold every native lifetime, aliasing,
  synchronization, and destruction invariant manually.

### Compound Byte Conversion

Box3D converts a compound to bytes by returning the same allocation after
scrubbing internal pointers. It converts bytes back to a compound by mutating the
same allocation in place.

Status: acceptable.

Reasoning:

- `Compound::into_bytes` takes the complete inner owner only after native
  conversion succeeds, moving both the allocation and its ordinary lease to
  `CompoundBytes`.
- `CompoundBytes::drop` calls `b3DestroyCompound` on the same allocation, which
  matches the upstream representation.
- `CompoundBytes::into_compound` likewise takes the complete inner owner only
  after successful conversion, transferring the same allocation and lease back
  to `Compound`.

### Callback Contexts

World callback contexts are boxed and kept inside `WorldCallbacks`; raw Box3D
callback pointers are cleared before the world is destroyed.

Status: acceptable.

Reasoning:

- Context boxes outlive the registered callbacks because they are owned by the
  `World`.
- `PendingCallback` allocates a fresh resource token before registration;
  `RegisteredCallback` publishes, snapshots, validates, and retires that token so
  a replaced closure cannot be mistaken for the current registration.
- Registered callback types are `Send + Sync + 'static` and receive only value
  data such as shape ids, contact point/normal, or material inputs. They do not
  receive a `World` capability.
- `World` contains `PhantomData<Rc<()>>`, so it is not `Send` or `Sync`; safe
  Rust cannot move a live `World` into these registered callbacks.
- Callback replacement runs under the World's ordinary owner admission and
  exclusive `&mut World` borrow. It does not acquire the World-slot mutex or
  serialize unrelated Worlds.
- `World::drop` clears raw callbacks before destroying the native world.
- Material-mix callbacks use registry slots and release them when both callbacks
  are cleared.
- Local callback state is used for synchronous visitor-style calls. Each world
  step installs one fresh shared invocation state into the World-owned stable
  task context; world, material, and task callbacks all race through that same
  first-winner slot, which is detached and drained after native return.
- Callback trampolines catch panics and report them after stepping as
  `Error::CallbackPanicked`.
- Non-finite material-mix return values fall back to the upstream default mix
  instead of being treated as panics.

Token decision:

- Do not require a global `OutsideCallbackToken` for every `World` method. That
  would make the whole safe API harder to use while adding little soundness:
  the important callback registration paths are already type-constrained, and
  the runtime `Error::InCallback` guard is still needed for raw handles, global
  state, FFI reentrancy, debug draw/query visitors, and future callback surfaces
  that Rust's type system cannot fully model.

### Debug Draw Callbacks

Debug draw callbacks receive transient Box3D draw data while Box3D walks the
world for visualization.

Status: acceptable.

Reasoning:

- The safe `DebugDraw` trait receives copied value types and optional safe shape
  identifiers instead of raw Box3D pointers.
- Debug drawing enters the same callback-state guard used by other Box3D
  callback surfaces, so reentrant safe `World` APIs return `Error::InCallback`.
- `World::debug_draw` catches Rust panics and reports
  `Error::CallbackPanicked` after the FFI call returns.
- The safe `DebugDraw::draw_shape` method intentionally returns `()`. The
  upstream C callback type returns `bool`, but the current Box3D draw loop does
  not consume that return value, so the safe API does not expose it as a
  misleading control-flow contract.

### Task System

Box3D requires `finishTask` to block until a task completes. The current
`TaskSystem::blocking_threads` adapter joins thread handles in `finishTask`.

Status: acceptable.

Reasoning:

- The reusable scheduler state is an `Arc`, while each World owns an
  address-stable installed task context that points at the current invocation.
- Worker handles retain the invocation state selected at enqueue time, so a
  failure does not prevent required join and exactly-once accounting.
- Panics in enqueue, task execution, or finish are contained and reported as the
  advanced step's post-step error. The next step installs fresh failure state;
  scheduler statistics remain intentionally sticky for diagnostics.

Protocol constraint:

- Scheduler adapters must remain deliberately conservative. Bevy or async task
  pools are compatible only if they can provide the exact blocking `finishTask`
  semantics and participate in the shared callback-failure protocol.

## Token Guard Decision

Do not introduce a general outside-callback token. The architecture uses narrow
capabilities where they encode a real lifetime: `RecordingSession` owns the
active recording interval, closure-scoped event views prevent transient buffers
from escaping, and geometry views borrow their native owner. New public borrowed
surfaces should follow one of those patterns instead of adding a token to every
`World` method.

Evidence:

- Definition tests cover owned names, defaults, validation, and private native
  lowering behavior without exposing pointer-bearing C definitions.
- Provenance and ledger tests cover foreign owners, stale generations, native-bit
  reuse, discarded staging, cascaded destruction, contact epochs, destruction
  windows, and ambiguous same-slot reuse.
- Event borrowed views are exposed only through `with_*_events_view` closures and
  body, sensor, contact, and joint views have rustdoc `compile_fail` examples
  showing they cannot escape.
- An isolated guarded allocator test covers all four non-empty `*_events_into`
  families across variable counts, while transactional unit tests preserve the
  exact caller allocation on conversion failure.
- Recording tests cover exclusive `RecordingSession` borrowing, explicit finish,
  drop finalization, and both owner-drop orders after a deliberately forgotten
  session.
- Foundation subprocess tests account for World/raw-parts, DynamicTree, Hull,
  MeshData, HeightField, Compound/bytes, detached Recording, and RecPlayer
  activity. They cover busy admission in both directions, second-player
  rejection, deep create-failure restoration, explicit-close poisoning, and
  callback-frame retention of replay exclusivity.
- `ffi_lifetime_inventory` is intentionally a lexical module-boundary change
  detector. It derives native function names from the pregenerated bindings,
  records which production modules reference native or provider functions, and
  requires every such module to carry reviewed Foundation activity classes. It
  does not resolve Rust names, follow closures, infer receivers, or prove guard
  lifetime and lock order. Those guarantees come from private capability types,
  compile-fail contracts, focused Foundation/callback/transaction/concurrency
  tests, Clippy, and code review. Calls added inside an already reviewed module
  are reviewed through the normal diff and that module's interface and behavior
  tests rather than through a partial Rust frontend.
- `foundation_concurrency` synchronizes two independent Worlds from inside
  native custom-filter callbacks. Both steps must be in flight together; the
  former process mutex would make the watchdog fail. Separate stress cases cover
  concurrent World creation/destruction and World-slot mutation racing replay
  validation, construction, and destruction without leaking activity or
  deadlocking the activity-before-slot lock order.
- Callback registration signatures and `World: !Send + !Sync` prevent moving a
  live `World` into Box3D callbacks in safe Rust.
- World callback and task tests cover panic containment, real-thread first-winner
  behavior, fresh per-step recovery, post-step provenance, exactly-once joins,
  and non-finite material-mix fallback behavior.
- Debug draw tests cover panic containment, callback-guard rejection, invalid
  bounds rejection, and shape callback visitation.
- Shape and compound borrowed views are tied to `&World` or `&Compound` and now
  document that replacing or destroying the owner invalidates the native view.
