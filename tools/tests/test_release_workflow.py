from __future__ import annotations

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "release-crates.yml"


class ReleaseWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = WORKFLOW_PATH.read_text(encoding="utf-8")
        cls.preflight, cls.publish = cls.source.split("\n  publish:\n", maxsplit=1)

    def test_publish_checkout_uses_the_preflight_commit(self) -> None:
        self.assertIn(
            "    outputs:\n      commit: ${{ steps.version.outputs.commit }}",
            self.preflight,
        )
        checkout, _ = self.publish.split(
            "\n      - name: Read version from tag", maxsplit=1
        )
        self.assertIn("          ref: ${{ needs.preflight.outputs.commit }}", checkout)
        self.assertNotIn("inputs.source_ref", checkout)

    def test_publish_revalidates_the_tag_against_the_preflight_commit(self) -> None:
        self.assertIn(
            "          EXPECTED_COMMIT: ${{ needs.preflight.outputs.commit }}",
            self.publish,
        )
        self.assertIn('[ "$head_commit" != "$EXPECTED_COMMIT" ]', self.publish)
        self.assertIn('[ "$tag_commit" != "$EXPECTED_COMMIT" ]', self.publish)


if __name__ == "__main__":
    unittest.main()
