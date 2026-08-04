from __future__ import annotations

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


class ReleaseDocumentationTests(unittest.TestCase):
    def test_packaged_migration_guide_matches_canonical_guide(self) -> None:
        canonical = REPO_ROOT / "docs" / "migrating-0.3-to-0.4.md"
        packaged = REPO_ROOT / "boxddd" / "MIGRATING-0.3-TO-0.4.md"

        self.assertEqual(
            packaged.read_bytes(),
            canonical.read_bytes(),
            "the packaged migration guide must match the canonical workspace guide",
        )


if __name__ == "__main__":
    unittest.main()
