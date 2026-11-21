"""Keep the epistemic adversarial audit tied to real executable test cases."""
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
TRACE = ROOT / "docs" / "architecture" / "EPISTEMIC_ADVERSARIAL_TEST_TRACEABILITY.md"


class EpistemicAdversarialTraceabilityTests(unittest.TestCase):
    def test_all_adversarial_cases_are_individually_traceable(self):
        rows = re.findall(r"^\| (AT-\d{2})[^|]*\| ([^|]+)\| ([^|]+)\| ([^|]+)\|$",
                          TRACE.read_text(encoding="utf-8"), re.MULTILINE)
        self.assertEqual([row[0] for row in rows], [f"AT-{i:02d}" for i in range(1, 27)])
        for case, status, witnesses, remaining in rows:
            with self.subTest(case=case):
                self.assertIn(status.strip(), {"Focused", "Constituent", "Open"})
                self.assertRegex(witnesses, r"`[a-z0-9_]+::[a-z0-9_]+`")
                self.assertTrue(remaining.strip())

    def test_witnesses_refer_to_actual_rust_integration_tests(self):
        witnesses = re.findall(r"`([a-z0-9_]+)::([a-z0-9_]+)`", TRACE.read_text(encoding="utf-8"))
        self.assertTrue(witnesses)
        for suite, name in witnesses:
            with self.subTest(suite=suite, name=name):
                source = (ROOT / "tests" / f"{suite}.rs").read_text(encoding="utf-8")
                self.assertRegex(source, rf"#\[(?:tokio::)?test(?:\([^\n]*\))?\]\s*(?:async\s+)?fn\s+{re.escape(name)}\s*\(")


if __name__ == "__main__":
    unittest.main()
