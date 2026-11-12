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

    def test_recorded_epistemic_controls_are_explained_without_probability_claims(self):
        policies = self.root / "data" / "epistemic_policies.json"
        policies.write_text(json.dumps([{
            "policy_id": "policy-1",
            "situation_id": "situation-1",
            "scope": "SITUATION",
            "support_multiplier_pct": 80,
            "counter_multiplier_pct": 140,
            "per_origin_cap": 75,
            "rationale": "Conservative counter-weighting",
            "recorded_by_principal_id": "principal-1",
            "created_at": "2026-11-11 00:00:00+00",
            "is_probability_model": False,
            "proves_policy_was_used_for_published_output": False,
        }]), encoding="utf-8")
        credence = self.root / "data" / "human_credence.json"
        credence.write_text(json.dumps([{
            "claim_id": "c1",
            "credence_id": "credence-1",
            "confidence_percent": 63,
            "usage_policy": "SUBJECTIVE_BELIEF",
            "rationale": "Human context",
            "recorded_by_principal_id": "principal-1",
            "created_at": "2026-11-11 00:00:01+00",
            "is_human_declaration": True,
            "is_calibrated_probability": False,
            "is_documentary_evidence": False,
            "makes_claim_a_fact": False,
            "part_of_frozen_analysis_input": False,
        }]), encoding="utf-8")
        scope = self.root / "data" / "epistemic_export_scope.json"
        scope.write_text(json.dumps({
            "recorded_policy_count": 1,
            "recorded_human_credence_count": 1,
            "deterministic_scores_exported": False,
            "policy_history_proves_policy_used_for_output": False,
            "human_credence_is_documentary_evidence": False,
            "human_credence_is_calibrated_probability": False,
        }), encoding="utf-8")
        sums = json.loads((self.root / "SHA256SUMS.json").read_text())
        for path in [policies, credence, scope]:
            sums[str(path.relative_to(self.root))] = hashlib.sha256(path.read_bytes()).hexdigest()
        (self.root / "SHA256SUMS.json").write_text(json.dumps(sums), encoding="utf-8")

        result = self.run_inspector()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Recorded epistemic controls:", result.stdout)
        self.assertIn("policy records relevant to this Situation: 1", result.stdout)
        self.assertIn("human credence records for this analysis: 1", result.stdout)
        self.assertIn("deterministic grounded scores exported: False", result.stdout)
        self.assertIn("policy history proves policy used for output: False", result.stdout)
        self.assertIn("63% SUBJECTIVE_BELIEF", result.stdout)
        self.assertIn("not documentary evidence or calibrated truth probabilities", result.stdout)

    def test_optional_examined_source_extracts_are_disclosed_without_truth_claims(self):
        extracts = self.root / "data" / "examined_source_fragments.json"
        extracts.write_text(
            json.dumps([{
                "document_id": "doc",
                "evidence_id": "evidence",
                "ordinal": 0,
                "format": "WHATSAPP_EXPORT",
                "locator": "private-chat.txt",
                "document_sha256": "a" * 64,
                "fragment_sha256": hashlib.sha256(b"private excerpt").hexdigest(),
                "speaker_label": "Alice",
                "source_class": "HUMAN_COMMUNICATION",
                "content": "private excerpt",
            }]),
            encoding="utf-8",
        )
        sums = json.loads((self.root / "SHA256SUMS.json").read_text())
        sums["data/examined_source_fragments.json"] = hashlib.sha256(
            extracts.read_bytes()
        ).hexdigest()
        (self.root / "SHA256SUMS.json").write_text(json.dumps(sums), encoding="utf-8")
        result = self.run_inspector()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Explicitly authorized examined source excerpts: 1", result.stdout)
        self.assertIn("NOT original uploaded files", result.stdout)
        self.assertIn("do NOT establish truth", result.stdout)
        self.assertIn("sensitive/private communications", result.stdout)
        self.assertIn("fragment SHA-256", result.stdout)
        self.assertIn(hashlib.sha256(b"private excerpt").hexdigest(), result.stdout)
        self.assertIn("document SHA-256", result.stdout)
        self.assertNotIn("private excerpt", result.stdout)

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
