# boxddd-sys

Low-level Rust FFI for the vendored [Box3D](https://github.com/erincatto/box3d) C API.

Most users should depend on [`boxddd`](https://crates.io/crates/boxddd) instead. Use `boxddd-sys` directly when you need raw C symbols, custom native linking, or binding-generation maintenance.

Direct FFI use bypasses `boxddd`'s owned-definition lowering, opaque handle
provenance, owner-before-FFI authorization, world ledger and transaction
finalizers, lifetime guards, callback/task protocols, and Result-first typed
errors. A caller that chooses this layer owns all native aliasing,
synchronization, callback, and destruction invariants and must not mutate native
objects that are simultaneously owned by a live safe wrapper.

## Build Contract

Default builds compile the vendored Box3D C sources with the Rust `cc` crate and link the resulting static library into `boxddd-sys`.

Normal users need a platform C compiler, such as MSVC Build Tools on Windows, Clang on macOS, or GCC/Clang on Linux. Normal users do not need CMake, LLVM, libclang, or bindgen because checked-in pregenerated bindings are used by default.

The vendored source, exact upstream commit, declarative local patches, generated
bindings, browser provider v2 ABI, and artifact fingerprints are declared in
`box3d-upstream.toml`. Build and provider tooling consume the generated Rust
contract derived from that manifest rather than maintaining independent source,
patch, or ABI version strings.

## Features

- `build-from-source`: compile vendored Box3D C sources. Enabled by default.
- `bindgen`: allow regenerating bindings when `BOXDDD_SYS_FORCE_BINDGEN=1` is set.
- `double-precision`: build Box3D with `BOX3D_DOUBLE_PRECISION` and use matching pregenerated bindings.
- `disable-simd`: define `BOX3D_DISABLE_SIMD`.
- `validate`: define `BOX3D_VALIDATE`.

## Native Linking

Disable default features to skip vendored C compilation and link an external `box3d` library:

```toml
boxddd-sys = { version = "0.3", default-features = false }
```

Optional environment variables:

- `BOXDDD_SYS_LINK_LIB`: external library name. Defaults to `box3d`.
- `BOXDDD_SYS_LINK_SEARCH`: native library search directory.

## Upstream And Binding Maintenance

Pregenerated bindings are ABI-mode specific. The default build uses `bindings_pregenerated.rs`; `double-precision` uses `bindings_pregenerated_double.rs`.

Regenerate both checked-in binding modes only when maintaining this crate:

```bash
python tools/update_box3d_and_bindings.py generate --mode both
```

Given a local clone that contains the manifest's exact upstream commit object,
verify the vendored subset, declared patches, generated sample and capability
inventories, Rust contract, and both binding modes without rewriting checked-in
artifacts:

```bash
python tools/update_box3d_and_bindings.py check --source repo-ref/box3d --mode both
```

This check proves source and patch provenance, generated-artifact
reproducibility, ABI-mode agreement, and provider capability classification. It
does not prove the semantic safety of APIs built above the raw bindings; that is
the responsibility of `boxddd`'s ownership, provenance, lifetime, callback, and
typed-error tests.

The direct forced-bindgen checks remain useful diagnostics:

```bash
BOXDDD_SYS_FORCE_BINDGEN=1 cargo check -p boxddd-sys --features bindgen
BOXDDD_SYS_FORCE_BINDGEN=1 cargo check -p boxddd-sys --features "bindgen double-precision"
```

See [Upstream Conformance](https://github.com/Latias94/boxddd/blob/main/docs/development/upstream-conformance.md)
for the source synchronization contract, read-only check semantics, patch
workflow, and upgrade procedure.

## WASM

WASM support uses a manifest-driven provider v2 contract with an explicitly
classified capability subset.

| Target | Status |
|---|---|
| `wasm32-unknown-unknown` | Compile-only by default. Provider mode imports Box3D symbols from the manifest-driven module `box3d-sys-v2`. |
| `wasm32-wasip1` | Runtime-capable source build when a WASI SDK sysroot is configured. |
| Browser visual demos | Not a `boxddd-sys` public API contract. The workspace builds Bevy Web examples through provider mode for the demo hub. |

The v2 provider exports `boxddd_provider_abi_revision`; Node and browser loaders
must match that sentinel to the manifest's bridge revision before instantiating
the Rust application. This catches a stale provider even if it is accidentally
published under the current asset name.

Provider mode currently supports only the default single-precision ABI.
Combining `BOXDDD_SYS_WASM_MODE=provider` with `double-precision` is rejected at
build time. Native and WASI source builds continue to support the
`double-precision` feature.

Useful environment variables:

- `BOXDDD_SYS_WASM_MODE`: `compile-only`, `source`, or `provider`.
- `WASI_SYSROOT`: WASI libc sysroot for `wasm32-wasip1` source builds.
- `WASI_SDK_PATH`: WASI SDK root. Used as `$WASI_SDK_PATH/share/wasi-sysroot` when `WASI_SYSROOT` is unset.

Detailed WASM commands live in the workspace documentation: <https://github.com/Latias94/boxddd/blob/main/docs/platforms/wasm.md>.

## Check-Only Builds

`BOXDDD_SYS_SKIP_CC=1` skips native C compilation for check-only workflows. Do not use it for normal runnable native builds.

```bash
BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys
```

## License

This crate is licensed as MIT OR Apache-2.0. Vendored Box3D is MIT-licensed.
