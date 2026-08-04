#!/usr/bin/env python3
"""Audit one reviewed release profile with cargo-semver-checks."""

from __future__ import annotations

import argparse
import csv
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field, replace
from pathlib import Path
from typing import Iterable, Mapping, Sequence


REPO_ROOT = Path(__file__).resolve().parents[1]
ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
BLOCK_RE = re.compile(
    r"^--- (?P<level>failure|warning) (?P<lint>[^:]+):[^\n]*$",
    re.MULTILINE,
)
EVIDENCE_RE = re.compile(r"^  (?P<evidence>\S.*)$", re.MULTILINE)
SOURCE_LOCATION_RE = re.compile(
    r"^(?P<api>.+?)(?:, previously in file |, in | in file | in ).+:\d+(?::\d+)?$"
)
LINT_RE = re.compile(r"^[a-z][a-z0-9_]*$")
REASON_RE = re.compile(r"^[a-z][a-z0-9_]*$")
INVENTORY_HEADER = ("crate", "lint", "api", "reason")
MANUAL_REVIEW_LINT = "manual_reviewed_break"
BREAK_REASON_DESCRIPTIONS: Mapping[str, str] = {
    "advanced_step_publication": "Bevy publication follows native step advancement.",
    "collision_api_cleanup": "Collision helpers moved to the canonical API surface.",
    "debug_draw_assets": "Debug draw geometry uses retained asset values.",
    "error_message_policy": "Recoverable Bevy failures use message reporting.",
    "feature_surface_cleanup": "Obsolete Cargo feature surface was removed.",
    "foundation_error_model": "Typed Foundation and creation failures extend the error model.",
    "foundation_owner_lifetime": "Native owners retain Foundation activity through destruction.",
    "math_conversion_functions": "Math extension traits became conversion functions.",
    "owner_scoped_handles": "Public IDs now enforce owner and resource identity.",
    "raw_data_encapsulation": "Raw definition internals are no longer public API.",
    "replay_scale_isolation": "Replay state now carries scoped length-unit ownership.",
    "result_first_api": "Fallible operations use canonical result-returning names.",
    "scoped_event_views": "Event values and iterators are scoped to their World.",
    "typed_error_model": "Typed errors replace coarse or layout-stable variants.",
    "upstream_ffi_refresh": "Generated FFI matches the pinned upstream revision.",
    "bevy_foundation_lifecycle": "Bevy initialization owns explicit Foundation configuration.",
    "foundation_configuration": "Process configuration is frozen by explicit Foundation initialization.",
    "foundation_rooted_construction": "Scale-aware definitions and Worlds are created by Foundation factories.",
    "replay_exclusivity": "Replay has exclusive Foundation activity for its native lifetime.",
}


class AuditError(RuntimeError):
    """A release contract or semver inventory mismatch."""


@dataclass(frozen=True)
class ExpectedFailure:
    lint: str
    api: str
    reason: str


@dataclass(frozen=True)
class ReleaseProfile:
    release_version: str
    baseline_rev: str
    release_type: str
    inventory_path: Path
    release_crates: tuple[str, ...]
    expected_breaks: Mapping[str, tuple[ExpectedFailure, ...]] = field(
        default_factory=dict,
        repr=False,
    )


RELEASE_PROFILES: dict[str, ReleaseProfile] = {
    "0.4.0": ReleaseProfile(
        release_version="0.4.0",
        baseline_rev="v0.3.0",
        release_type="minor",
        inventory_path=Path("tools/semver/0.4.0.tsv"),
        release_crates=("bevy_boxddd", "boxddd", "boxddd-sys"),
    ),
}


def load_break_inventory(
    path: Path,
    release_crates: Sequence[str],
) -> dict[str, tuple[ExpectedFailure, ...]]:
    expected: dict[str, list[ExpectedFailure]] = {
        crate: [] for crate in release_crates
    }
    try:
        with path.open("r", encoding="utf-8", newline="") as source:
            rows = list(csv.reader(source, delimiter="\t", strict=True))
    except FileNotFoundError as error:
        raise AuditError(f"missing semver inventory {path}") from error
    except (OSError, UnicodeError, csv.Error) as error:
        raise AuditError(f"could not parse semver inventory {path}: {error}") from error

    if not rows or tuple(rows[0]) != INVENTORY_HEADER:
        raise AuditError(
            f"invalid semver inventory header in {path}; expected tab-separated "
            f"{' / '.join(INVENTORY_HEADER)}"
        )

    known_crates = set(release_crates)
    seen: set[tuple[str, str, str]] = set()
    previous_key: tuple[str, str, str] | None = None
    for line_number, row in enumerate(rows[1:], start=2):
        if len(row) != len(INVENTORY_HEADER):
            raise AuditError(
                f"invalid semver inventory row {line_number} in {path}: "
                f"expected {len(INVENTORY_HEADER)} tab-separated fields"
            )
        crate, lint, api, reason = row
        values = (crate, lint, api, reason)
        if (
            any(not value or value != value.strip() for value in values)
            or not LINT_RE.fullmatch(lint)
            or "*" in api
            or not REASON_RE.fullmatch(reason)
        ):
            raise AuditError(
                f"invalid semver inventory row {line_number} in {path}: "
                "crate, exact lint/API, and reason category are required"
            )
        if crate not in known_crates:
            raise AuditError(
                f"semver inventory {path} row {line_number} names unknown release crate "
                f"{crate}"
            )
        if reason not in BREAK_REASON_DESCRIPTIONS:
            raise AuditError(
                f"semver inventory {path} row {line_number} has unknown reason category "
                f"{reason}"
            )

        key = (crate, lint, api)
        if key in seen:
            raise AuditError(
                f"semver inventory {path} repeats break evidence "
                f"{crate}: {lint} / {api}"
            )
        if previous_key is not None and key < previous_key:
            raise AuditError(
                f"semver inventory {path} is not sorted at row {line_number}: "
                f"{crate}: {lint} / {api}"
            )
        seen.add(key)
        previous_key = key
        expected[crate].append(ExpectedFailure(lint=lint, api=api, reason=reason))

    return {crate: tuple(entries) for crate, entries in expected.items()}


def inventory_path(root: Path, relative_path: Path) -> Path:
    if relative_path.is_absolute():
        raise AuditError(f"semver inventory path must be repository-relative: {relative_path}")
    resolved_root = root.resolve()
    resolved_path = (resolved_root / relative_path).resolve()
    try:
        resolved_path.relative_to(resolved_root)
    except ValueError as error:
        raise AuditError(
            f"semver inventory path escapes the workspace: {relative_path}"
        ) from error
    return resolved_path


def select_release_profile(
    release_version: str,
    root: Path = REPO_ROOT,
) -> ReleaseProfile:
    try:
        template = RELEASE_PROFILES[release_version]
    except KeyError as error:
        supported = ", ".join(sorted(RELEASE_PROFILES))
        raise AuditError(
            f"unsupported release version {release_version}; supported versions: {supported}"
        ) from error
    expected_breaks = load_break_inventory(
        inventory_path(root, template.inventory_path),
        template.release_crates,
    )
    profile = replace(template, expected_breaks=expected_breaks)
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
    if not profile.release_crates:
        raise AuditError(f"release profile {profile.release_version} has no release crates")
    if len(set(profile.release_crates)) != len(profile.release_crates):
        raise AuditError(
            f"release profile {profile.release_version} repeats a release crate"
        )
    if set(profile.expected_breaks) != set(profile.release_crates):
        raise AuditError(
            f"release profile {profile.release_version} inventory crate set does not match "
            "its release crates"
        )

    seen: set[tuple[str, str, str]] = set()
    for crate, expected in profile.expected_breaks.items():
        if not crate:
            raise AuditError(f"release profile {profile.release_version} has an empty crate name")
        for item in expected:
            if not item.lint or not item.api or not item.reason:
                raise AuditError(
                    f"release profile {profile.release_version} has incomplete break evidence "
                    f"for {crate}"
                )
            key = (crate, item.lint, item.api)
            if key in seen:
                raise AuditError(
                    f"release profile {profile.release_version} repeats break evidence "
                    f"{crate}: {item.lint} / {item.api}"
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


def normalize_failure_evidence(lint: str, evidence: str) -> str:
    normalized = evidence.strip()
    match = SOURCE_LOCATION_RE.fullmatch(normalized)
    if match is not None:
        normalized = match.group("api").strip()
    if not normalized:
        raise AuditError(f"failure block {lint} has empty normalized API evidence")
    return normalized


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
        observed.extend(
            (lint, normalize_failure_evidence(lint, evidence))
            for evidence in failure_evidence(lint, block)
        )
    observed = list(dict.fromkeys(observed))

    expected_by_key = {(item.lint, item.api): item for item in expected}
    if len(expected_by_key) != len(expected):
        raise AuditError(f"expected semver inventory for {crate} contains duplicate evidence")

    matched: set[tuple[str, str]] = set()
    errors: list[str] = []
    for lint, evidence in observed:
        key = (lint, evidence)
        if key not in expected_by_key:
            errors.append(f"unexpected semver break in {crate}: {lint}: {evidence}")
            continue
        matched.add(key)

    for item in expected:
        if (item.lint, item.api) not in matched:
            errors.append(
                f"expected semver break not observed in {crate}: "
                f"{item.lint} / {item.api}"
            )

    if errors:
        raise AuditError("\n".join(errors))


def audit_crate(crate: str, profile: ReleaseProfile) -> None:
    expected = profile.expected_breaks[crate]
    tool_expected = tuple(
        item for item in expected if item.lint != MANUAL_REVIEW_LINT
    )
    manual_expected = tuple(
        item for item in expected if item.lint == MANUAL_REVIEW_LINT
    )
    completed = run_semver(crate, profile)
    validate_semver_result(crate, tool_expected, completed)
    if expected:
        lint_counts: dict[str, int] = {}
        for item in tool_expected:
            lint_counts[item.lint] = lint_counts.get(item.lint, 0) + 1
        tool_summary = ", ".join(
            f"{lint}={count}" for lint, count in sorted(lint_counts.items())
        )
        summary_parts = [
            f"matched {len(tool_expected)} cargo-semver-checks breaks"
        ]
        if tool_summary:
            summary_parts[-1] += f" ({tool_summary})"
        if manual_expected:
            summary_parts.append(
                f"recorded {len(manual_expected)} manually reviewed tool blind spots"
            )
        print(
            f"{crate}: {'; '.join(summary_parts)} for {profile.release_version} "
            f"from {profile.inventory_path}"
        )
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
        help="Validate the release profile and workspace without invoking Cargo.",
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
        root = args.workspace_root.resolve()
        profile = select_release_profile(args.release_version, root)
        validate_workspace_contract(root, profile)
        if args.validate_only:
            print(
                f"release {profile.release_version}: profile matches workspace "
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
