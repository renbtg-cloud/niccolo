import unittest

import protocol

EXPECTED_SHA = "d296700ec6439a3f1cae406b03ca6b54ca45fcded605f6e4632a0896b9e64c76"
EXPECTED_HIGH_SHA = "43d5ed35d9c50432a61c7f42eda740ccea847759c7cc67237135ccced87c868c"
EXPECTED_MAX_SHA = "53487d297c66bcf6bf04db15bd0e3ecfa95d867df680f1a8d44f8d775bd52e5a"


class ProtocolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = protocol.build_cases()
        cls.summary = protocol.corpus_summary(cls.cases)

    def test_frozen_corpus_hash_and_size(self):
        self.assertEqual(len(self.cases), 1000)
        self.assertEqual(self.summary["corpus_sha256"], EXPECTED_SHA)
        self.assertEqual(self.summary["high_ids_sha256"], EXPECTED_HIGH_SHA)
        self.assertEqual(self.summary["max_ids_sha256"], EXPECTED_MAX_SHA)

    def test_balanced_labels_and_families(self):
        self.assertEqual(self.summary["label_counts"], {label: 250 for label in protocol.LABELS})
        self.assertEqual(self.summary["family_counts"], {family: 100 for family in protocol.FAMILIES})

    def test_holdout_and_escalation_counts(self):
        self.assertEqual(self.summary["holdout_bases"], 50)
        self.assertEqual(self.summary["holdout_cases"], 200)
        self.assertEqual(self.summary["high_cases"], 200)
        self.assertEqual(self.summary["max_cases"], 30)
        high, maximum = protocol.escalation_sets(self.cases)
        self.assertTrue(maximum <= high)

    def test_gold_replays_through_verifier(self):
        for case in self.cases:
            verdict, _ = protocol.interpret(case, [r["gold"] for r in case["records"]])
            self.assertEqual(verdict, case["expected"], case["case_id"])

    def test_no_hidden_relation_in_visible_text(self):
        for case in self.cases:
            for record in case["records"]:
                self.assertNotIn('"gold"', record["text"])
                protocol.parse_visible(record["text"])

    def test_deterministic_parser_has_both_abstention_and_trap_risk(self):
        self.assertGreater(self.summary["deterministic_abstention_cases"], 0)
        self.assertGreater(self.summary["parser_trap_cases"], 0)
        trapped = [c for c in self.cases if c["difficulty"]["parser_trap"]]
        self.assertTrue(any(
            protocol.lexical(protocol.parse_visible(r["text"])[2]) not in ("A", r["gold"])
            for c in trapped for r in c["records"]
        ))


if __name__ == "__main__":
    unittest.main()
