import json
import unittest
from collections import Counter
from pathlib import Path

from protocol import build_cases, interpret, lexical, mental, neg, resolve
from run import direct_prompt, parser_prompt, summarize


class ProtocolTest(unittest.TestCase):
    def test_factivity_nested(self):
        self.assertEqual(resolve(["K(A,t1,K(B,t0,p))"], [], "p")[0], "SUPPORTED")

    def test_nonfactive(self):
        for statement in ["B(A,t1,p)", "R(A,t1,p)", "!K(A,t1,p)"]:
            self.assertEqual(resolve([statement], [], "p")[0], "UNRESOLVED")

    def test_conflict_without_explosion(self):
        self.assertEqual(resolve(["p", "!p"], [], "p")[0], "CONFLICT")
        self.assertEqual(resolve(["p", "!p"], [], "q")[0], "UNRESOLVED")

    def test_negated_query(self):
        self.assertEqual(resolve(["p"], [], "!p")[0], "REFUTED")

    def test_temporal_scope(self):
        self.assertEqual(resolve(["K(A,t1,p)"], [], "K(A,t2,p)")[0], "UNRESOLVED")

    def test_rule_requires_both_explicit_inputs(self):
        rules = [{"if": ["p", "baseline"], "then": "anomaly"}]
        self.assertEqual(resolve(["p"], rules, "anomaly")[0], "UNRESOLVED")
        self.assertEqual(resolve(["p", "baseline"], rules, "anomaly")[0], "SUPPORTED")

    def test_freeze_and_gold(self):
        cases = json.loads(Path(__file__).with_name("cases.json").read_text())
        self.assertEqual(cases, build_cases())
        self.assertEqual(len(cases), 100)
        self.assertEqual(len({c["pair_id"] for c in cases}), 50)
        self.assertEqual(sorted(Counter(c["family"] for c in cases).values()), [20] * 5)
        for c in cases:
            self.assertEqual(interpret(c, c["gold_relation"])[0], c["expected"], c["case_id"])
            for prompt in (direct_prompt(c), parser_prompt(c), parser_prompt(c, True)):
                self.assertNotIn("gold_relation", prompt)
                self.assertNotIn("case_id", prompt)

    def test_parser_revision_cache_is_valid(self):
        cases = build_cases()
        pairs = {}
        for c in cases:
            pairs.setdefault(c["pair_id"], []).append(c)
        for pair in pairs.values():
            self.assertEqual(parser_prompt(pair[0]), parser_prompt(pair[1]))
            self.assertEqual(parser_prompt(pair[0], True), parser_prompt(pair[1], True))
            self.assertNotEqual(direct_prompt(pair[0]), direct_prompt(pair[1]))

    def test_lexical_unknown(self):
        self.assertIsNone(lexical("has a hunch"))
        self.assertEqual(lexical("does not know"), "N")

    def test_summary(self):
        cases = build_cases()
        rows = [{"case_id": c["case_id"], "treatment": "lexical", "expected": c["expected"],
                 "observed": c["expected"], "match": True} for c in cases]
        metrics = summarize(cases, rows)["treatments"]["lexical"]
        self.assertEqual(metrics["correct"], 100)
        self.assertEqual(metrics["pairs_both_correct"], 50)
        self.assertEqual(metrics["false_certainty"], 0)


if __name__ == "__main__":
    unittest.main()
