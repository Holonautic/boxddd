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


def inventory_text(*rows: tuple[str, str, str, str]) -> str:
    lines = ["crate\tlint\tapi\treason"]
    lines.extend("\t".join(row) for row in rows)
    return "\n".join(lines) + "\n"


class ReleaseProfileTests(unittest.TestCase):
    def test_selects_the_exact_0_4_profile(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")

        self.assertEqual(tuple(sorted(SEMVER_AUDIT.RELEASE_PROFILES)), ("0.4.0",))
        self.assertEqual(profile.release_version, "0.4.0")
        self.assertEqual(profile.baseline_rev, "v0.3.0")
        self.assertEqual(profile.release_type, "minor")
        self.assertEqual(
            profile.inventory_path,
            Path("tools/semver/0.4.0.tsv"),
        )
        self.assertEqual(
            tuple(sorted(profile.expected_breaks)),
            ("bevy_boxddd", "boxddd", "boxddd-sys"),
        )
        manual_breaks = [
            (crate, item.api)
            for crate, items in profile.expected_breaks.items()
            for item in items
            if item.lint == SEMVER_AUDIT.MANUAL_REVIEW_LINT
        ]
        self.assertEqual(
            manual_breaks,
            [
                ("bevy_boxddd", "impl Default for BoxdddPhysicsPlugin removed"),
                (
                    "boxddd",
                    "function boxddd::prelude::version return type Version -> Result<Version>",
                ),
                (
                    "boxddd",
                    "function boxddd::version return type Version -> Result<Version>",
                ),
                (
                    "boxddd",
                    "function boxddd::world::version return type Version -> Result<Version>",
                ),
                ("boxddd", "impl Default for BodyDef removed"),
                ("boxddd", "impl Default for BodyDefBuilder removed"),
                ("boxddd", "impl Default for ShapeDef removed"),
                ("boxddd", "impl Default for ShapeDefBuilder removed"),
                ("boxddd", "impl Default for WorldDef removed"),
                ("boxddd", "impl Default for WorldDefBuilder removed"),
                (
                    "boxddd",
                    "method Aabb::is_bounded return type bool -> Result<bool>",
                ),
                (
                    "boxddd",
                    "method Aabb::is_sane return type bool -> Result<bool>",
                ),
                (
                    "boxddd",
                    "method DynamicTree::contains_proxy return type bool -> Result<bool>",
                ),
                ("boxddd", "method World::contains_body return type bool -> Result<bool>"),
                (
                    "boxddd",
                    "method World::contains_contact return type bool -> Result<bool>",
                ),
                ("boxddd", "method World::contains_joint return type bool -> Result<bool>"),
                ("boxddd", "method World::contains_shape return type bool -> Result<bool>"),
            ],
        )

    def test_manual_breaks_are_not_expected_from_cargo_semver_checks(self) -> None:
        profile = SEMVER_AUDIT.ReleaseProfile(
            release_version="0.4.0",
            baseline_rev="v0.3.0",
            release_type="minor",
            inventory_path=Path("tools/semver/fixture.tsv"),
            release_crates=("boxddd",),
            expected_breaks={
                "boxddd": (
                    SEMVER_AUDIT.ExpectedFailure(
                        lint=SEMVER_AUDIT.MANUAL_REVIEW_LINT,
                        api="method World::contains_body return type bool -> Result<bool>",
                        reason="result_first_api",
                    ),
                )
            },
        )
        completed = subprocess.CompletedProcess([], 0, stdout="")

        with mock.patch.object(SEMVER_AUDIT, "run_semver", return_value=completed), mock.patch(
            "builtins.print"
        ) as output:
            SEMVER_AUDIT.audit_crate("boxddd", profile)

        output.assert_called_once_with(
            "boxddd: matched 0 cargo-semver-checks breaks; recorded 1 manually "
            "reviewed tool blind spots for 0.4.0 from tools/semver/fixture.tsv"
        )

    def test_unknown_release_version_fails_before_running_cargo(self) -> None:
        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "unsupported release version 0.3.0",
        ):
            SEMVER_AUDIT.select_release_profile("0.3.0")

    def test_semver_command_comes_only_from_the_profile(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")

        self.assertEqual(
            SEMVER_AUDIT.semver_command("boxddd", profile),
            [
                "cargo",
                "semver-checks",
                "check-release",
                "-p",
                "boxddd",
                "--baseline-rev",
                "v0.3.0",
                "--release-type",
                "minor",
            ],
        )

    def test_cargo_process_start_failure_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")
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
                    "0.4.0",
                    "--validate-only",
                    "--workspace-root",
                    str(REPO_ROOT),
                ]
            )

        self.assertEqual(result, 0)
        run_semver.assert_not_called()

    def test_validate_only_does_not_depend_on_a_workflow_file(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            root.joinpath("Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.4.0"\n',
                encoding="utf-8",
            )
            for crate in ("boxddd", "boxddd-sys", "bevy_boxddd"):
                crate_root = root / crate
                crate_root.mkdir()
                crate_root.joinpath("Cargo.toml").write_text(
                    f'[package]\nname = "{crate}"\nversion.workspace = true\n',
                    encoding="utf-8",
                )
            inventory = root / "tools" / "semver" / "0.4.0.tsv"
            inventory.parent.mkdir(parents=True)
            inventory.write_text(
                "crate\tlint\tapi\treason\n",
                encoding="utf-8",
            )

            with mock.patch.object(SEMVER_AUDIT, "run_semver") as run_semver:
                result = SEMVER_AUDIT.main(
                    [
                        "--release-version",
                        "0.4.0",
                        "--validate-only",
                        "--workspace-root",
                        str(root),
                    ]
                )

            self.assertEqual(result, 0)
            self.assertFalse(root.joinpath(".github").exists())
            run_semver.assert_not_called()


class InventoryArtifactTests(unittest.TestCase):
    def test_missing_inventory_is_fatal(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "missing.tsv"

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "missing semver inventory",
            ):
                SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

    def test_precise_reexports_are_loaded_as_independent_entries(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "0.4.0.tsv"
            path.write_text(
                inventory_text(
                    (
                        "boxddd",
                        "struct_missing",
                        "struct boxddd::DebugShape",
                        "debug_draw_assets",
                    ),
                    (
                        "boxddd",
                        "struct_missing",
                        "struct boxddd::prelude::DebugShape",
                        "debug_draw_assets",
                    ),
                ),
                encoding="utf-8",
            )

            inventory = SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

        self.assertEqual(
            tuple(item.api for item in inventory["boxddd"]),
            (
                "struct boxddd::DebugShape",
                "struct boxddd::prelude::DebugShape",
            ),
        )

    def test_duplicate_inventory_api_is_fatal(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "0.4.0.tsv"
            path.write_text(
                inventory_text(
                    (
                        "boxddd",
                        "function_missing",
                        "function boxddd::legacy_api",
                        "result_first_api",
                    ),
                    (
                        "boxddd",
                        "function_missing",
                        "function boxddd::legacy_api",
                        "raw_data_encapsulation",
                    ),
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "repeats break evidence.*boxddd.*function_missing.*legacy_api",
            ):
                SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

    def test_unknown_inventory_crate_is_fatal(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "0.4.0.tsv"
            path.write_text(
                inventory_text(
                    (
                        "surprise",
                        "function_missing",
                        "function surprise::legacy_api",
                        "result_first_api",
                    ),
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "unknown release crate surprise",
            ):
                SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

    def test_unsorted_inventory_is_fatal(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "0.4.0.tsv"
            path.write_text(
                inventory_text(
                    (
                        "boxddd",
                        "struct_missing",
                        "struct boxddd::Legacy",
                        "result_first_api",
                    ),
                    (
                        "boxddd",
                        "function_missing",
                        "function boxddd::legacy",
                        "result_first_api",
                    ),
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "is not sorted at row 3",
            ):
                SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

    def test_unknown_reason_category_is_fatal(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "0.4.0.tsv"
            path.write_text(
                inventory_text(
                    (
                        "boxddd",
                        "function_missing",
                        "function boxddd::legacy_api",
                        "unreviewed_reason",
                    ),
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "unknown reason category unreviewed_reason",
            ):
                SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

    def test_malformed_inventory_is_fatal(self) -> None:
        malformed = (
            "crate,lint,api,reason\n"
            "boxddd,function_missing,function boxddd::legacy_api,fixture\n"
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "0.4.0.tsv"
            path.write_text(malformed, encoding="utf-8")

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "invalid semver inventory header",
            ):
                SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))

    def test_wildcard_or_lint_only_inventory_is_fatal(self) -> None:
        for label, api in (("wildcard", "*"), ("lint only", "")):
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temp_dir:
                path = Path(temp_dir) / "0.4.0.tsv"
                path.write_text(
                    inventory_text(
                        (
                            "boxddd",
                            "function_missing",
                            api,
                            "result_first_api",
                        ),
                    ),
                    encoding="utf-8",
                )

                with self.assertRaisesRegex(
                    SEMVER_AUDIT.AuditError,
                    "invalid semver inventory row 2",
                ):
                    SEMVER_AUDIT.load_break_inventory(path, ("boxddd",))


class WorkspaceContractTests(unittest.TestCase):
    def test_repository_workspace_matches_the_0_4_profile(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")

        SEMVER_AUDIT.validate_workspace_contract(REPO_ROOT, profile)

    def test_workspace_version_mismatch_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.2.0")

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "workspace version 0.2.0 does not match release profile 0.4.0",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_member_version_mismatch_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.4.0")
            (root / "boxddd" / "Cargo.toml").write_text(
                '[package]\nname = "boxddd"\nversion = "9.9.9"\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "crate boxddd version 9.9.9 does not match release profile 0.4.0",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_workspace_dependency_version_mismatch_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.4.0")
            root.joinpath("Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.4.0"\n'
                '[workspace.dependencies]\nboxddd = { version = "0.2.0" }\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "workspace dependency boxddd version 0.2.0 does not match release profile 0.4.0",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_workspace_dependency_without_version_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.4.0")
            root.joinpath("Cargo.toml").write_text(
                '[workspace.package]\nversion = "0.4.0"\n'
                '[workspace.dependencies]\nboxddd = { path = "boxddd" }\n',
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                SEMVER_AUDIT.AuditError,
                "workspace dependency boxddd has no valid release version",
            ):
                SEMVER_AUDIT.validate_workspace_contract(root, profile)

    def test_missing_published_crate_manifest_is_fatal(self) -> None:
        profile = SEMVER_AUDIT.select_release_profile("0.4.0")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = self.write_workspace(Path(temp_dir), "0.4.0")
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
                api="function boxddd::legacy_api",
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

    def test_identical_tool_evidence_from_multiple_feature_runs_is_deduplicated(self) -> None:
        block = failure_output(
            "function_missing",
            "function boxddd::legacy_api, previously in file boxddd/src/lib.rs:10",
        )

        SEMVER_AUDIT.validate_semver_result(
            "boxddd",
            self.expected,
            subprocess.CompletedProcess([], 1, stdout=block * 3),
        )

    def test_identical_normalized_evidence_from_different_build_paths_is_deduplicated(self) -> None:
        output = failure_output(
            "function_missing",
            "function boxddd::legacy_api, previously in file /tmp/first/src/lib.rs:10",
            "function boxddd::legacy_api, previously in file C:/tmp/second/src/lib.rs:99",
        )

        SEMVER_AUDIT.validate_semver_result(
            "boxddd",
            self.expected,
            subprocess.CompletedProcess([], 1, stdout=output),
        )

    def test_current_source_location_is_removed_from_exact_evidence(self) -> None:
        expected = (
            SEMVER_AUDIT.ExpectedFailure(
                lint="constructible_struct_adds_field",
                api="field DebugDrawOptions.draw_sleep",
                reason="fixture",
            ),
        )
        output = failure_output(
            "constructible_struct_adds_field",
            "field DebugDrawOptions.draw_sleep in /tmp/debug_draw.rs:435",
        )

        SEMVER_AUDIT.validate_semver_result(
            "boxddd",
            expected,
            subprocess.CompletedProcess([], 1, stdout=output),
        )

    def test_partial_api_prefix_does_not_match(self) -> None:
        expected = (
            SEMVER_AUDIT.ExpectedFailure(
                lint="function_missing",
                api="function boxddd::legacy",
                reason="fixture",
            ),
        )
        output = failure_output(
            "function_missing",
            "function boxddd::legacy_api, previously in file boxddd/src/lib.rs:10",
        )

        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "unexpected semver break in boxddd.*boxddd::legacy_api",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                expected,
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

    def test_missing_reexport_is_fatal(self) -> None:
        expected = (
            SEMVER_AUDIT.ExpectedFailure(
                lint="struct_missing",
                api="struct boxddd::DebugShape",
                reason="fixture",
            ),
            SEMVER_AUDIT.ExpectedFailure(
                lint="struct_missing",
                api="struct boxddd::prelude::DebugShape",
                reason="fixture",
            ),
        )
        output = failure_output(
            "struct_missing",
            "struct boxddd::DebugShape, previously in file boxddd/src/lib.rs:10",
        )

        with self.assertRaisesRegex(
            SEMVER_AUDIT.AuditError,
            "expected semver break not observed in boxddd.*prelude::DebugShape",
        ):
            SEMVER_AUDIT.validate_semver_result(
                "boxddd",
                expected,
                subprocess.CompletedProcess([], 1, stdout=output),
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


if __name__ == "__main__":
    unittest.main()
