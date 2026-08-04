#!/usr/bin/env python3
"""Audit generated Cargo package archives against the release contract."""

from __future__ import annotations

import argparse
import sys
import tarfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable


SAMPLE_MATRIX_LINK = b"docs/upstream-parity/box3d-sample-matrix.md"
FORBIDDEN_COMPONENTS = frozenset({".github", "repo-ref", "target"})
FORBIDDEN_BOX3D_DIRECTORIES = frozenset(
    {"benchmark", "docs", "extern", "samples", "test"}
)


class AuditError(RuntimeError):
    """A package archive does not match the release contract."""


@dataclass(frozen=True)
class ArchiveContract:
    crate: str
    required_files: tuple[str, ...]
    required_literals: tuple[tuple[str, tuple[bytes, ...]], ...] = ()
    forbid_box3d_development_tree: bool = False


SYS_CONTRACT = ArchiveContract(
    crate="boxddd-sys",
    required_files=(
        "README.md",
        "box3d-upstream.toml",
        "patches/0001-emscripten-single-thread-timer.patch",
        "patches/0002-ccd-null-pre-solve-guard.patch",
        "provider/debug_callbacks.c",
        "src/bindings_pregenerated.rs",
        "src/bindings_pregenerated_double.rs",
        "src/upstream_contract.rs",
        "third-party/box3d/LICENSE",
    ),
    required_literals=(
        (
            "box3d-upstream.toml",
            (
                b"[provider]",
                b'module = "box3d-sys-v2"',
                b"required_capabilities = [",
                b"intentionally_unsupported_capabilities = [",
                b"[capabilities]",
                b'inventory = "docs/upstream-parity/box3d-capability-inventory.json"',
            ),
        ),
    ),
    forbid_box3d_development_tree=True,
)

CORE_CONTRACT = ArchiveContract(
    crate="boxddd",
    required_files=(
        "MIGRATING-0.3-TO-0.4.md",
        "README.md",
        "examples/README.md",
        "examples/body_controls.rs",
        "examples/character_mover.rs",
        "examples/compound_query.rs",
        "examples/continuous_collision.rs",
        "examples/dynamic_tree.rs",
        "examples/events.rs",
        "examples/glam_interop.rs",
        "examples/mesh_height_field_query.rs",
        "examples/nalgebra_interop.rs",
        "tests/fixtures/api_coverage_symbols.txt",
    ),
    required_literals=(("README.md", (SAMPLE_MATRIX_LINK,)),),
)

BEVY_CONTRACT = ArchiveContract(
    crate="bevy_boxddd",
    required_files=(
        "README.md",
        "examples/README.md",
        "examples/advanced_colliders_3d.rs",
        "examples/joint_gallery_3d.rs",
        "examples/support/mod.rs",
        "examples/testbed_3d/control.rs",
        "examples/testbed_3d/lab.rs",
        "examples/testbed_3d/main.rs",
        "examples/testbed_3d/picking.rs",
        "examples/testbed_3d/scene_catalog.rs",
        "examples/testbed_3d/scenes.rs",
        "examples/testbed_3d/ui.rs",
    ),
    required_literals=(("README.md", (SAMPLE_MATRIX_LINK,)),),
)


def archive_root(archive: Path) -> str:
    suffix = ".crate"
    if not archive.name.endswith(suffix):
        raise AuditError(f"package archive must end in {suffix}: {archive}")
    return archive.name[: -len(suffix)]


def relative_archive_path(member_name: str, expected_root: str) -> str | None:
    path = PurePosixPath(member_name)
    if path.is_absolute() or ".." in path.parts:
        raise AuditError(f"archive contains unsafe path: {member_name}")
    if not path.parts or path.parts[0] != expected_root:
        raise AuditError(
            f"archive entry {member_name!r} is outside expected root {expected_root!r}"
        )
    if len(path.parts) == 1:
        return None
    return PurePosixPath(*path.parts[1:]).as_posix()


def forbidden_path(path: str, contract: ArchiveContract) -> bool:
    parts = PurePosixPath(path).parts
    if any(part in FORBIDDEN_COMPONENTS for part in parts):
        return True
    if parts[:2] == ("docs", "plans"):
        return True
    return (
        contract.forbid_box3d_development_tree
        and parts[:2] == ("third-party", "box3d")
        and len(parts) >= 3
        and parts[2] in FORBIDDEN_BOX3D_DIRECTORIES
    )


def read_member(
    archive: tarfile.TarFile,
    members: dict[str, tarfile.TarInfo],
    path: str,
) -> bytes:
    extracted = archive.extractfile(members[path])
    if extracted is None:
        raise AuditError(f"could not read package file {path}")
    return extracted.read()


def audit_archive(path: Path, contract: ArchiveContract) -> None:
    expected_root = archive_root(path)
    with tarfile.open(path, "r:*") as archive:
        files: dict[str, tarfile.TarInfo] = {}
        all_paths: list[str] = []
        for member in archive.getmembers():
            relative = relative_archive_path(member.name, expected_root)
            if relative is None:
                continue
            all_paths.append(relative)
            if member.isfile():
                if relative in files:
                    raise AuditError(f"{path} contains duplicate file {relative}")
                files[relative] = member

        forbidden = sorted(
            relative for relative in all_paths if forbidden_path(relative, contract)
        )
        if forbidden:
            raise AuditError(
                f"{path} contains non-publishable content: {', '.join(forbidden)}"
            )

        missing = sorted(set(contract.required_files) - files.keys())
        if missing:
            raise AuditError(f"{path} is missing required files: {', '.join(missing)}")

        for member_path, literals in contract.required_literals:
            content = read_member(archive, files, member_path)
            for literal in literals:
                if literal not in content:
                    display = literal.decode("utf-8", errors="replace")
                    raise AuditError(
                        f"{path} file {member_path} is missing required text: {display}"
                    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--sys", type=Path, required=True, help="boxddd-sys .crate archive"
    )
    parser.add_argument(
        "--core", type=Path, required=True, help="boxddd .crate archive"
    )
    parser.add_argument(
        "--bevy", type=Path, required=True, help="bevy_boxddd .crate archive"
    )
    return parser


def main(argv: Iterable[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    archives = (
        (args.sys, SYS_CONTRACT),
        (args.core, CORE_CONTRACT),
        (args.bevy, BEVY_CONTRACT),
    )
    try:
        for path, contract in archives:
            audit_archive(path, contract)
            print(f"{path}: {contract.crate} package contents are valid")
    except (AuditError, OSError, tarfile.TarError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
