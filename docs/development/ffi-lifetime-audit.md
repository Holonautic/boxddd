# FFI Lifetime Audit

This document tracks ownership and lifetime decisions for the safe `boxddd`
wrapper over the vendored Box3D C API.

## Current Verdict

The audited `0.3.0` architecture has the required ownership, provenance, and
failure boundaries for the safe wrapper:

- `WorldDef`, `BodyDef`, `ShapeDef`, and joint definitions are owned Rust values.
  Pointer-bearing native definitions are created only by private lowering after
  validation and while the Box3D lock is held.
- Public body, shape, joint, contact, tree-proxy, and replay identifiers are
  opaque provenance handles. Owner plus resource or contact-epoch tokens are
  checked before an ID is converted to native bits or passed to FFI, so foreign
  and stale handles return typed errors without probing the wrong native owner.
- `WorldLedger` stages allocation with pending resources and publishes IDs only
  after native creation succeeds. Destructive mutations prepare cascades or
  contact epochs before FFI and apply their `finish_*` finalizers only after the
  native mutation, keeping the Rust graph and Box3D state in the same order.
- Public recoverable operations use one unprefixed Result-first API. Validation,
  provenance, callback-state, allocation, and native failures remain explicit.
- `World`, `Recording`, `RecPlayer`, `DynamicTree`, and native geometry owners
  are intentionally `!Send` and `!Sync` through `PhantomData<Rc<()>>`.
- Box3D calls are serialized through the crate-global Box3D lock, and safe APIs
  reject calls made from Box3D callbacks through the callback-state guard.
- Local and shared callback execution state contains panics and carries the first
  failure back across the FFI return boundary. Tokenized pending/registered
  callback state makes replacement, snapshots, and retirement explicit; the task
  adapter uses the same shared failure protocol plus Box3D's blocking
  `finishTask` contract.
- Body/shape ledger sidecars keep `MeshData`, `HeightField`, and `Compound` alive
  while Box3D shapes refer to them.
- `RecordingSession` borrows both `World` and `Recording`, while a shared
  attachment marker lets either owner stop the native recording even if the
  session is deliberately forgotten.

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
- `Recording::save_to_file` copies bytes while the Box3D lock is held. The active
  session's mutable borrow prevents safe callers from saving concurrently. The
  upstream native save helper remains raw-only because the safe API already
  writes validated bytes through Rust IO.
- `RecordingSession::finish` stops and finalizes the stream; dropping an active
  session performs the same native stop before releasing either mutable borrow.
- `World` and `Recording` share a stable attachment marker. `mem::forget` can
  skip the session destructor but cannot make the native pointer outlive the
  recording: either owner detaches first, and byte access returns
  `Error::RecordingInUse` until detachment.

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

- `Compound::into_bytes` uses `ManuallyDrop` so the native allocation is not
  freed during transfer to `CompoundBytes`.
- `CompoundBytes::drop` calls `b3DestroyCompound` on the same allocation, which
  matches the upstream representation.
- `CompoundBytes::into_compound` forgets `self` only after successful conversion,
  transferring ownership back to `Compound`.

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
- Callback replacement is performed while the Box3D global lock is held.
- `World::drop` clears raw callbacks before destroying the native world.
- Material-mix callbacks use registry slots and release them when both callbacks
  are cleared.
- Local callback state is used for synchronous visitor-style calls, while shared
  callback state records the first failure across world, material, and task
  callbacks and reports it after the native operation returns.
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

- The task-system context is an `Arc` kept alive by `World`.
- Worker threads temporarily clone the `Arc` from the raw pointer before running.
- Panics in enqueue, task execution, or finish are contained and reported after
  `World::step`.

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
  reuse, discarded staging, cascaded destruction, and contact epochs.
- Event borrowed views are exposed only through `with_*_events_view` closures and
  body, sensor, contact, and joint views have rustdoc `compile_fail` examples
  showing they cannot escape.
- Recording tests cover exclusive `RecordingSession` borrowing, explicit finish,
  drop finalization, and both owner-drop orders after a deliberately forgotten
  session.
- Callback registration signatures and `World: !Send + !Sync` prevent moving a
  live `World` into Box3D callbacks in safe Rust.
- World callback tests cover panic containment and non-finite material-mix
  fallback behavior.
- Debug draw tests cover panic containment, callback-guard rejection, invalid
  bounds rejection, and shape callback visitation.
- Shape and compound borrowed views are tied to `&World` or `&Compound` and now
  document that replacing or destroying the owner invalidates the native view.
