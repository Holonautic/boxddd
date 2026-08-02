#!/usr/bin/env python3
"""Synchronize Box3D sources and verify generated binding artifacts."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable, Mapping, NamedTuple, Sequence


MANIFEST_RELATIVE_PATH = Path("boxddd-sys/box3d-upstream.toml")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")
CPP_REGISTRATION_PATTERN = re.compile(
    r'Register(?:Sample|Replay)\(\s*"([^"]+)"\s*,\s*"([^"]+)"'
)
RUST_FUNCTION_PATTERN = re.compile(r"(?m)^\s*pub fn ([A-Za-z_][A-Za-z0-9_]*)\s*\(")


class ContractError(RuntimeError):
    """Raised when the checked-in upstream contract cannot be satisfied."""


class OfficialSample(NamedTuple):
    category: str
    name: str
    source: str


@dataclass(frozen=True)
class PatchSpec:
    path: Path
    sha256: str


@dataclass(frozen=True)
class BindingSpec:
    mode: str
    output: Path
    features: tuple[str, ...]
    fingerprint_key: str


@dataclass(frozen=True)
class UpstreamContract:
    repo_root: Path
    manifest_path: Path
    repository: str
    commit: str
    vendor_root: Path
    vendor_paths: tuple[str, ...]
    patches: tuple[PatchSpec, ...]
    provider_module: str
    provider_asset_basename: str
    provider_bridge_revision: int
    provider_precisions: tuple[str, ...]
    provider_required_capabilities: tuple[str, ...]
    provider_unsupported_capabilities: tuple[str, ...]
    rust_contract: Path
    bindings: Mapping[str, BindingSpec]
    sample_inventory: Path
    capability_inventory: Path
    fingerprints: Mapping[str, str]


def validate_relative_path(value: str) -> str:
    path = PurePosixPath(value)
    if path.is_absolute() or not path.parts or any(part in {"", ".", ".."} for part in path.parts):
        raise ContractError(f"contract path must stay inside its declared root: {value!r}")
    return path.as_posix()


def resolve_repo_path(repo_root: Path, value: str) -> Path:
    relative = validate_relative_path(value)
    resolved_root = repo_root.resolve()
    resolved = (resolved_root / relative).resolve()
    try:
        resolved.relative_to(resolved_root)
    except ValueError as exc:
        raise ContractError(f"contract path escapes repository root: {value!r}") from exc
    return resolved


def require_table(parent: Mapping[str, object], key: str) -> Mapping[str, object]:
    value = parent.get(key)
    if not isinstance(value, dict):
        raise ContractError(f"manifest field {key!r} must be a table")
    return value


def require_string(parent: Mapping[str, object], key: str) -> str:
    value = parent.get(key)
    if not isinstance(value, str) or not value:
        raise ContractError(f"manifest field {key!r} must be a non-empty string")
    return value


def require_string_list(parent: Mapping[str, object], key: str) -> tuple[str, ...]:
    value = parent.get(key)
    if not isinstance(value, list) or not all(isinstance(item, str) and item for item in value):
        raise ContractError(f"manifest field {key!r} must be an array of non-empty strings")
    return tuple(value)


def load_contract(repo_root: Path) -> UpstreamContract:
    repo_root = repo_root.resolve()
    manifest_path = repo_root / MANIFEST_RELATIVE_PATH
    try:
        raw = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise ContractError(f"failed to read {manifest_path}: {exc}") from exc

    if raw.get("schema_version") != 1:
        raise ContractError("box3d-upstream.toml must use schema_version = 1")

    upstream = require_table(raw, "upstream")
    vendor = require_table(raw, "vendor")
    provider = require_table(raw, "provider")
    bindings_raw = require_table(raw, "bindings")
    samples = require_table(raw, "samples")
    capabilities = require_table(raw, "capabilities")
    fingerprints_raw = require_table(raw, "fingerprints")

    commit = require_string(upstream, "commit")
    if not COMMIT_PATTERN.fullmatch(commit):
        raise ContractError("upstream.commit must be a full lowercase 40-character SHA")

    vendor_paths = tuple(validate_relative_path(path) for path in require_string_list(vendor, "paths"))
    if len(vendor_paths) != len(set(vendor_paths)):
        raise ContractError("vendor.paths contains duplicate entries")
    if tuple(sorted(vendor_paths)) != vendor_paths:
        raise ContractError("vendor.paths must be sorted for reviewable diffs")

    patches_raw = raw.get("patches", [])
    if not isinstance(patches_raw, list):
        raise ContractError("manifest patches must be an array of tables")
    patches: list[PatchSpec] = []
    for index, patch_raw in enumerate(patches_raw):
        if not isinstance(patch_raw, dict):
            raise ContractError(f"patches[{index}] must be a table")
        digest = require_string(patch_raw, "sha256")
        if not SHA256_PATTERN.fullmatch(digest):
            raise ContractError(f"patches[{index}].sha256 must be a SHA-256 digest")
        patches.append(
            PatchSpec(
                path=resolve_repo_path(repo_root, require_string(patch_raw, "path")),
                sha256=digest,
            )
        )

    binding_specs: dict[str, BindingSpec] = {}
    for mode in ("default", "double"):
        binding = require_table(bindings_raw, mode)
        binding_specs[mode] = BindingSpec(
            mode=mode,
            output=resolve_repo_path(repo_root, require_string(binding, "output")),
            features=require_string_list(binding, "features"),
            fingerprint_key=f"bindings_{mode}",
        )

    fingerprints: dict[str, str] = {}
    for key, value in fingerprints_raw.items():
        if not isinstance(value, str) or not SHA256_PATTERN.fullmatch(value):
            raise ContractError(f"fingerprints.{key} must be a SHA-256 digest")
        fingerprints[key] = value

    bridge_revision = provider.get("bridge_revision")
    if not isinstance(bridge_revision, int) or bridge_revision <= 0:
        raise ContractError("provider.bridge_revision must be a positive integer")

    return UpstreamContract(
        repo_root=repo_root,
        manifest_path=manifest_path,
        repository=require_string(upstream, "repository"),
        commit=commit,
        vendor_root=resolve_repo_path(repo_root, require_string(vendor, "root")),
        vendor_paths=vendor_paths,
        patches=tuple(patches),
        provider_module=require_string(provider, "module"),
        provider_asset_basename=require_string(provider, "asset_basename"),
        provider_bridge_revision=bridge_revision,
        provider_precisions=require_string_list(provider, "supported_precisions"),
        provider_required_capabilities=require_string_list(provider, "required_capabilities"),
        provider_unsupported_capabilities=require_string_list(
            provider, "intentionally_unsupported_capabilities"
        ),
        rust_contract=resolve_repo_path(repo_root, require_string(bindings_raw, "rust_contract")),
        bindings=binding_specs,
        sample_inventory=resolve_repo_path(repo_root, require_string(samples, "inventory")),
        capability_inventory=resolve_repo_path(
            repo_root, require_string(capabilities, "inventory")
        ),
        fingerprints=fingerprints,
    )


def run_command(
    command: Sequence[str],
    *,
    cwd: Path,
    env: Mapping[str, str] | None = None,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    printable = " ".join(command)
    print(f"$ {printable}")
    try:
        return subprocess.run(
            list(command),
            cwd=cwd,
            env=dict(env) if env is not None else None,
            check=True,
            capture_output=capture_output,
            text=True,
        )
    except subprocess.CalledProcessError as exc:
        if not capture_output:
            raise
        diagnostic = (exc.stderr or exc.stdout or "").strip()
        suffix = f"\n{diagnostic}" if diagnostic else ""
        raise ContractError(f"command failed ({exc.returncode}): {printable}{suffix}") from exc


def git_bytes(source: Path, *arguments: str) -> bytes:
    command = ["git", "-C", str(source), *arguments]
    try:
        result = subprocess.run(command, check=True, capture_output=True)
    except subprocess.CalledProcessError as exc:
        stderr = exc.stderr.decode("utf-8", errors="replace").strip()
        raise ContractError(f"git command failed: {' '.join(command)}\n{stderr}") from exc
    return result.stdout


def normalize_repository_url(value: str) -> str:
    normalized = value.strip().rstrip("/")
    if normalized.startswith("git@github.com:"):
        normalized = "https://github.com/" + normalized.removeprefix("git@github.com:")
    if normalized.endswith(".git"):
        normalized = normalized[:-4]
    return normalized.lower()


def verify_source_checkout(source: Path, contract: UpstreamContract) -> None:
    source = source.resolve()
    if not source.is_dir():
        raise ContractError(f"upstream source is not a directory: {source}")
    actual_repository = git_bytes(source, "remote", "get-url", "origin").decode().strip()
    if normalize_repository_url(actual_repository) != normalize_repository_url(contract.repository):
        raise ContractError(
            f"upstream repository mismatch: expected {contract.repository!r}, got {actual_repository!r}"
        )
    resolved = git_bytes(source, "rev-parse", "--verify", f"{contract.commit}^{{commit}}").decode().strip()
    if resolved != contract.commit:
        raise ContractError(
            f"upstream commit mismatch: expected {contract.commit}, resolved {resolved}"
        )


def checkout_source(contract: UpstreamContract, destination: Path) -> None:
    destination = destination.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    if os.path.lexists(destination):
        raise ContractError(f"checkout destination already exists: {destination}")

    with tempfile.TemporaryDirectory(
        prefix=f".{destination.name}-",
        dir=destination.parent,
    ) as temporary_directory:
        staging = Path(temporary_directory)
        run_command(["git", "init", str(staging)], cwd=destination.parent)
        run_command(
            ["git", "-C", str(staging), "remote", "add", "origin", contract.repository],
            cwd=destination.parent,
        )
        run_command(
            [
                "git",
                "-C",
                str(staging),
                "fetch",
                "--depth",
                "1",
                "origin",
                contract.commit,
            ],
            cwd=destination.parent,
        )
        run_command(
            ["git", "-C", str(staging), "checkout", "--detach", contract.commit],
            cwd=destination.parent,
        )

        verify_source_checkout(staging, contract)
        head = git_bytes(staging, "rev-parse", "HEAD").decode().strip()
        if head != contract.commit:
            raise ContractError(
                f"upstream checkout HEAD mismatch: expected {contract.commit}, got {head}"
            )
        if os.path.lexists(destination):
            raise ContractError(f"checkout destination already exists: {destination}")
        staging.rename(destination)

    print(f"checked out Box3D {contract.commit} into {destination}")


def materialize_git_paths(
    source: Path,
    commit: str,
    paths: Sequence[str],
    destination: Path,
) -> None:
    validated = tuple(validate_relative_path(path) for path in paths)
    entries = git_archive_entries(source, commit, validated)
    destination.mkdir(parents=True, exist_ok=True)
    for relative in validated:
        output = destination.joinpath(*PurePosixPath(relative).parts)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(entries[relative])


def git_archive_entries(
    source: Path,
    commit: str,
    paths: Sequence[str],
) -> dict[str, bytes]:
    validated = tuple(validate_relative_path(path) for path in paths)
    if not validated:
        return {}
    archive = git_bytes(source, "archive", "--format=tar", commit, "--", *validated)
    entries: dict[str, bytes] = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as stream:
        for member in stream:
            if member.isdir():
                continue
            relative = validate_relative_path(member.name)
            if not member.isfile() or relative in entries:
                raise ContractError(f"unexpected Git archive member: {member.name!r}")
            extracted = stream.extractfile(member)
            if extracted is None:
                raise ContractError(f"failed to read Git archive member: {member.name!r}")
            entries[relative] = extracted.read()
    expected = set(validated)
    actual = set(entries)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise ContractError(f"Git archive path mismatch: missing={missing}, extra={extra}")
    return entries


def fingerprint_entries(entries: Iterable[tuple[str, bytes]]) -> str:
    digest = hashlib.sha256()
    for path, content in sorted(entries, key=lambda item: item[0]):
        encoded_path = validate_relative_path(path).encode("utf-8")
        digest.update(len(encoded_path).to_bytes(8, "big"))
        digest.update(encoded_path)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_entries(root: Path) -> dict[str, bytes]:
    if not root.is_dir():
        raise ContractError(f"expected directory is missing: {root}")
    return {
        path.relative_to(root).as_posix(): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def fingerprint_tree(root: Path) -> str:
    return fingerprint_entries(tree_entries(root).items())


def fingerprint_selected_paths(root: Path, paths: Sequence[str]) -> str:
    entries: list[tuple[str, bytes]] = []
    for relative in paths:
        path = root.joinpath(*PurePosixPath(relative).parts)
        if not path.is_file():
            raise ContractError(f"fingerprinted path is missing: {path}")
        entries.append((relative, path.read_bytes()))
    return fingerprint_entries(entries)


def apply_declared_patches(contract: UpstreamContract, destination: Path) -> None:
    patch_environment = os.environ.copy()
    patch_environment["GIT_CEILING_DIRECTORIES"] = str(destination.resolve())
    patch_environment["GIT_WORK_TREE"] = str(destination.resolve())
    for patch in contract.patches:
        actual_digest = file_sha256(patch.path)
        if actual_digest != patch.sha256:
            raise ContractError(
                f"patch fingerprint mismatch for {patch.path}: expected {patch.sha256}, got {actual_digest}"
            )
        run_command(
            ["git", "apply", "--whitespace=nowarn", str(patch.path)],
            cwd=destination,
            env=patch_environment,
        )


def materialize_expected_vendor(
    contract: UpstreamContract,
    source: Path,
    destination: Path,
) -> None:
    materialize_git_paths(source, contract.commit, contract.vendor_paths, destination)
    apply_declared_patches(contract, destination)


def compare_trees(expected: Path, actual: Path) -> None:
    expected_entries = tree_entries(expected)
    actual_entries = tree_entries(actual)
    expected_paths = set(expected_entries)
    actual_paths = set(actual_entries)
    missing = sorted(expected_paths - actual_paths)
    extra = sorted(actual_paths - expected_paths)
    changed = sorted(
        path
        for path in expected_paths & actual_paths
        if expected_entries[path] != actual_entries[path]
    )
    if not missing and not extra and not changed:
        return
    details = []
    if missing:
        details.append("missing: " + ", ".join(missing))
    if extra:
        details.append("extra: " + ", ".join(extra))
    if changed:
        details.append("changed: " + ", ".join(changed))
    raise ContractError("vendored Box3D tree does not match the contract:\n  " + "\n  ".join(details))


def replace_vendor_tree(expected: Path, vendor_root: Path) -> None:
    vendor_root.parent.mkdir(parents=True, exist_ok=True)
    backup = vendor_root.parent / f".{vendor_root.name}.backup-{os.getpid()}"
    if backup.exists():
        raise ContractError(f"refusing to overwrite stale synchronization backup: {backup}")
    if vendor_root.exists():
        os.replace(vendor_root, backup)
    try:
        os.replace(expected, vendor_root)
    except BaseException:
        if backup.exists() and not vendor_root.exists():
            os.replace(backup, vendor_root)
        raise
    if backup.exists():
        shutil.rmtree(backup)


def list_git_paths(source: Path, commit: str, prefix: str) -> tuple[str, ...]:
    output = git_bytes(source, "ls-tree", "-r", "--name-only", commit, prefix)
    return tuple(
        validate_relative_path(line)
        for line in output.decode("utf-8").splitlines()
        if line.strip()
    )


def parse_official_samples(sources: Mapping[str, str]) -> list[OfficialSample]:
    samples: list[OfficialSample] = []
    for path, source in sorted(sources.items()):
        name = PurePosixPath(validate_relative_path(path)).name
        for line_number, line in enumerate(source.splitlines(), start=1):
            match = CPP_REGISTRATION_PATTERN.search(line)
            if match is not None:
                samples.append(OfficialSample(match.group(1), match.group(2), f"{name}:{line_number}"))
    samples.sort()
    if not samples:
        raise ContractError("the pinned upstream commit contains no official sample registrations")
    return samples


def official_samples_from_commit(source: Path, commit: str) -> list[OfficialSample]:
    paths = tuple(
        path
        for path in list_git_paths(source, commit, "samples")
        if PurePosixPath(path).name.startswith("sample_") and path.endswith(".cpp")
    )
    sources = {
        path: content.decode("utf-8")
        for path, content in git_archive_entries(source, commit, paths).items()
    }
    return parse_official_samples(sources)


def render_sample_inventory(commit: str, samples: Sequence[OfficialSample]) -> bytes:
    payload = {
        "schema_version": 1,
        "upstream_commit": commit,
        "samples": [sample._asdict() for sample in samples],
    }
    return (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode("utf-8")


def extract_bound_functions(binding_source: str) -> tuple[str, ...]:
    extern_lines: list[str] = []
    in_extern_block = False
    for line in binding_source.splitlines():
        if not in_extern_block and re.fullmatch(r'\s*unsafe extern "C"\s*\{\s*', line):
            in_extern_block = True
            continue
        if in_extern_block and line.strip() == "}":
            in_extern_block = False
            continue
        if in_extern_block:
            extern_lines.append(line)
    if in_extern_block:
        raise ContractError("generated bindings contain an unterminated extern block")
    functions = tuple(sorted(set(RUST_FUNCTION_PATTERN.findall("\n".join(extern_lines)))))
    if not functions:
        raise ContractError("generated bindings contain no public Box3D functions")
    return functions


def render_capability_inventory(
    contract: UpstreamContract,
    default_binding: str,
    double_binding: str,
) -> bytes:
    default_functions = set(extract_bound_functions(default_binding))
    double_functions = set(extract_bound_functions(double_binding))
    all_functions = sorted(default_functions | double_functions)

    def classification(name: str, functions: set[str]) -> str:
        return "required-supported" if name in functions else "not-exposed-by-upstream"

    payload = {
        "schema_version": 1,
        "upstream_commit": contract.commit,
        "provider": {
            "module": contract.provider_module,
            "bridge_revision": contract.provider_bridge_revision,
            "supported_precisions": list(contract.provider_precisions),
            "required_capabilities": list(contract.provider_required_capabilities),
            "intentionally_unsupported_capabilities": list(
                contract.provider_unsupported_capabilities
            ),
        },
        "raw_functions": [
            {
                "name": name,
                "native_default": classification(name, default_functions),
                "native_double": classification(name, double_functions),
                "wasm_source_default": classification(name, default_functions),
                "wasm_source_double": classification(name, double_functions),
                "wasm_provider_default_symbol": classification(name, default_functions),
                "wasm_provider_double_symbol": "intentionally-unsupported",
            }
            for name in all_functions
        ],
    }
    return (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode("utf-8")


def render_rust_contract(contract: UpstreamContract) -> bytes:
    supports_double = "double" in contract.provider_precisions
    c_sources = [path for path in contract.vendor_paths if path.endswith(".c")]
    rendered_sources = "\n".join(f"    {json.dumps(path)}," for path in c_sources)
    source = f'''// @generated by tools/update_box3d_and_bindings.py; do not edit.

pub const UPSTREAM_REPOSITORY: &str = {json.dumps(contract.repository)};
pub const UPSTREAM_COMMIT: &str = {json.dumps(contract.commit)};
pub const PROVIDER_MODULE: &str = {json.dumps(contract.provider_module)};
pub const PROVIDER_ASSET_BASENAME: &str = {json.dumps(contract.provider_asset_basename)};
pub const PROVIDER_BRIDGE_REVISION: u32 = {contract.provider_bridge_revision};
pub const PROVIDER_SUPPORTS_DOUBLE_PRECISION: bool = {str(supports_double).lower()};
pub const BOX3D_C_SOURCES: &[&str] = &[
{rendered_sources}
];
'''
    return source.encode("utf-8")


def write_if_changed(path: Path, content: bytes) -> bool:
    if path.is_file() and path.read_bytes() == content:
        return False
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)
    print(f"updated {path}")
    return True


def binding_header(mode: str) -> str:
    return (
        "// AUTOGENERATED: pregenerated Box3D bindings for docs.rs/offline builds\n"
        "// To refresh, run tools/update_box3d_and_bindings.py generate "
        f"--mode {mode}\n\n"
    )


def generated_binding_content(
    contract: UpstreamContract,
    spec: BindingSpec,
    profile: str,
) -> bytes:
    env = os.environ.copy()
    env["BOXDDD_SYS_SKIP_CC"] = "1"
    env["BOXDDD_SYS_FORCE_BINDGEN"] = "1"
    command = [
        "cargo",
        "build",
        "-p",
        "boxddd-sys",
        "--no-default-features",
        "--features",
        ",".join(spec.features),
        "--message-format=json-render-diagnostics",
    ]
    if profile == "release":
        command.append("--release")
    result = run_command(
        command,
        cwd=contract.repo_root,
        env=env,
        capture_output=True,
    )
    out_dirs: list[Path] = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") != "build-script-executed":
            continue
        package_id = str(message.get("package_id", ""))
        if "boxddd-sys" not in package_id:
            continue
        out_dir = message.get("out_dir")
        if isinstance(out_dir, str):
            out_dirs.append(Path(out_dir))
    if not out_dirs:
        raise ContractError("cargo did not report a boxddd-sys build-script output directory")
    generated = out_dirs[-1] / "bindings.rs"
    if not generated.is_file():
        raise ContractError(f"bindgen output is missing: {generated}")
    return (binding_header(spec.mode) + generated.read_text(encoding="utf-8")).encode("utf-8")


def replace_manifest_fingerprints(manifest_path: Path, values: Mapping[str, str]) -> None:
    source = manifest_path.read_text(encoding="utf-8")
    for key, value in sorted(values.items()):
        if not SHA256_PATTERN.fullmatch(value):
            raise ContractError(f"refusing to write invalid SHA-256 digest for {key}")
        pattern = re.compile(rf'(?m)^({re.escape(key)}\s*=\s*)"[0-9a-f]{{64}}"$')
        source, count = pattern.subn(rf'\g<1>"{value}"', source)
        if count != 1:
            raise ContractError(
                f"expected exactly one fingerprints.{key} entry in {manifest_path}, found {count}"
            )
    temporary = manifest_path.with_suffix(manifest_path.suffix + ".tmp")
    temporary.write_text(source, encoding="utf-8", newline="\n")
    os.replace(temporary, manifest_path)


def patch_fingerprint(contract: UpstreamContract) -> str:
    return fingerprint_entries(
        (
            patch.path.relative_to(contract.repo_root).as_posix(),
            patch.path.read_bytes(),
        )
        for patch in contract.patches
    )


def fingerprint_for_key(contract: UpstreamContract, key: str) -> str:
    header_paths = tuple(path for path in contract.vendor_paths if path.startswith("include/"))
    paths = {
        "rust_contract": contract.rust_contract,
        "samples": contract.sample_inventory,
        "capabilities": contract.capability_inventory,
    }
    if key == "vendor":
        return fingerprint_tree(contract.vendor_root)
    if key == "headers":
        return fingerprint_selected_paths(contract.vendor_root, header_paths)
    if key == "patches":
        return patch_fingerprint(contract)
    if key in paths:
        return file_sha256(paths[key])
    for mode, spec in contract.bindings.items():
        if key == spec.fingerprint_key:
            return file_sha256(spec.output)
    raise ContractError(f"unknown upstream fingerprint key: {key}")


def verify_fingerprints(contract: UpstreamContract, keys: Sequence[str] | None = None) -> None:
    selected = tuple(keys) if keys is not None else tuple(sorted(contract.fingerprints))
    failures = []
    for key in selected:
        expected = contract.fingerprints.get(key)
        if expected is None:
            failures.append(f"{key}: missing from manifest")
            continue
        actual = fingerprint_for_key(contract, key)
        if actual != expected:
            failures.append(f"{key}: expected {expected}, got {actual}")
    if failures:
        raise ContractError("upstream artifact fingerprint mismatch:\n  " + "\n  ".join(failures))


def sync_sources(contract: UpstreamContract, source: Path) -> None:
    verify_source_checkout(source, contract)
    with tempfile.TemporaryDirectory(prefix="boxddd-upstream-", dir=contract.vendor_root.parent) as temp:
        expected = Path(temp) / "box3d"
        materialize_expected_vendor(contract, source, expected)
        replace_vendor_tree(expected, contract.vendor_root)

    samples = official_samples_from_commit(source, contract.commit)
    write_if_changed(
        contract.sample_inventory,
        render_sample_inventory(contract.commit, samples),
    )
    write_if_changed(contract.rust_contract, render_rust_contract(contract))
    fingerprints = {
        "vendor": fingerprint_tree(contract.vendor_root),
        "headers": fingerprint_selected_paths(
            contract.vendor_root,
            tuple(path for path in contract.vendor_paths if path.startswith("include/")),
        ),
        "patches": patch_fingerprint(contract),
        "samples": file_sha256(contract.sample_inventory),
        "rust_contract": file_sha256(contract.rust_contract),
    }
    replace_manifest_fingerprints(contract.manifest_path, fingerprints)
    print(f"synchronized Box3D {contract.commit} into {contract.vendor_root}")


def selected_modes(mode: str) -> tuple[str, ...]:
    return ("default", "double") if mode == "both" else (mode,)


def generate_artifacts(contract: UpstreamContract, mode: str, profile: str) -> None:
    verify_fingerprints(contract, ("vendor", "headers", "patches", "rust_contract"))
    generated: dict[str, bytes] = {}
    for selected in selected_modes(mode):
        spec = contract.bindings[selected]
        content = generated_binding_content(contract, spec, profile)
        write_if_changed(spec.output, content)
        generated[selected] = content

    default_source = generated.get("default")
    if default_source is None:
        default_source = contract.bindings["default"].output.read_bytes()
    double_source = generated.get("double")
    if double_source is None:
        double_source = contract.bindings["double"].output.read_bytes()
    capabilities = render_capability_inventory(
        contract,
        default_source.decode("utf-8"),
        double_source.decode("utf-8"),
    )
    write_if_changed(contract.capability_inventory, capabilities)

    fingerprints = {
        contract.bindings[selected].fingerprint_key: file_sha256(
            contract.bindings[selected].output
        )
        for selected in selected_modes(mode)
    }
    fingerprints["capabilities"] = file_sha256(contract.capability_inventory)
    replace_manifest_fingerprints(contract.manifest_path, fingerprints)


def check_artifacts(
    contract: UpstreamContract,
    source: Path,
    mode: str,
    profile: str,
) -> None:
    verify_source_checkout(source, contract)
    with tempfile.TemporaryDirectory(prefix="boxddd-check-") as temp:
        expected = Path(temp) / "box3d"
        materialize_expected_vendor(contract, source, expected)
        compare_trees(expected, contract.vendor_root)

    expected_samples = render_sample_inventory(
        contract.commit,
        official_samples_from_commit(source, contract.commit),
    )
    if contract.sample_inventory.read_bytes() != expected_samples:
        raise ContractError(f"sample inventory is stale: {contract.sample_inventory}")
    expected_contract = render_rust_contract(contract)
    if contract.rust_contract.read_bytes() != expected_contract:
        raise ContractError(f"generated Rust contract is stale: {contract.rust_contract}")

    generated: dict[str, bytes] = {}
    for selected in selected_modes(mode):
        spec = contract.bindings[selected]
        content = generated_binding_content(contract, spec, profile)
        generated[selected] = content
        if spec.output.read_bytes() != content:
            raise ContractError(f"pregenerated {selected} bindings are stale: {spec.output}")

    default_source = generated.get("default", contract.bindings["default"].output.read_bytes())
    double_source = generated.get("double", contract.bindings["double"].output.read_bytes())
    expected_capabilities = render_capability_inventory(
        contract,
        default_source.decode("utf-8"),
        double_source.decode("utf-8"),
    )
    if contract.capability_inventory.read_bytes() != expected_capabilities:
        raise ContractError(f"capability inventory is stale: {contract.capability_inventory}")
    verify_fingerprints(contract)
    print(f"Box3D upstream contract is reproducible at {contract.commit}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Synchronize and verify the pinned Box3D source and bindings"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    checkout = subparsers.add_parser(
        "checkout", help="check out the pinned upstream commit into a new directory"
    )
    checkout.add_argument("--destination", type=Path, required=True)

    sync = subparsers.add_parser("sync", help="replace the vendor tree from a pinned commit")
    sync.add_argument("--source", type=Path, required=True)

    generate = subparsers.add_parser("generate", help="regenerate checked-in Rust artifacts")
    generate.add_argument("--mode", choices=("default", "double", "both"), default="both")
    generate.add_argument("--profile", choices=("debug", "release"), default="debug")

    check = subparsers.add_parser("check", help="verify sources and generated artifacts read-only")
    check.add_argument("--source", type=Path, required=True)
    check.add_argument("--mode", choices=("default", "double", "both"), default="both")
    check.add_argument("--profile", choices=("debug", "release"), default="debug")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    try:
        contract = load_contract(repo_root)
        if args.command == "checkout":
            checkout_source(contract, args.destination)
        elif args.command == "sync":
            sync_sources(contract, args.source)
        elif args.command == "generate":
            generate_artifacts(contract, args.mode, args.profile)
        elif args.command == "check":
            check_artifacts(contract, args.source, args.mode, args.profile)
        else:
            raise AssertionError(f"unhandled command: {args.command}")
    except (ContractError, OSError, subprocess.CalledProcessError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
