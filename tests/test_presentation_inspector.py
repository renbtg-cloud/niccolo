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
        self.analysis_id = "00000000-0000-0000-0000-000000000003"
        bundle = {
            "analysis_id": self.analysis_id,
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

    def write_shared_origin_support(self, second_claim, *, listed=True):
        routes = [{"argument_id": "a1", "grounding_set_id": route} for route in ["r1", "r2"]]
        sidecar = {
            "analysis_id": self.analysis_id,
            "format": "niccolo.presentation-workbench-support",
            "format_version": 1,
            "support": {
                "citations": [
                    {"claim_id": "c1", **routes[0]},
                    {"claim_id": second_claim, **routes[1]},
                ],
                "shared_artifact_origins": [{
                    "source_artifact_label": "Claire's interview",
                    "source_scope_locator": "Second answer",
                    "derivations": [{"routes": [route]} for route in routes],
                }],
                "shared_artifact_origins_truncated": True,
            },
        }
        path = self.root / "data" / "workbench_support.json"
        path.write_text(json.dumps(sidecar), encoding="utf-8")
        if listed:
            manifest = self.root / "SHA256SUMS.json"
            sums = json.loads(manifest.read_text(encoding="utf-8"))
            sums["data/workbench_support.json"] = hashlib.sha256(path.read_bytes()).hexdigest()
            manifest.write_text(json.dumps(sums), encoding="utf-8")

    def test_reports_original_and_actual_stored_relations(self):
        result = self.run_inspector()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("What happened?", result.stdout)
        self.assertIn("Budget ran out", result.stdout)
        self.assertIn("SUPPORTS", result.stdout)
        self.assertIn("CONTRADICTS", result.stdout)
        self.assertIn("NOT evidence weights", result.stdout)

    def test_shared_origins_are_reported_per_cited_claim(self):
        self.write_shared_origin_support("c1")
        same_claim = self.run_inspector()
        self.assertEqual(same_claim.returncode, 0, same_claim.stderr)
        self.assertIn("Shared origin for Unverified interpretation", same_claim.stdout)
        self.assertIn("Claire's interview · Second answer", same_claim.stdout)
        self.assertIn("limited set of recorded derivations", same_claim.stdout)

        self.write_shared_origin_support("c2")
        different_claims = self.run_inspector()
        self.assertEqual(different_claims.returncode, 0, different_claims.stderr)
        self.assertNotIn("Shared origin for", different_claims.stdout)

    def test_unlisted_workbench_support_is_ignored(self):
        self.write_shared_origin_support("c1", listed=False)
        result = self.run_inspector()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("Recorded Workbench route citations", result.stdout)

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
