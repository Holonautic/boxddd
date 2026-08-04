# CI And Maintainer Checks

This document keeps the detailed build matrix out of the top-level README while
preserving the commands maintainers need when changing the binding, CI, platform
support, or vendored Box3D build.

## Target Support

| Target | CI coverage | Notes |
|---|---|---|
| `x86_64` Windows MSVC | tests | Primary Windows target. Vendored Box3D C sources are compiled by `boxddd-sys`. |
| `x86_64` Linux GNU | tests | Primary Linux target. CI installs the native windowing/audio packages needed by Bevy examples. |
| `aarch64` macOS | tests | Primary Apple desktop target on GitHub-hosted macOS runners. |
| `x86_64-pc-windows-gnu` | link check | CI builds `boxddd-sys` tests with MSYS2/MinGW to catch GNU linker regressions. |
| `armv7-unknown-linux-gnueabihf` | compile-only | FFI signedness sentinel for pregenerated bindings. Native C linking is skipped. |
| `aarch64-apple-ios` | compile-only | Rust wrapper and pregenerated bindings are type-checked. Native C linking is skipped. |
| `aarch64-apple-ios-sim` | compile-only | Simulator compile sentinel. Native C linking is skipped. |
| `aarch64-linux-android` | compile-only | Android compile sentinel. Native C linking is skipped. |
| `wasm32-unknown-unknown` | compile-only + provider smoke + Pages browser smoke | Browser-oriented target. Default checks skip Box3D C; default-precision provider mode imports Box3D from the manifest-driven module `box3d-sys-v2`; CI validates its ABI revision sentinel, runs shared-memory and Chromium runtime smokes, and publishes direct Bevy + egui Web examples. Provider plus `double-precision` is rejected. |
| `wasm32-wasip1` | runtime smoke | CI builds vendored Box3D C with WASI SDK in default and double precision, then runs `boxddd/examples/wasm_smoke.rs` under wasmtime in both modes. |

See [`../platforms/wasm.md`](../platforms/wasm.md) for the exact WASM matrix.

## CI Coverage

The GitHub Actions workflow is shaped like a native binding crate gate rather
than a single workspace smoke test:

- format check on stable Rust
- actionlint workflow validation using `rhysd/actionlint` v1.7.12
- native `cargo nextest run --workspace` on Windows, Linux, and macOS
- Linux `cargo nextest run --workspace --features "boxddd/double-precision boxddd-sys/double-precision"`
  plus double-precision `boxddd-sys` ABI checks and layout tests
- `cargo test --workspace --doc` for workspace doctests
- representative headless core examples execute their validation, owner-scoped handle, callback recovery, recording/replay, and dedicated-thread integration paths
- Bevy example compile checks, including debug draw, picking, and the 3D testbed
- explicit headless Bevy testbed scene validation through `bevy_boxddd/tests/testbed.rs`
- GitHub Pages generation and static validation plus a Playwright Chromium smoke
  that launches the direct Bevy Web example, verifies provider/scene readiness,
  rejects browser errors and unexpected failed requests, and observes changing
  canvas frames
- read-only upstream conformance against the manifest's exact Box3D commit,
  declared patch set, generated bindings, and artifact fingerprints
- official Box3D sample parity validation that keeps
  `docs/upstream-parity/box3d-sample-matrix.md` synchronized with the compact
  generated JSON inventory for the exact upstream commit
- docs.rs paths for `boxddd-sys`, `boxddd`, and `bevy_boxddd`
- no-default-feature checks, optional math interop `nextest` checks, and direct math interop example runs
- package checks for all publishable crates
- forced bindgen refresh checks for single and double precision
- default `boxddd-sys` dependency checks proving `bindgen` and `clang-sys` are not required for normal users
- Windows GNU, armv7, mobile, and WASM compile/link sentinels
- C-backed `wasm32-wasip1` runtime smokes with WASI SDK and wasmtime in
  default and double precision
- browser-style provider smoke with Emscripten, shared `WebAssembly.Memory`, and Node

The workflow uses current Node-runtime action majors where they are available.
For example, repository checkout uses `actions/checkout@v7` to avoid the Node 20
deprecation warning emitted by older checkout releases. Dependency caching pins
`Swatinem/rust-cache@v2.9.1` rather than the floating `v2` tag so release
preflight uses the currently published v2 action.

## Local Workspace Checks

```bash
cargo fmt --all --check
cargo build --workspace
cargo nextest run --workspace
cargo test --workspace --doc
cargo nextest run --workspace --features "boxddd/double-precision boxddd-sys/double-precision"
cargo check --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p boxddd --example hello_world
cargo run -p boxddd --example error_handling
cargo run -p boxddd --example callbacks_and_step_outcomes
cargo run -p boxddd --example recording_replay
cargo run -p boxddd --example physics_thread
cargo nextest run -p boxddd --features "mint glam nalgebra serde" --test interop
cargo check -p bevy_boxddd --examples
cargo check -p bevy_boxddd --features "debug-gizmos physics-picking" --example testbed_3d
cargo nextest run -p bevy_boxddd --test testbed
cargo run -p xtask -- build-pages-wasm
cargo run -p xtask -- validate-pages
npm --prefix tools/pages-smoke ci
npm --prefix tools/pages-smoke run install:chromium
npm --prefix tools/pages-smoke test
cargo run -p xtask -- sample-parity --check
python tools/update_box3d_and_bindings.py check --source repo-ref/box3d --mode both
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

The conformance command requires a clone whose `origin` matches the repository
declared in `boxddd-sys/box3d-upstream.toml` and which contains the exact commit
object. Its current branch and working-tree contents are irrelevant because the
tool materializes declared files directly from that commit object. See
[`upstream-conformance.md`](upstream-conformance.md) for prerequisites and the
upgrade workflow.

## Xtask Scope

`xtask` is limited to repository task orchestration, deterministic Pages
generation, narrow artifact validation, and launching real provider smoke
tests. It consumes the dependency-free Testbed scene catalog directly and uses
the host `WebAssembly` API to inspect imports. It must not parse Rust syntax,
infer call graphs or receiver types, or claim to prove FFI safety. Rust safety
evidence belongs in capability types, compile-fail contracts, and focused
behavior tests; JavaScript runtime behavior belongs in the checked-in Node or
browser smoke harnesses.

## Official Sample Parity

The official sample parity matrix is case-level: every registration in
`docs/upstream-parity/box3d-sample-inventory.json` has one row with its source
location, parity mode, target artifact, and rationale. The inventory is compact,
sorted JSON generated from `RegisterSample` and `RegisterReplay` calls in the
manifest's exact upstream commit. The pruned vendored runtime tree intentionally
contains no upstream sample host or sample sources.

Run this gate after synchronizing a new upstream commit or changing example
coverage:

```bash
cargo run -p xtask -- sample-parity --check
```

The check reads the committed JSON inventory, verifies that its commit matches
the generated upstream contract, validates sorted and unique entries, and then
compares it with `docs/upstream-parity/box3d-sample-matrix.md`. It does not need
an upstream checkout and does not scan the vendored runtime tree. The full
upstream conformance check separately regenerates the inventory from the exact
commit object and byte-compares it with the committed JSON.

## Bevy Testbed Validation

The maintained visual teaching surface is the native Bevy testbed:

```bash
cargo run -p bevy_boxddd --features "debug-gizmos physics-picking" --example testbed_3d
```

CI does not require a GPU window or screenshots. The required headless gate is:

```bash
cargo nextest run -p bevy_boxddd --test testbed
```

That test constructs every registry scene with `TimePlugin`, steps physics, and
checks body, shape, joint, query, interaction, and lifecycle invariants without
creating a renderer window. Screenshots remain local documentation artifacts
until there is a renderer path stable enough for unattended CI.

## Pages Static Site

The GitHub Pages site under `docs/pages` is the public example hub. Its root
page is the Bevy example index, and each card opens a direct Bevy + egui Web
example backed by the shared `bevy_boxddd/examples/testbed_3d` wasm bundle.
The generated pages and the Testbed compile the same dependency-free catalog in
`bevy_boxddd/examples/testbed_3d/scene_catalog.rs`.

```bash
cargo run -p xtask -- generate-pages
cargo run -p xtask -- build-pages-wasm
cargo run -p xtask -- validate-pages
npm --prefix tools/pages-smoke ci
npm --prefix tools/pages-smoke run install:chromium
npm --prefix tools/pages-smoke test
```

Set `BOXDDD_PAGES_SMOKE_PORT` when port `4173` is already occupied locally.

The Pages workflow generates `docs/pages/examples/<scene>/index.html`, builds
browser WASM assets into `docs/pages/wasm/generated` and
`docs/pages/bevy-testbed/generated`, validates the site, and runs the real
Falling Stack route in headless Chromium before uploading only `docs/pages`.
Pull requests run the same build and browser smoke but never configure, upload,
or deploy GitHub Pages. Release and crates.io publishing workflows remain
separate.

## Rustdoc Coverage

The public rustdoc gate should stay complete for both the core binding and the
Bevy integration layer:

```bash
cargo rustdoc -p boxddd --all-features -- -D missing_docs
cargo rustdoc -p bevy_boxddd --all-features -- -D missing_docs
```

`RUSTDOCFLAGS="-D warnings" cargo doc -p boxddd --all-features --no-deps`
should also pass before release. See `rustdoc-alignment.md` for the semantic
alignment status by module and `ffi-lifetime-audit.md` for ownership and
lifetime decisions that should be reflected in rustdoc.

## Binding Checks

Default builds use vendored Box3D C sources and pregenerated bindings, so normal
builds do not require LLVM or libclang.

The authoritative maintenance path is manifest-driven:

```bash
python tools/update_box3d_and_bindings.py generate --mode both
python tools/update_box3d_and_bindings.py check --source repo-ref/box3d --mode both
```

`generate` updates both checked-in binding modes, the capability inventory, and
their manifest fingerprints. `check` reconstructs the declared vendor subset in
a temporary directory, applies only fingerprinted patches, regenerates both
binding modes and inventories, and compares every declared artifact without
rewriting checked-in files. It still invokes Cargo and bindgen, so normal
`target` build output may be created.

The provider contract is manifest-owned: `box3d-upstream.toml` declares the v2
module name (`box3d-sys-v2`), bridge revision, supported precision, required
capabilities, and intentionally unsupported capabilities. The generated
`docs/upstream-parity/box3d-capability-inventory.json` is the reviewed
per-mode classification behind that declaration. Package audit verifies that
the published sys crate retains the provider manifest, v2 capability declarations,
and generated Rust provider contract; the repository conformance gate verifies
the generated capability inventory itself.

Useful CI-equivalent binding checks:

```bash
BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys
cargo check -p boxddd-sys --features double-precision
cargo nextest run -p boxddd-sys --features double-precision --test layout
BOXDDD_SYS_FORCE_BINDGEN=1 cargo check -p boxddd-sys --features bindgen
BOXDDD_SYS_FORCE_BINDGEN=1 cargo check -p boxddd-sys --features "bindgen double-precision"
```

For the `0.4.0` release line, release preflight runs the semver audit in
minor-release mode against `v0.3.0` so `0.x` breaking changes are enumerated
instead of skipped by the default semver policy. The release profile owns the
workspace version, baseline, release type, published crates, and exact
lint-plus-API evidence inventory:

```bash
python3 tools/semver_audit.py --release-version 0.4.0
```

The reviewed inventory lives in `tools/semver/0.4.0.tsv`. Most rows name the
release crate, exact cargo-semver-checks lint, normalized API evidence, and one
documented migration-reason category. The reserved `manual_reviewed_break`
value records reviewed public breaks that cargo-semver-checks cannot observe,
such as arbitrary return-type changes or removal of a hand-written trait impl.
Those rows are data only: the audit validates and reports them but does not
parse Rust source. Keep the inventory sorted and update it only after a real
audit proves the corresponding public API change.

Pull-request CI, manual release preflight, and tag publish preflight all run the
maintenance-tool regression suite before relying on the semver parser or
synchronization contract:

```bash
python3 -m unittest discover -s tools/tests
```

Unknown release versions, workspace or crate version drift, unexpected or
missing break evidence, and tool failures without parseable semver diagnostics
all fail closed. The workflow explicitly runs `--validate-only` before
installing `cargo-semver-checks`, then runs the same profile for the real audit.
The semver tool validates release data and Cargo artifacts; it does not parse or
duplicate the GitHub Actions workflow structure.

When checking release packaging locally, use the same temporary registry patch
configuration as CI so the unpublished dependency chain can be verified before
anything is on crates.io.

```bash
cargo package -p boxddd-sys --locked
cargo package -p boxddd --locked --config 'patch.crates-io.boxddd-sys.path="boxddd-sys"'
cargo package -p bevy_boxddd --locked --config 'patch.crates-io.boxddd.path="boxddd"' --config 'patch.crates-io.boxddd-sys.path="boxddd-sys"'
```

Audit the generated archives before publishing:

The official sample parity matrix is a repository-level release artifact rather
than a copied crate-package file. Crate README files link to the GitHub-hosted
matrix, while `cargo run -p xtask -- sample-parity --check` remains the
authoritative synchronization gate.

```bash
version="$(cargo pkgid -p boxddd | sed -E 's/.*[@#]//')"
sys_crate="target/package/boxddd-sys-${version}.crate"
core_crate="target/package/boxddd-${version}.crate"
bevy_crate="target/package/bevy_boxddd-${version}.crate"
python3 tools/audit_crate_packages.py \
  --sys "$sys_crate" \
  --core "$core_crate" \
  --bevy "$bevy_crate"
```

## Release Workflows

Release automation is split into validation, GitHub Release, and crates.io
publishing workflows.

- `Release Preflight`: manual `workflow_dispatch` check for a version and source
  ref. It verifies the workspace version, changelog release notes, formatting,
  the exact release-profile semver inventory, package archives, and the
  `boxddd-sys` crates.io dry-run. The active `0.4.0` profile is bound to the
  `v0.3.0` baseline with a minor release type.
- `GitHub Release`: runs on pushed `v*` tags or manual dispatch. It verifies the
  workspace version, extracts the matching `CHANGELOG.md` section with
  `tools/changelog.py`, rejects manually wrapped changelog prose, and creates or
  updates the GitHub Release notes.
- `Release Crates (crates.io)`: runs on pushed `v*` tags or manual dispatch.
  It verifies the tag matches the workspace version and requires a successful
  `CI` push run for the exact tag commit on the repository default branch. This
  binds publishing to the native, feature, provider, WASI, documentation, lint,
  semver, and package matrix without duplicating those jobs in the release
  workflow. The preflight job passes that immutable commit SHA to the publish
  job, which checks out the SHA and rejects a release tag that moved after
  preflight. It then verifies changelog release notes and publishes
  `boxddd-sys`, `boxddd`, and `bevy_boxddd` in dependency order.

The publish workflow expects a repository or environment secret named
`CARGO_REGISTRY_TOKEN` and uses the protected `crates.io` environment. Downstream
crate dry-runs happen after their dependency crate is visible on crates.io,
because `cargo publish --dry-run` resolves registry dependencies rather than
workspace path dependencies.

Release automation does not create git tags. Create and push the tag after the
workspace version and changelog section are ready:

```bash
version=0.4.0
git tag -a "v${version}" -m "v${version}"
git push origin "v${version}"
```

## Cross-Target Compile-Only Checks

```bash
rustup target add armv7-unknown-linux-gnueabihf wasm32-unknown-unknown aarch64-linux-android
BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys -p boxddd --target armv7-unknown-linux-gnueabihf
BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys -p boxddd --target aarch64-linux-android
cargo check -p boxddd --target wasm32-unknown-unknown
BOXDDD_SYS_WASM_MODE=provider cargo check -p boxddd --target wasm32-unknown-unknown
cargo check -p bevy_boxddd --target wasm32-unknown-unknown --no-default-features
```

On macOS, CI also runs:

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys -p boxddd --target aarch64-apple-ios
BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys -p boxddd --target aarch64-apple-ios-sim
```

## WASM Runtime Smokes

C-backed WASI runtime smoke:

```bash
rustup target add wasm32-wasip1
export WASI_SDK_PATH=/path/to/wasi-sdk-33.0-x86_64-linux
export WASI_SYSROOT="$WASI_SDK_PATH/share/wasi-sysroot"
export CC_wasm32_wasip1="$WASI_SDK_PATH/bin/clang"
cargo build -p boxddd --example wasm_smoke --target wasm32-wasip1
wasmtime target/wasm32-wasip1/debug/examples/wasm_smoke.wasm
cargo build -p boxddd --example wasm_smoke --target wasm32-wasip1 --features double-precision
wasmtime target/wasm32-wasip1/debug/examples/wasm_smoke.wasm
```

Browser-style provider smoke:

```bash
rustup target add wasm32-unknown-unknown
cargo run -p xtask -- provider-smoke-app

# Full provider smoke also requires Emscripten SDK on PATH or EMSDK set.
cargo run -p xtask -- provider-smoke

# Pages Bevy Web build also requires wasm-bindgen-cli matching Cargo.lock.
cargo install wasm-bindgen-cli --version 0.2.126 --locked
cargo run -p xtask -- build-pages-wasm
```

The provider smoke and generated Pages loader both require the manifest's
default-precision `box3d-sys-v2` module and validate
`boxddd_provider_abi_revision` before instantiating the Rust application. A
provider build with `double-precision` is intentionally unsupported and fails
during build configuration.

The full smoke also verifies exact contact/sensor destruction-event and replay
scale metrics, rejects malformed replay before accepting a valid replay, drains
World-destruction callback errors, tears down every World and player, and then
instantiates the same compiled module a second time in the same Node process.
Both cycles must produce identical metrics without inherited callback tokens,
error-table entries, or ambient length scale.
