#!/usr/bin/env python3
"""Audit one reviewed release profile with cargo-semver-checks."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Mapping, Sequence


REPO_ROOT = Path(__file__).resolve().parents[1]
ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
BLOCK_RE = re.compile(
    r"^--- (?P<level>failure|warning) (?P<lint>[^:]+):[^\n]*$",
    re.MULTILINE,
)
EVIDENCE_RE = re.compile(r"^  (?P<evidence>\S.*)$", re.MULTILINE)


class AuditError(RuntimeError):
    """A release contract or semver inventory mismatch."""


@dataclass(frozen=True)
class ExpectedFailure:
    lint: str
    needle: str
    reason: str


@dataclass(frozen=True)
class ReleaseProfile:
    release_version: str
    baseline_rev: str
    release_type: str
    expected_breaks: Mapping[str, tuple[ExpectedFailure, ...]]


EXPECTED_0_3_BREAKS: dict[str, tuple[ExpectedFailure, ...]] = {
    "boxddd": (
        ExpectedFailure(
            lint="enum_variant_missing",
            needle="variant Error::InvalidArgument",
            reason="typed InvalidValue errors replace the coarse invalid-argument variant",
        ),
        ExpectedFailure(
            lint="struct_missing",
            needle="struct boxddd::DebugShape",
            reason="debug draw geometry is retained as DebugShapeAsset values",
        ),
        ExpectedFailure(
            lint="inherent_method_missing",
            needle="World::try_step",
            reason="fallible operations use their canonical unprefixed names",
        ),
    ),
    "bevy_boxddd": (
        ExpectedFailure(
            lint="enum_variant_missing",
            needle="variant BoxdddErrorPolicy::Panic",
            reason="recoverable plugin failures are always emitted as messages",
        ),
        ExpectedFailure(
            lint="trait_missing",
            needle="trait bevy_boxddd::math::BevyVec3BoxdddExt",
            reason="math conversion extension traits were replaced by free functions",
        ),
    ),
    "boxddd-sys": (),
}


RELEASE_PROFILES: dict[str, ReleaseProfile] = {
    "0.3.0": ReleaseProfile(
        release_version="0.3.0",
        baseline_rev="v0.2.0",
        release_type="minor",
        expected_breaks=EXPECTED_0_3_BREAKS,
    ),
}


def select_release_profile(release_version: str) -> ReleaseProfile:
    try:
        profile = RELEASE_PROFILES[release_version]
    except KeyError as error:
        supported = ", ".join(sorted(RELEASE_PROFILES))
        raise AuditError(
            f"unsupported release version {release_version}; supported versions: {supported}"
        ) from error
    validate_profile(profile)
    return profile


def validate_profile(profile: ReleaseProfile) -> None:
    if profile.release_type not in {"patch", "minor", "major"}:
        raise AuditError(
            f"release profile {profile.release_version} has invalid release type "
            f"{profile.release_type}"
        )
    if not profile.baseline_rev.startswith("v"):
        raise AuditError(
            f"release profile {profile.release_version} has invalid baseline "
            f"{profile.baseline_rev}"
        )
    if not profile.expected_breaks:
        raise AuditError(f"release profile {profile.release_version} has no release crates")

    seen: set[tuple[str, str, str]] = set()
    for crate, expected in profile.expected_breaks.items():
        if not crate:
            raise AuditError(f"release profile {profile.release_version} has an empty crate name")
        for item in expected:
            if not item.lint or not item.needle or not item.reason:
                raise AuditError(
                    f"release profile {profile.release_version} has incomplete break evidence "
                    f"for {crate}"
                )
            key = (crate, item.lint, item.needle)
            if key in seen:
                raise AuditError(
                    f"release profile {profile.release_version} repeats break evidence "
                    f"{crate}: {item.lint} / {item.needle}"
                )
            seen.add(key)


def read_toml(path: Path) -> dict[str, object]:
    try:
        with path.open("rb") as source:
            return tomllib.load(source)
    except FileNotFoundError as error:
        raise AuditError(f"missing manifest {path}") from error
    except tomllib.TOMLDecodeError as error:
        raise AuditError(f"invalid TOML in {path}: {error}") from error


def validate_workspace_contract(root: Path, profile: ReleaseProfile) -> None:
    root_manifest = read_toml(root / "Cargo.toml")
    workspace = root_manifest.get("workspace")
    if not isinstance(workspace, dict):
        raise AuditError("root Cargo.toml is missing [workspace]")
    workspace_package = workspace.get("package")
    if not isinstance(workspace_package, dict):
        raise AuditError("root Cargo.toml is missing [workspace.package]")
    workspace_version = workspace_package.get("version")
    if workspace_version != profile.release_version:
        raise AuditError(
            f"workspace version {workspace_version} does not match release profile "
            f"{profile.release_version}"
        )

    dependencies = workspace.get("dependencies", {})
    if not isinstance(dependencies, dict):
        raise AuditError("[workspace.dependencies] must be a table")

    for crate in profile.expected_breaks:
        manifest_path = root / crate / "Cargo.toml"
        if not manifest_path.is_file():
            raise AuditError(f"missing manifest for release crate {crate}: {manifest_path}")
        manifest = read_toml(manifest_path)
        package = manifest.get("package")
        if not isinstance(package, dict):
            raise AuditError(f"manifest for release crate {crate} is missing [package]")
        if package.get("name") != crate:
            raise AuditError(
                f"release crate directory {crate} declares package name {package.get('name')}"
            )
        package_version = package.get("version")
        if package_version == {"workspace": True}:
            resolved_version = workspace_version
        elif isinstance(package_version, str):
            resolved_version = package_version
        else:
            raise AuditError(f"crate {crate} has an invalid package version declaration")
        if resolved_version != profile.release_version:
            raise AuditError(
                f"crate {crate} version {resolved_version} does not match release profile "
                f"{profile.release_version}"
            )
        if package.get("publish") is False:
            raise AuditError(f"release profile includes non-publishable crate {crate}")

        dependency = dependencies.get(crate)
        if dependency is None:
            continue
        if isinstance(dependency, str):
            dependency_version = dependency
        elif isinstance(dependency, dict) and isinstance(
            dependency.get("version"), str
        ):
            dependency_version = dependency["version"]
        else:
            raise AuditError(
                f"workspace dependency {crate} has no valid release version"
            )
        if dependency_version != profile.release_version:
            raise AuditError(
                f"workspace dependency {crate} version {dependency_version} does not match "
                f"release profile {profile.release_version}"
            )


def semver_command(crate: str, profile: ReleaseProfile) -> list[str]:
    if crate not in profile.expected_breaks:
        raise AuditError(
            f"crate {crate} is not part of release profile {profile.release_version}"
        )
    return [
        "cargo",
        "semver-checks",
        "check-release",
        "-p",
        crate,
        "--baseline-rev",
        profile.baseline_rev,
        "--release-type",
        profile.release_type,
    ]


def run_semver(crate: str, profile: ReleaseProfile) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            semver_command(crate, profile),
            check=False,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
    except OSError as error:
        raise AuditError(
            f"could not run cargo-semver-checks for {crate}: {error}"
        ) from error


def failure_blocks(output: str) -> list[tuple[str, str]]:
    clean_output = ANSI_RE.sub("", output)
    matches = list(BLOCK_RE.finditer(clean_output))
    blocks: list[tuple[str, str]] = []
    for index, match in enumerate(matches):
        if match.group("level") != "failure":
            continue
        end = matches[index + 1].start() if index + 1 < len(matches) else len(clean_output)
        blocks.append((match.group("lint"), clean_output[match.start() : end]))
    return blocks


def failure_evidence(lint: str, block: str) -> list[str]:
    marker = "\nFailed in:\n"
    if marker not in block:
        raise AuditError(f"failure block {lint} has no parseable API evidence")
    evidence = [
        match.group("evidence").strip()
        for match in EVIDENCE_RE.finditer(block.split(marker, 1)[1])
    ]
    if not evidence:
        raise AuditError(f"failure block {lint} has no parseable API evidence")
    return evidence


def validate_semver_result(
    crate: str,
    expected: Sequence[ExpectedFailure],
    completed: subprocess.CompletedProcess[str],
) -> None:
    output = completed.stdout or ""
    blocks = failure_blocks(output)
    if completed.returncode != 0 and not blocks:
        diagnostic = " ".join(output.split()) or "no output"
        raise AuditError(
            f"cargo-semver-checks failed for {crate} without parseable semver failure "
            f"evidence: {diagnostic}"
        )
    if completed.returncode == 0 and blocks:
        raise AuditError(
            f"cargo-semver-checks reported failure blocks for {crate} but exited successfully"
        )

    observed: list[tuple[str, str]] = []
    for lint, block in blocks:
        observed.extend((lint, evidence) for evidence in failure_evidence(lint, block))

    matched: set[int] = set()
    errors: list[str] = []
    for lint, evidence in observed:
        candidates = [
            index
            for index, item in enumerate(expected)
            if item.lint == lint and item.needle in evidence
        ]
        if len(candidates) != 1 or candidates[0] in matched:
            errors.append(f"unexpected semver break in {crate}: {lint}: {evidence}")
            continue
        matched.add(candidates[0])

    for index, item in enumerate(expected):
        if index not in matched:
            errors.append(
                f"expected semver break not observed in {crate}: "
                f"{item.lint} / {item.needle}"
            )

    if errors:
        raise AuditError("\n".join(errors))


def workflow_step(workflow: str, name: str) -> str:
    lines = workflow.splitlines()
    marker = f"- name: {name}"
    for index, line in enumerate(lines):
        if line.strip() != marker:
            continue
        indent = len(line) - len(line.lstrip())
        end = len(lines)
        for candidate in range(index + 1, len(lines)):
            candidate_line = lines[candidate]
            if (
                len(candidate_line) - len(candidate_line.lstrip()) == indent
                and candidate_line.strip().startswith("- name:")
            ):
                end = candidate
                break
        return "\n".join(lines[index:end])
    raise AuditError(f"release workflow is missing step {name}")


def validate_workflow_contract(workflow: str, profile: ReleaseProfile) -> None:
    if "- name: Verify Cargo workspace version matches input" in workflow:
        raise AuditError(
            "release workflow duplicates the profile-owned workspace version check"
        )
    validation_step = workflow_step(workflow, "Validate release semver profile")
    install_step = workflow_step(workflow, "Install cargo-semver-checks")
    audit_step = workflow_step(workflow, "Audit release semver breaks")
    positions = (
        workflow.index("- name: Validate release semver profile"),
        workflow.index("- name: Install cargo-semver-checks"),
        workflow.index("- name: Audit release semver breaks"),
    )
    if positions != tuple(sorted(positions)):
        raise AuditError(
            f"release workflow must validate profile {profile.release_version} before "
            "installing and running cargo-semver-checks"
        )
    for name, step in (
        ("Validate release semver profile", validation_step),
        ("Install cargo-semver-checks", install_step),
        ("Audit release semver breaks", audit_step),
    ):
        if re.search(r"^\s*if\s*:", step, re.MULTILINE):
            raise AuditError(f"release workflow step {name} must not have a skip condition")

    selection_command = (
        'python3 tools/semver_audit.py --release-version "${{ inputs.version }}"'
    )
    if f"{selection_command} --validate-only" not in validation_step:
        raise AuditError(
            "release workflow must validate the semver profile before installing Cargo tools"
        )
    if selection_command not in audit_step or "--validate-only" in audit_step:
        raise AuditError(
            "release workflow must select the semver profile through --release-version"
        )
    for forbidden in ("--baseline-rev", "--release-type"):
        if forbidden in workflow:
            raise AuditError(
                f"release workflow duplicates profile-owned argument {forbidden}"
            )


def audit_crate(crate: str, profile: ReleaseProfile) -> None:
    expected = profile.expected_breaks[crate]
    completed = run_semver(crate, profile)
    validate_semver_result(crate, expected, completed)
    if expected:
        print(f"{crate}: reviewed {profile.release_version} semver breaks matched:")
        for item in expected:
            print(f"  - {item.lint}: {item.needle} ({item.reason})")
    else:
        print(f"{crate}: no semver breaks")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--release-version",
        required=True,
        help="Release version used to select one exact reviewed profile.",
    )
    parser.add_argument(
        "--crate",
        action="append",
        help="Release crate to audit. Repeat to audit multiple crates; defaults to all.",
    )
    parser.add_argument(
        "--validate-only",
        action="store_true",
        help="Validate profile, workspace, and workflow without invoking Cargo.",
    )
    parser.add_argument(
        "--workspace-root",
        type=Path,
        default=REPO_ROOT,
        help=argparse.SUPPRESS,
    )
    return parser


def main(argv: Iterable[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        profile = select_release_profile(args.release_version)
        root = args.workspace_root.resolve()
        validate_workspace_contract(root, profile)
        workflow_path = root / ".github" / "workflows" / "release-preflight.yml"
        try:
            workflow = workflow_path.read_text(encoding="utf-8")
        except FileNotFoundError as error:
            raise AuditError(f"missing release workflow {workflow_path}") from error
        validate_workflow_contract(workflow, profile)
        if args.validate_only:
            print(
                f"release {profile.release_version}: profile matches workspace and workflow "
                f"({profile.baseline_rev}, {profile.release_type})"
            )
            return 0

        crates = args.crate or sorted(profile.expected_breaks)
        unknown_crates = sorted(set(crates) - set(profile.expected_breaks))
        if unknown_crates:
            raise AuditError(
                f"release profile {profile.release_version} has no crate(s): "
                f"{', '.join(unknown_crates)}"
            )

        failed = False
        for crate in crates:
            try:
                audit_crate(crate, profile)
            except AuditError as error:
                print(f"error: {error}", file=sys.stderr)
                failed = True
        return 1 if failed else 0
    except AuditError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
