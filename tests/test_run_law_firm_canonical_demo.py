import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

class CanonicalLawFirmDemoTests(unittest.TestCase):
    def test_packager_produces_auditable_non_verdict_bundle(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "demo"
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/run-law-firm-canonical-demo.py",
                    "--output",
                    str(output),
                ],
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout)
            execution = json.loads(
                (output / "canonical-execution.json").read_text(encoding="utf-8")
            )
            handoff = json.loads(
                (output / "canonical-handoff.json").read_text(encoding="utf-8")
            )
            manifest = json.loads(
                (output / "prepared-fixture" / "transform-manifest.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(execution["acceptance_summary"]["case_count"], 16)
            self.assertEqual(execution["acceptance_summary"]["completed_case_count"], 16)
            self.assertFalse(execution["execution_asserts_verdict"])
            self.assertFalse(execution["execution_asserts_authenticity"])
            self.assertFalse(execution["execution_grants_authority"])
            self.assertFalse(execution["rust_runtime_verification_requested"])
            self.assertEqual(handoff["scenario"]["expected_verdict"], None)
            self.assertFalse(handoff["handoff_asserts_verdict"])
            self.assertFalse(handoff["handoff_executes_actions"])
            self.assertFalse(manifest["manifest_asserts_authenticity"])
            self.assertFalse(manifest["manifest_asserts_completeness"])
            self.assertTrue((output / "README.md").is_file())
            self.assertTrue((output / "acceptance.completed.json").is_file())
            self.assertTrue((output / "operator-review.md").is_file())

if __name__ == "__main__":
    unittest.main()
