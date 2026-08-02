from __future__ import annotations

import importlib.util
import io
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = REPO_ROOT / "tools" / "audit_crate_packages.py"
SPEC = importlib.util.spec_from_file_location("audit_crate_packages", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
PACKAGE_AUDIT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PACKAGE_AUDIT
SPEC.loader.exec_module(PACKAGE_AUDIT)


class PackageAuditTests(unittest.TestCase):
    def test_valid_archives_match_the_release_contracts(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for name, contract in (
                ("boxddd-sys-0.4.0", PACKAGE_AUDIT.SYS_CONTRACT),
                ("boxddd-0.4.0", PACKAGE_AUDIT.CORE_CONTRACT),
                ("bevy_boxddd-0.4.0", PACKAGE_AUDIT.BEVY_CONTRACT),
            ):
                with self.subTest(crate=contract.crate):
                    archive = root / f"{name}.crate"
                    self.write_archive(archive, name, self.valid_files(contract))
                    PACKAGE_AUDIT.audit_archive(archive, contract)

    def test_sys_archive_requires_the_strict_ci_contract(self) -> None:
        contract = PACKAGE_AUDIT.SYS_CONTRACT
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for required in (
                "patches/0002-ccd-null-pre-solve-guard.patch",
                "src/upstream_contract.rs",
            ):
                with self.subTest(required=required):
                    files = self.valid_files(contract)
                    files.pop(required)
                    archive = root / "boxddd-sys-0.4.0.crate"
                    self.write_archive(archive, "boxddd-sys-0.4.0", files)
                    with self.assertRaisesRegex(
                        PACKAGE_AUDIT.AuditError,
                        required,
                    ):
                        PACKAGE_AUDIT.audit_archive(archive, contract)

            files = self.valid_files(contract)
            files["box3d-upstream.toml"] = b"\n".join(
                literal
                for literal in dict(contract.required_literals)["box3d-upstream.toml"]
                if literal != b"required_capabilities = ["
            )
            archive = root / "boxddd-sys-0.4.0.crate"
            self.write_archive(archive, "boxddd-sys-0.4.0", files)
            with self.assertRaisesRegex(
                PACKAGE_AUDIT.AuditError,
                "required_capabilities",
            ):
                PACKAGE_AUDIT.audit_archive(archive, contract)

    def test_bevy_archive_requires_transitive_example_modules(self) -> None:
        contract = PACKAGE_AUDIT.BEVY_CONTRACT
        required_modules = (
            "examples/support/mod.rs",
            "examples/testbed_3d/control.rs",
            "examples/testbed_3d/lab.rs",
            "examples/testbed_3d/picking.rs",
            "examples/testbed_3d/scene_catalog.rs",
            "examples/testbed_3d/ui.rs",
        )
        for required in required_modules:
            with self.subTest(required=required), tempfile.TemporaryDirectory() as temp_dir:
                files = self.valid_files(contract)
                files.pop(required)
                archive = Path(temp_dir) / "bevy_boxddd-0.4.0.crate"
                self.write_archive(archive, "bevy_boxddd-0.4.0", files)

                with self.assertRaises(PACKAGE_AUDIT.AuditError) as raised:
                    PACKAGE_AUDIT.audit_archive(archive, contract)

                self.assertIn(required, str(raised.exception))

    def test_wrong_archive_root_is_rejected(self) -> None:
        contract = PACKAGE_AUDIT.CORE_CONTRACT
        with tempfile.TemporaryDirectory() as temp_dir:
            archive = Path(temp_dir) / "boxddd-0.4.0.crate"
            self.write_archive(archive, "wrong-root", self.valid_files(contract))

            with self.assertRaisesRegex(
                PACKAGE_AUDIT.AuditError,
                "outside expected root",
            ):
                PACKAGE_AUDIT.audit_archive(archive, contract)

    def test_readme_marker_and_forbidden_paths_are_enforced(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            core_files = self.valid_files(PACKAGE_AUDIT.CORE_CONTRACT)
            core_files["README.md"] = b"missing release link\n"
            core_archive = root / "boxddd-0.4.0.crate"
            self.write_archive(core_archive, "boxddd-0.4.0", core_files)
            with self.assertRaisesRegex(PACKAGE_AUDIT.AuditError, "sample-matrix"):
                PACKAGE_AUDIT.audit_archive(
                    core_archive,
                    PACKAGE_AUDIT.CORE_CONTRACT,
                )

            sys_files = self.valid_files(PACKAGE_AUDIT.SYS_CONTRACT)
            sys_files["repo-ref/box3d/README.md"] = b"forbidden\n"
            sys_archive = root / "boxddd-sys-0.4.0.crate"
            self.write_archive(sys_archive, "boxddd-sys-0.4.0", sys_files)
            with self.assertRaisesRegex(PACKAGE_AUDIT.AuditError, "non-publishable"):
                PACKAGE_AUDIT.audit_archive(sys_archive, PACKAGE_AUDIT.SYS_CONTRACT)

    @staticmethod
    def valid_files(contract) -> dict[str, bytes]:
        files = {path: b"fixture\n" for path in contract.required_files}
        for path, literals in contract.required_literals:
            files[path] = b"\n".join(literals) + b"\n"
        return files

    @staticmethod
    def write_archive(
        archive_path: Path,
        archive_root: str,
        files: dict[str, bytes],
    ) -> None:
        with tarfile.open(archive_path, "w:gz") as archive:
            for relative, content in sorted(files.items()):
                member = tarfile.TarInfo(f"{archive_root}/{relative}")
                member.size = len(content)
                archive.addfile(member, io.BytesIO(content))


if __name__ == "__main__":
    unittest.main()
