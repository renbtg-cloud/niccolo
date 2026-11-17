import unittest

import protocol
import run_deepseek as runner


class RunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = protocol.build_cases()

    def test_prompts_do_not_contain_gold_fields(self):
        for case in self.cases[:50]:
            prompt = runner.stable_json(runner.direct_messages(case))
            self.assertNotIn('"gold"', prompt)
            for record in case["records"]:
                p = runner.stable_json(runner.parser_messages(record))
                self.assertNotIn('"gold"', p)

    def test_effort_cohorts_are_frozen_and_nested(self):
        high, maximum = protocol.escalation_sets(self.cases)
        self.assertEqual(len(high), 200)
        self.assertEqual(len(maximum), 30)
        self.assertTrue(maximum <= high)

    def test_peak_cost_estimate_is_conservative_formula(self):
        usage = {"prompt_tokens": 1_000_000, "completion_tokens": 1_000_000}
        self.assertAlmostEqual(runner.peak_cost_from_usage(usage, .30, 1.20), 1.50)

    def test_choice_extraction(self):
        self.assertEqual(runner.extract_letter("X", ["W", "X", "Y", "Z"]), "X")
        self.assertEqual(runner.extract_letter("Final answer: Y", ["W", "X", "Y", "Z"]), "Y")
        with self.assertRaises(ValueError):
            runner.extract_letter("I refuse", ["W", "X", "Y", "Z"])


if __name__ == "__main__":
    unittest.main()
