from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = REPO_ROOT / "tools" / "semver_audit.py"
SPEC = importlib.util.spec_from_file_location("semver_audit", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
SEMVER_AUDIT = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = SEMVER_AUDIT
SPEC.loader.exec_module(SEMVER_AUDIT)


def failure_output(lint: str, *evidence: str) -> str:
    details = "\n".join(f"  {item}" for item in evidence)
    return (
        f"--- failure {lint}: fixture failure ---\n\n"
        "Description:\nfixture description\n\n"
        f"Failed in:\n{details}\n"
    )


class ReleaseProfileTests(unittest.TestCase):
    def test_selects_the_exact_0_3_profile(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")

        self.assertEqual(profile.release_version, "0.3.0")
        self.assertEqual(profile.baseline_rev, "v0.2.0")
        self.assertEqual(profile.release_type, "minor")
        self.assertEqual(
            tuple(sorted(profile.expected_breaks)),
            ("bevy_boxddd", "boxddd", "boxddd-sys"),
        )

    def test_unknown_release_version_fails_before_running_cargo(self) -> None:
        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "unsupported release version 0.4.0",
        ):
            SEMVER_AUDIT.select_release_profile("0.4.0")

    def test_semver_command_comes_only_from_the_profile(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")

        self.assertEqual(
            SEMVER_AUDIT.semver_command("boxddd", profile),
            [
                "cargo",
                "semver-checks",
                "check-release",
                "-p",
                "boxddd",
                "--baseline-rev",
                "v0.2.0",
                "--release-type",
                "minor",
            ],
        )

    def test_cargo_process_start_failure_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        with mock.patch.object(
            SEMVER_AUDIT.subprocess,
            "run",
            side_effect=FileNotFoundError("cargo not found"),
        ):
            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "could not run cargo-semver-checks for boxddd.*cargo not found",
            ):
                SEMVER_AUDIT.run_semver("boxddd", profile)

    def test_validate_only_does_not_run_semver(self) -> None:
        with mock.patch.object(SEMVER_AUDIT, "run_semver") as run_semver:
            result = SEMVER_AUDIT.main(
                [
                    "--release-version",
                    "0.3.0",
                    "--validate-only",
                    "--workspace-root",
                    str(REPO_ROOT),
                ]
            )

        self.assertEqual(result, 0)
        run_semver.assert_not_called()


class WorkspaceContractTests(unittest.TestCase):
    def test_repository_workspace_matches_the_0_3_profile(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")

        SEMVER_AUDIT.validate_workspace_contract(REPO_ROOT, profile)

    def test_workspace_version_mismatch_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.2.0")

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "workspace version 0.2.0 does not match release profile 0.3.0",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_member_version_mismatch_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.3.0")
            (root / "boxddd" / "Cargo.toml").write_text(
                '[package]\nname = "boxddd"\nversion = "9.9.9"\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "crate boxddd version 9.9.9 does not match release profile 0.3.0",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_workspace_dependency_version_mismatch_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.3.0")
            root.joinpath("Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.3.0"\n'
                '[workspace.dependencies]\nboxddd = { version = "0.2.0" }\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "workspace dependency boxddd version 0.2.0 does not match release profile 0.3.0",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_workspace_dependency_without_version_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.3.0")
            root.joinpath("Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.3.0"\n'
                '[workspace.dependencies]\nboxddd = { path = "boxddd" }\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "workspace dependency boxddd has no valid release version",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_missing_published_crate_manifest_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.3.0")
            (root / "bevy_boxddd" / "Cargo.toml").unlink()

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "missing manifest for release crate bevy_boxddd",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    @staticmethod
    def write_workspace(root: Path, version: str) -> Path:
        root.joinpath("Cargo.toml").write_text(
            f'[workspace.package]\nversion = "{version}"\n',
            encoding="utf-8",
        )
        for crate in ("boxddd", "boxddd-sys", "bevy_boxddd"):
            crate_root = root / crate
            crate_root.mkdir()
            crate_root.joinpath("Cargo.toml").write_text(
                f'[package]\nname = "{crate}"\nversion.workspace = true\n',
                encoding="utf-8",
            )
        return root


class BreakInventoryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.expected = (
            SEMVER_AUDIT.ExpectedFailure(
                lint="function_missing",
                needle="function boxddd::legacy_api",
                reason="fixture",
            ),
        )

    def test_exact_known_break_is_accepted(self) -> None:
        output = failure_output(
            "function_missing",
            "function boxddd::legacy_api, previously in file boxddd/src/lib.rs:10",
        )

        SEMVER_AUDIT.validate_semver_result(
            "boxddd",
            self.expected,
            subprocess.CompletedProcess([], 1, stdout=output),
        )

    def test_unknown_break_is_fatal(self) -> None:
        output = failure_output(
            "function_missing",
            "function boxddd::surprise, previously in file boxddd/src/lib.rs:11",
        )

        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "unexpected semver break in boxddd.*boxddd::surprise",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                self.expected,
                subprocess.CompletedProcess([], 1, stdout=output),
            )

    def test_unknown_evidence_in_a_known_lint_block_is_fatal(self) -> None:
        output = failure_output(
            "function_missing",
            "function boxddd::legacy_api, previously in file boxddd/src/lib.rs:10",
            "function boxddd::surprise, previously in file boxddd/src/lib.rs:11",
        )

        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "unexpected semver break in boxddd.*boxddd::surprise",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                self.expected,
                subprocess.CompletedProcess([], 1, stdout=output),
            )

    def test_missing_expected_break_is_fatal(self) -> None:
        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "expected semver break not observed in boxddd.*boxddd::legacy_api",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                self.expected,
                subprocess.CompletedProcess([], 0, stdout="semver check passed\n"),
            )

    def test_non_semver_tool_failure_is_fatal(self) -> None:
        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "failed for boxddd without parseable semver failure evidence",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                self.expected,
                subprocess.CompletedProcess([], 2, stdout="network unavailable\n"),
            )

    def test_failure_block_without_parseable_evidence_is_fatal(self) -> None:
        output = "--- failure function_missing: fixture failure ---\nmissing details\n"

        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "failure block function_missing has no parseable API evidence",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                self.expected,
                subprocess.CompletedProcess([], 1, stdout=output),
            )

    def test_ansi_colored_failure_output_is_parsed(self) -> None:
        output = failure_output(
            "function_missing",
            "function boxddd::legacy_api, previously in file boxddd/src/lib.rs:10",
        )
        output = f"\x1b[31m{output}\x1b[0m"

        SEMVER_AUDIT.validate_semver_result(
            "boxddd",
            self.expected,
            subprocess.CompletedProcess([], 1, stdout=output),
        )


class WorkflowContractTests(unittest.TestCase):
    def test_release_workflow_delegates_to_the_profile_without_a_skip_condition(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        workflow = (REPO_ROOT / ".github/workflows/release-preflight.yml").read_text(
            encoding="utf-8"
        )

        SEMVER_AUDIT.validate_workflow_contract(workflow, profile)

    def test_workflow_profile_drift_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.3.0")
        valid = '''
      - name: Validate release semver profile
        run: python3 tools/semver_audit.py --release-version "${{ inputs.version }}" --validate-only

      - name: Install cargo-semver-checks
        run: cargo install cargo-semver-checks --version 0.48.0 --locked

      - name: Audit release semver breaks
        run: python3 tools/semver_audit.py --release-version "${{ inputs.version }}"
'''
        validation_step, remaining_steps = valid.split("\n\n", 1)
        reordered = f"{remaining_steps}\n\n{validation_step}"

        for label, mutated in (
            (
                "skip condition",
                valid.replace(
                    "        run: cargo install",
                    "        if: ${{ inputs.version == '0.3.0' }}\n"
                    "        run: cargo install",
                ),
            ),
            (
                "baseline",
                valid.replace(
                    " --release-version",
                    " --baseline-rev v0.1.0 --release-version",
                ),
            ),
            (
                "release type",
                valid.replace(
                    " --release-version",
                    " --release-type major --release-version",
                ),
            ),
            (
                "duplicate workspace version check",
                valid
                + "\n      - name: Verify Cargo workspace version matches input\n"
                + "        run: grep version Cargo.toml\n",
            ),
            ("validation order", reordered),
        ):
            with self.subTest(label=label), self.assertRaises(
                SEMVER_AUDIT.AuditError
            ):
                SEMVER_AUDIT.validate_workflow_contract(mutated, profile)


if __name__ == "__main__":
    unittest.main()
