import json
import unittest

from protocol import HERE, build_cases, interpret, lexical, parse_visible, sensitivity
from run import direct_prompt, parser_prompt


class ProtocolTests(unittest.TestCase):
    def setUp(self):
        self.cases = json.loads((HERE / "cases.json").read_text())

    def test_frozen_cases_and_gold(self):
        self.assertEqual(self.cases, build_cases())
        self.assertEqual(len(self.cases), 40)
        self.assertEqual(len({c["pair_id"] for c in self.cases}), 20)
        for c in self.cases:
            self.assertEqual(interpret(c, [r["gold"] for r in c["records"]])[0], c["expected"])

    def test_lexical_no_substring_traps(self):
        self.assertEqual(lexical("does not know"), "N")
        self.assertEqual(lexical("cannot tell whether she knows"), "A")
        self.assertEqual(lexical("reported"), "R")
        for c in self.cases:
            for r in c["records"]:
                label = lexical(parse_visible(r["text"])[2])
                self.assertIn(label, ("A", r["gold"]))

    def test_sensitivity_contains_every_chosen_outcome(self):
        for c in self.cases:
            base = [lexical(parse_visible(r["text"])[2]) for r in c["records"]]
            gold = [r["gold"] for r in c["records"]]
            s = sensitivity(c, base, gold)
            self.assertIn(c["expected"], s["possible_verdicts"])
            self.assertEqual(s["stable"], len(s["possible_verdicts"]) == 1)

    def test_no_gold_in_prompt_and_revisions_cacheable(self):
        for c in self.cases:
            peer = next(x for x in self.cases if x["pair_id"] == c["pair_id"] and x != c)
            self.assertEqual([parser_prompt(r) for r in c["records"]],
                             [parser_prompt(r) for r in peer["records"]])
            altered = dict(c, expected="NOT_GOLD")
            self.assertEqual(direct_prompt(c), direct_prompt(altered))
            for r in c["records"]:
                self.assertEqual(parser_prompt(r), parser_prompt(dict(r, gold="NOT_GOLD")))


if __name__ == "__main__":
    unittest.main()
