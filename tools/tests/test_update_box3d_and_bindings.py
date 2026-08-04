from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


REPO_ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = REPO_ROOT / "tools" / "update_box3d_and_bindings.py"
SPEC = importlib.util.spec_from_file_location("update_box3d_and_bindings", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
UPDATE_TOOL = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = UPDATE_TOOL
SPEC.loader.exec_module(UPDATE_TOOL)


class UpdateBox3dAndBindingsTests(unittest.TestCase):
    def test_checkout_source_materializes_the_exact_detached_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source, commit = self.create_git_fixture(root)
            destination = root / "checkout"
            contract = SimpleNamespace(repository=str(source), commit=commit)

            UPDATE_TOOL.checkout_source(contract, destination)

            self.assertEqual(
                self.run_git(destination, "rev-parse", "HEAD").stdout.strip(),
                commit,
            )
            self.assertEqual(
                self.run_git(destination, "remote", "get-url", "origin").stdout.strip(),
                str(source),
            )

    def test_checkout_source_rejects_an_existing_destination(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source, commit = self.create_git_fixture(root)
            destination = root / "checkout"
            destination.mkdir()
            contract = SimpleNamespace(repository=str(source), commit=commit)

            with self.assertRaisesRegex(
                UPDATE_TOOL.ContractError,
                "checkout destination already exists",
            ):
                UPDATE_TOOL.checkout_source(contract, destination)

    def test_checkout_source_cleans_temporary_directory_after_git_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source, commit = self.create_git_fixture(root)
            destination = root / "checkout"
            broken_contract = SimpleNamespace(
                repository=str(root / "missing-upstream"),
                commit=commit,
            )

            with self.assertRaises(subprocess.CalledProcessError):
                UPDATE_TOOL.checkout_source(broken_contract, destination)

            self.assertFalse(destination.exists())
            self.assertEqual(list(root.glob(".checkout-*")), [])

            contract = SimpleNamespace(repository=str(source), commit=commit)
            UPDATE_TOOL.checkout_source(contract, destination)
            self.assertTrue(destination.is_dir())

    def test_source_verification_rejects_remote_and_commit_drift(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source, commit = self.create_git_fixture(root)
            destination = root / "checkout"
            contract = SimpleNamespace(repository=str(source), commit=commit)
            UPDATE_TOOL.checkout_source(contract, destination)

            wrong_remote = root / "wrong-remote"
            wrong_remote.mkdir()
            self.run_git(destination, "remote", "set-url", "origin", str(wrong_remote))
            with self.assertRaisesRegex(
                UPDATE_TOOL.ContractError,
                "upstream repository mismatch",
            ):
                UPDATE_TOOL.verify_source_checkout(destination, contract)

            self.run_git(destination, "remote", "set-url", "origin", str(source))
            wrong_commit = SimpleNamespace(repository=str(source), commit="0" * 40)
            with self.assertRaisesRegex(UPDATE_TOOL.ContractError, "git command failed"):
                UPDATE_TOOL.verify_source_checkout(destination, wrong_commit)

    def test_materialization_reads_commit_objects_not_dirty_worktree_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            source = root / "upstream"
            destination = root / "materialized"
            source.mkdir()
            self.run_git(source, "init")
            self.run_git(source, "config", "user.email", "test@example.com")
            self.run_git(source, "config", "user.name", "Test User")
            header = source / "include" / "box3d" / "box3d.h"
            header.parent.mkdir(parents=True)
            header.write_text("committed\n", encoding="utf-8")
            self.run_git(source, "add", "include/box3d/box3d.h")
            self.run_git(source, "commit", "-m", "fixture")
            commit = self.run_git(source, "rev-parse", "HEAD").stdout.strip()

            header.write_text("dirty\n", encoding="utf-8")
            UPDATE_TOOL.materialize_git_paths(
                source,
                commit,
                ("include/box3d/box3d.h",),
                destination,
            )

            self.assertEqual(
                (destination / "include" / "box3d" / "box3d.h").read_text(
                    encoding="utf-8"
                ),
                "committed\n",
            )

    def test_materialization_rejects_paths_outside_the_vendor_root(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            with self.assertRaises(UPDATE_TOOL.ContractError):
                UPDATE_TOOL.materialize_git_paths(
                    root,
                    "0" * 40,
                    ("../escape",),
                    root / "destination",
                )

    def test_patch_application_is_isolated_from_a_parent_git_worktree(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            self.run_git(root, "init")
            parent_file = root / "source.txt"
            parent_file.write_text("parent\n", encoding="utf-8")

            destination = root / "temporary" / "vendor"
            destination.mkdir(parents=True)
            destination_file = destination / "source.txt"
            destination_file.write_text("before\n", encoding="utf-8")

            patch = root / "change.patch"
            patch.write_text(
                """diff --git a/source.txt b/source.txt
--- a/source.txt
+++ b/source.txt
@@ -1 +1 @@
-before
+after
""",
                encoding="utf-8",
            )
            contract = SimpleNamespace(
                patches=(
                    UPDATE_TOOL.PatchSpec(
                        path=patch,
                        sha256=UPDATE_TOOL.file_sha256(patch),
                    ),
                )
            )

            UPDATE_TOOL.apply_declared_patches(contract, destination)

            self.assertEqual(destination_file.read_text(encoding="utf-8"), "after\n")
            self.assertEqual(parent_file.read_text(encoding="utf-8"), "parent\n")

    def test_tree_fingerprint_is_order_independent_and_content_sensitive(self) -> None:
        first = {
            "include/box3d/box3d.h": b"header\n",
            "src/core.c": b"source\n",
        }
        second = dict(reversed(tuple(first.items())))
        changed = dict(first)
        changed["src/core.c"] = b"changed\n"

        self.assertEqual(
            UPDATE_TOOL.fingerprint_entries(first.items()),
            UPDATE_TOOL.fingerprint_entries(second.items()),
        )
        self.assertNotEqual(
            UPDATE_TOOL.fingerprint_entries(first.items()),
            UPDATE_TOOL.fingerprint_entries(changed.items()),
        )

    def test_official_sample_parser_includes_samples_and_replays(self) -> None:
        sources = {
            "samples/sample_world.cpp": (
                'RegisterSample( "World", "Falling Box", CreateFallingBox );\n'
            ),
            "samples/sample_replay.cpp": (
                'RegisterReplay( "Replay", "Viewer", CreateReplay );\n'
            ),
        }

        samples = UPDATE_TOOL.parse_official_samples(sources)

        self.assertEqual(
            [
                (sample.category, sample.name, sample.source)
                for sample in samples
            ],
            [
                ("Replay", "Viewer", "sample_replay.cpp:1"),
                ("World", "Falling Box", "sample_world.cpp:1"),
            ],
        )

    def test_bound_function_inventory_ignores_struct_methods(self) -> None:
        bindings = '''
impl b3Plane {
    pub fn axis(&self) -> &b3Vec3 {
        todo!()
    }
}
unsafe extern "C" {
    pub fn b3CreateWorld(def: *const b3WorldDef) -> b3WorldId;
    pub fn b3DestroyWorld(world_id: b3WorldId);
}
'''

        self.assertEqual(
            UPDATE_TOOL.extract_bound_functions(bindings),
            ("b3CreateWorld", "b3DestroyWorld"),
        )

    def test_captured_command_failure_preserves_stderr(self) -> None:
        with self.assertRaisesRegex(UPDATE_TOOL.ContractError, "fixture diagnostic"):
            UPDATE_TOOL.run_command(
                [
                    sys.executable,
                    "-c",
                    "import sys; sys.stderr.write('fixture diagnostic'); sys.exit(2)",
                ],
                cwd=REPO_ROOT,
                capture_output=True,
            )

    @staticmethod
    def create_git_fixture(root: Path) -> tuple[Path, str]:
        source = root / "upstream"
        source.mkdir()
        UpdateBox3dAndBindingsTests.run_git(source, "init")
        UpdateBox3dAndBindingsTests.run_git(
            source, "config", "user.email", "test@example.com"
        )
        UpdateBox3dAndBindingsTests.run_git(source, "config", "user.name", "Test User")
        source.joinpath("README.md").write_text("fixture\n", encoding="utf-8")
        UpdateBox3dAndBindingsTests.run_git(source, "add", "README.md")
        UpdateBox3dAndBindingsTests.run_git(source, "commit", "-m", "fixture")
        commit = UpdateBox3dAndBindingsTests.run_git(
            source, "rev-parse", "HEAD"
        ).stdout.strip()
        return source, commit

    @staticmethod
    def run_git(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=cwd,
            check=True,
            capture_output=True,
            text=True,
        )


if __name__ == "__main__":
    unittest.main()
