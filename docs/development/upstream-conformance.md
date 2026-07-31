# Upstream Conformance

`boxddd-sys/box3d-upstream.toml` is the single provenance contract for the
vendored Box3D runtime and every ABI artifact derived from it. The contract pins
the upstream repository and full commit SHA, declares the minimal vendored path
set and local patches, names generated artifacts, defines the browser provider
ABI, and fingerprints the checked-in result.

Normal crate users build the checked-in vendor tree and pregenerated bindings.
They do not need an upstream checkout, Python, LLVM, libclang, or bindgen. This
document is for maintainers changing Box3D, bindings, provider assets, or parity
data.

Conformance automation proves provenance, reproducibility, ABI-mode agreement,
declared patch application, and provider capability classification. It does not
prove the semantic safety of the high-level wrapper. The `boxddd` Foundation and
Result-first contract is established separately by initialization, activity,
definition-lowering, creation-transaction, handle-provenance, ledger, lifetime,
callback, task, recording-session, and typed-error tests.

## Contract Contents

| Manifest section | Contract |
|---|---|
| `upstream` | Canonical repository URL and an exact 40-character commit SHA. |
| `vendor` | Destination root and a sorted allowlist of runtime, header, README, and license paths. |
| `patches` | Reviewable patch paths, SHA-256 digests, targets, and rationales. |
| `provider` | Import module, asset basename, ABI bridge revision, supported precision modes, and required or intentionally unsupported capabilities. |
| `bindings` | Generated Rust contract and the default- and double-precision pregenerated binding outputs. |
| `samples` | Compact official-sample inventory generated from the exact commit. |
| `capabilities` | Per-mode raw-function and provider capability classification. |
| `fingerprints` | SHA-256 identities for the vendor tree, headers, patches, bindings, inventories, and generated Rust contract. |

The generated `boxddd-sys/src/upstream_contract.rs` carries the manifest values
needed during Rust builds and by `xtask`. It is generated data and must not be
edited directly.

## Exact Commit Materialization

The synchronization tool accepts a local Git clone through `--source`. It first
normalizes and verifies the clone's `origin` URL against the manifest, then
resolves `<commit>^{commit}` and requires the result to equal the declared full
SHA.

The source clone does not need to have that commit checked out, and it may have
uncommitted changes. Declared files and official sample registrations are read
from the commit object with Git object commands, not from working-tree paths.
Dirty or branch-local source bytes therefore cannot enter the vendored tree or
sample inventory.

For the repository's conventional local clone:

```bash
manifest_commit=EXACT_40_CHARACTER_SHA_FROM_BOX3D_UPSTREAM_TOML
git -C repo-ref/box3d remote get-url origin
git -C repo-ref/box3d fetch origin "$manifest_commit"
git -C repo-ref/box3d cat-file -e "${manifest_commit}^{commit}"
```

Substitute the exact `upstream.commit` value from the manifest. The manifest
remains authoritative; do not copy a branch name or the clone's current `HEAD`
into automation or duplicate a commit pin in prose.

## Prerequisites

- Python 3.11 or newer, including the standard-library `tomllib` module.
- Git, including the exact upstream commit object in the source clone.
- Rust and Cargo for binding regeneration.
- Clang and libclang discoverable by bindgen for `generate` and full `check`.
- The native tools required by focused follow-up gates, such as Emscripten and
  Node for `provider-smoke`.

`sync` itself does not invoke bindgen. `generate` and `check` do.

## Commands

Run all commands from the workspace root.

### Synchronize Sources

```bash
python tools/update_box3d_and_bindings.py sync --source repo-ref/box3d
```

`sync` performs these tracked writes:

1. Materializes only `vendor.paths` from the exact commit object into a
   temporary tree.
2. Verifies every declared patch file's SHA-256 digest and applies only those
   patches to the temporary tree.
3. Atomically replaces `boxddd-sys/third-party/box3d` with that expected tree.
4. Regenerates `docs/upstream-parity/box3d-sample-inventory.json` from upstream
   `RegisterSample` and `RegisterReplay` registrations.
5. Regenerates `boxddd-sys/src/upstream_contract.rs`.
6. Updates the affected manifest fingerprints.

Manual edits under the vendor root are not an accepted maintenance path. Express
an intentional deviation as a declared patch, or it will fail conformance.

### Generate Bindings

```bash
python tools/update_box3d_and_bindings.py generate --mode both
```

`generate` first verifies the synchronized vendor, header, patch, and generated
Rust-contract fingerprints. It then runs forced bindgen for the default and
double-precision modes, updates both pregenerated binding files, regenerates the
capability inventory, and updates their manifest fingerprints.

Use `--mode default` or `--mode double` only for a focused diagnostic refresh.
Use `--mode both` for upstream upgrades and before review. `--profile debug` is
the default; `--profile release` selects Cargo's release profile for bindgen
generation.

### Check Reproducibility

```bash
python tools/update_box3d_and_bindings.py check --source repo-ref/box3d --mode both
```

`check` is read-only with respect to checked-in source and generated artifacts.
It reconstructs the vendor tree in a temporary directory, applies the declared
patch set, regenerates the sample inventory, Rust contract, both binding modes,
and capability inventory, then byte-compares those results and verifies every
manifest fingerprint. It never refreshes a stale tracked artifact for the
caller.

Read-only does not mean that no filesystem output is produced: binding
comparison invokes Cargo and bindgen, so ordinary ignored `target` output and
temporary directories may be created. A successful check means the tracked
workspace needed no rewrite.

## Declarative Patches

Each local Box3D deviation belongs in `boxddd-sys/patches` and in one
`[[patches]]` manifest entry. The entry records:

- `path`: repository-relative patch file;
- `sha256`: exact patch-byte identity;
- `target`: intended upstream file for review;
- `rationale`: why the deviation is required.

The aggregate patch fingerprint also covers the declared patch paths and bytes.
Both `sync` and `check` verify the individual digest before applying a patch with
`git apply`. A changed, missing, or non-applicable patch is a hard failure.

When a patch changes, rebase its diff against the new exact upstream commit,
review the resulting behavior, update its manifest digest and metadata, and run
`sync`. Do not edit the patched vendor file and then bless the resulting vendor
fingerprint; that loses the upstream-to-local provenance chain.

## Sample Parity Inventory

The vendored runtime intentionally excludes Box3D's sample host, renderer,
assets, and `samples/sample_*.cpp` sources. Sample parity therefore does not scan
the vendor tree.

`sync` reads all matching sample source files from the exact upstream commit
object and writes a compact, sorted JSON inventory containing only:

- `schema_version`;
- `upstream_commit`;
- each registration's `category`, `name`, and `source` location.

The repository-level parity gate consumes that committed inventory:

```bash
cargo run -p xtask -- sample-parity --check
```

It checks the inventory schema, commit identity, non-empty sorted unique entries,
and exact correspondence with
`docs/upstream-parity/box3d-sample-matrix.md`. This gate needs neither the
upstream clone nor the removed sample sources. The full conformance `check`
remains responsible for regenerating the JSON from the exact commit object and
proving that the compact inventory is authoritative.

## Browser Provider ABI

The provider contract currently declares:

- import module and asset basename `box3d-sys-v2`;
- bridge revision `2`;
- default precision as the only supported provider precision.

Rust provider imports, provider export discovery, smoke assets, Node runners,
and generated Pages loaders obtain those values through the manifest-generated
contract. Runtime loaders call the provider export
`boxddd_provider_abi_revision` before instantiating the Rust application and
require it to equal the declared bridge revision.

One provider instance and imported memory support one active Rust consumer at a
time. Generated loaders acquire an identity-checked consumer lease, reject a
second live consumer, and permit sequential replacement only after release.
This host boundary complements Foundation activity inside each Rust instance; it
does not claim simultaneous coordination across independent WASM consumers.

The versioned module name prevents a pre-v2 provider from satisfying v2 imports.
The ABI revision sentinel adds a second check for a stale or mislabelled provider
published under the current filename. Both checks are required; changing only a
filename is not an ABI validation strategy.

Provider mode is single precision only. The build script explicitly rejects
`BOXDDD_SYS_WASM_MODE=provider` together with `double-precision`, preventing a
Rust double-precision ABI from being paired with the single-precision C
provider. Native and C-backed WASI source builds continue to support both
precision modes. Supporting provider double precision requires a distinct
manifest declaration, provider artifact and module contract, and dedicated
runtime smoke coverage.

## Upstream Upgrade Procedure

1. Select and review one exact upstream commit. Update `upstream.repository` and
   the full `upstream.commit` SHA; never pin a mutable tag or branch.
2. Fetch that commit object into the source clone and verify its repository URL
   and object identity.
3. Reconcile the sorted `vendor.paths` allowlist with the runtime and license
   files required by the new commit. Do not restore sample hosts, benchmarks,
   generated documentation, or renderer dependencies without an explicit
   runtime need.
4. Rebase every local patch, update its SHA-256 digest, target, and rationale,
   and delete patches that are no longer necessary.
5. Review upstream ABI changes. When the provider ABI changes, increment the
   `box3d-sys-vN` module and asset namespace and the bridge revision together;
   review required and intentionally unsupported provider capabilities.
6. Run `sync --source <clone>` and inspect the vendor, patch, generated Rust
   contract, and sample-inventory diff.
7. Run `generate --mode both` and inspect both binding files and the capability
   inventory. Adapt Rust FFI, safe wrappers, provider bridges, and layout/default
   expectations to the new headers.
8. Reconcile `box3d-sample-matrix.md` with the generated JSON inventory. Migrate
   every provider consumer and regenerate browser assets when the provider
   contract changed.
9. Run the read-only `check --source <clone> --mode both`, sample parity, layout
   tests, forced-bindgen checks, and provider smoke. A gate that cannot run must
   be reported with its exact missing prerequisite; it is not a pass.
10. Review package contents so the manifest, all declared patches, provider
    helper, both pregenerated bindings, vendor license, and minimal runtime tree
    are present, while upstream clones and removed sample sources are absent.

The core verification sequence is:

```bash
python tools/update_box3d_and_bindings.py check --source repo-ref/box3d --mode both
cargo run -p xtask -- sample-parity --check
cargo nextest run -p boxddd-sys --test layout
BOXDDD_SYS_FORCE_BINDGEN=1 BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys --features bindgen
BOXDDD_SYS_FORCE_BINDGEN=1 BOXDDD_SYS_SKIP_CC=1 cargo check -p boxddd-sys --no-default-features --features "bindgen,double-precision"
cargo run -p xtask -- provider-smoke
```

## CI Materialization

CI must provision a temporary source clone whose `origin` matches the manifest
repository and fetch the exact declared commit object before invoking `check`.
It must pass that clone explicitly through `--source`; it must not depend on a
developer-only `repo-ref` directory, mutable upstream `HEAD`, or bytes copied
from a dirty checkout.

The repository tool provides the narrow materialization command used by CI:

```bash
python tools/update_box3d_and_bindings.py checkout --destination /tmp/box3d-upstream
python tools/update_box3d_and_bindings.py check --source /tmp/box3d-upstream --mode both
```

The lightweight `xtask sample-parity --check` gate is intentionally independent
of that clone because it consumes the committed JSON inventory. Package and
release gates likewise use checked-in manifest artifacts; the full conformance
job proves beforehand that those artifacts derive from the exact source object.

## Diagnosing Failures

- **Repository mismatch:** the source clone's `origin` is not the manifest
  repository. Use the correct clone; do not weaken the identity check.
- **Commit mismatch or missing object:** fetch the exact SHA into the source
  clone. Checking out a similarly named branch is insufficient.
- **Vendor mismatch:** inspect reported missing, extra, and changed paths. Update
  the allowlist or patch declaration instead of editing the vendor tree.
- **Patch fingerprint or apply failure:** review and rebase the patch against the
  pinned commit, then update its declared digest.
- **Stale sample inventory or Rust contract:** run `sync` and review the generated
  diff.
- **Stale bindings or capability inventory:** install the bindgen prerequisites,
  run `generate --mode both`, and review ABI changes.
- **Fingerprint mismatch:** regenerate through the owning command. Do not replace
  a digest merely to silence conformance.
- **Provider revision mismatch:** rebuild and republish the provider and loader
  from the same manifest contract; never alias an old provider into the current
  module namespace.
