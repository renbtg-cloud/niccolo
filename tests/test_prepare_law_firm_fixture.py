import hashlib
import json
import tempfile
import unittest
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "prepare-law-firm-fixture.py"
spec = importlib.util.spec_from_file_location("prepare_law_firm_fixture", SCRIPT)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


class PrepareLawFirmFixtureTest(unittest.TestCase):
    def test_preparation_preserves_original_hashes_and_marks_derivations(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "prepared"
            manifest = mod.prepare(ROOT / "fixtures" / "law-firm-mvp", out)
            self.assertGreaterEqual(len(manifest["records"]), 10)
            self.assertFalse(manifest["manifest_asserts_authenticity"])
            self.assertFalse(manifest["manifest_asserts_completeness"])
            for record in manifest["records"]:
                original = (ROOT / "fixtures" / "law-firm-mvp" / record["original_path"]).read_bytes()
                derived = (out / record["derived_path"]).read_bytes()
                self.assertEqual(hashlib.sha256(original).hexdigest(), record["original_sha256"])
                self.assertEqual(hashlib.sha256(derived).hexdigest(), record["derived_sha256"])
                self.assertFalse(record["truth_or_authenticity_upgraded"])
                if record["source_format"] == "WHATSAPP_EXPORT":
                    self.assertTrue(
                        derived.startswith(b"01/01/2026, 00:00 - External operator: DERIVED IMPORT TRANSCRIPT")
                    )
                    self.assertFalse(record["derived_is_original"])
                else:
                    self.assertEqual(derived, original)
                    self.assertTrue(record["derived_is_original"])

            stored = json.loads((out / "transform-manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(stored, manifest)


if __name__ == "__main__":
    unittest.main()
