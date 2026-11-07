"""Integration checks for the offline presentation script (no PostgreSQL needed)."""
import csv
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest


SOURCE = Path(__file__).resolve().parents[1] / "assets" / "presentation" / "inspect.py"


class PresentationInspectorTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "data").mkdir()
        (self.root / "inspect.py").write_bytes(SOURCE.read_bytes())
        bundle = {
            "analysis_id": "00000000-0000-0000-0000-000000000003",
            "input_revision": 3,
            "goal": "What happened?",
            "answer_text": "One recorded interpretation, not proven.",
            "coverage_note": "Budget ran out; some sources omitted.",
        }
        (self.root / "data" / "analysis.json").write_text(
            json.dumps({"format": "niccolo.presentation-analysis",
                        "format_version": 1, "bundle": bundle}), encoding="utf-8"
        )
        for name, header, rows in [
            ("claims.csv", ["claim_id", "kind", "text", "qualification"],
             [["c1", "HYPOTHESIS", "Unverified interpretation", "uncertain"]]),
            ("claim_fragments.csv", ["claim_id", "evidence_id", "relation"],
             [["c1", "e1", "SUPPORTS"], ["c1", "e2", "CONTRADICTS"]]),
            ("claim_contributions.csv", ["claim_id", "contribution_id", "relation"],
             [["c1", "k1", "CONTEXT"]]),
        ]:
            with (self.root / "data" / name).open(
                "w", newline="", encoding="utf-8"
            ) as handle:
                writer = csv.writer(handle)
                writer.writerow(header)
                writer.writerows(rows)
        sums = {
            str(path.relative_to(self.root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in self.root.rglob("*") if path.is_file()
        }
        (self.root / "SHA256SUMS.json").write_text(
            json.dumps(sums), encoding="utf-8"
        )

    def run_inspector(self):
        return subprocess.run(
            [sys.executable, str(self.root / "inspect.py")],
            capture_output=True, text=True, check=False, timeout=10
        )

    def test_reports_original_and_actual_stored_relations(self):
        result = self.run_inspector()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("What happened?", result.stdout)
        self.assertIn("Budget ran out", result.stdout)
        self.assertIn("SUPPORTS", result.stdout)
        self.assertIn("CONTRADICTS", result.stdout)
        self.assertIn("NOT evidence weights", result.stdout)

    def test_integrity_failure_is_fail_closed(self):
        path = self.root / "data" / "claims.csv"
        path.write_text(path.read_text(encoding="utf-8") + "tampered\n",
                        encoding="utf-8")
        result = self.run_inspector()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("integrity mismatch", result.stderr)

    def test_manifest_path_escape_is_rejected(self):
        sums = json.loads((self.root / "SHA256SUMS.json").read_text())
        sums["../secret"] = "fake"
        (self.root / "SHA256SUMS.json").write_text(json.dumps(sums))
        result = self.run_inspector()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsafe manifest path", result.stderr)


if __name__ == "__main__":
    unittest.main()
