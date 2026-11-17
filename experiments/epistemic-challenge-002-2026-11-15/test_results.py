"""Validate the immutable observed report and independent postprocessing."""
import json
import unittest
from pathlib import Path

from audit_results import audit
from protocol import interpret, lexical, parse_visible
from run import TREATMENTS, direct_prompt, file_hash, parser_prompt, summarize

HERE = Path(__file__).resolve().parent


class ObservedResultsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = json.loads((HERE / "cases.json").read_text())
        cls.report = json.loads((HERE / "results/observations.json").read_text())
        cls.index = {c["case_id"]: c for c in cls.cases}

    def test_frozen_dataset_and_coverage(self):
        self.assertEqual(self.report["cases_sha256"], file_hash(HERE / "cases.json"))
        rows = self.report["rows"]
        self.assertEqual(len(rows), 400)
        self.assertEqual({(r["case_id"], r["treatment"]) for r in rows},
                         {(c["case_id"], t) for c in self.cases for t in TREATMENTS})
        self.assertEqual(len({r["call_id"] for r in rows if "call_id" in r}), 200)

    def test_observed_choices_and_derivations(self):
        for row in self.report["rows"]:
            case = self.index[row["case_id"]]
            treatment = row["treatment"]
            self.assertEqual(row["expected"], case["expected"])
            self.assertEqual(row["match"], row["observed"] == case["expected"])
            if treatment == "direct":
                self.assertEqual(row["inference"]["prompt"], direct_prompt(case))
                mapping = case["answer_mapping"]
                self.assertEqual(row["observed"], mapping[row["inference"]["selected"]])
            else:
                if treatment == "lexical":
                    self.assertEqual(row["relation"], lexical(parse_visible(case["record"])[2]))
                else:
                    self.assertEqual(row["inference"]["prompt"],
                                     parser_prompt(case, treatment == "hybrid_challenge"))
                    self.assertEqual(row["relation"], case["parse_mapping"][row["inference"]["selected"]])
                verdict, trace = interpret(case, row["relation"])
                self.assertEqual(row["observed"], verdict)
                self.assertEqual(row["verifier_trace"], trace)

    def test_summary_and_exploratory_audit_reproduce(self):
        actual = {"summary": summarize(self.cases, self.report["rows"]),
                  **audit(self.cases, self.report)}
        self.assertEqual(self.report["summary"], actual["summary"])
        self.assertEqual(json.loads(json.dumps(actual)),
                         json.loads((HERE / "results/audit.json").read_text()))


if __name__ == "__main__":
    unittest.main()
