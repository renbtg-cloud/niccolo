import copy
import json
import unittest

from audit_results import audit
from protocol import HERE


class ResultTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report = json.loads((HERE / "results/observations.json").read_text())

    def test_complete_audit(self):
        result = audit(self.report)
        self.assertEqual(result["validated_rows"], 40)
        self.assertEqual(result["validated_calls"], len(self.report["calls"]))
        self.assertEqual(result["summary"]["direct"]["model_calls"], 40)
        self.assertLessEqual(result["summary"]["budget_matched_direct"]["input_tokens"],
                             result["summary"]["gated"]["input_tokens"])
        self.assertEqual(result["summary"]["gated_stable_only"]["abstain"],
                         result["summary"]["sensitivity"]["unstable"])

    def test_tampered_verdict_rejected(self):
        broken = copy.deepcopy(self.report)
        broken["rows"][0]["gated"] = "INVALID"
        with self.assertRaises(AssertionError):
            audit(broken)

    def test_tampered_prompt_rejected(self):
        broken = copy.deepcopy(self.report)
        key = next(iter(broken["calls"]))
        broken["calls"][key]["prompt"] += " altered"
        with self.assertRaises(AssertionError):
            audit(broken)


if __name__ == "__main__":
    unittest.main()
