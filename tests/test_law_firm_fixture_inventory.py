import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "fixtures" / "law-firm-mvp"


class LawFirmFixtureInventoryTest(unittest.TestCase):
    def test_compact_fixture_covers_required_adversarial_material(self):
        texts = []
        for path in sorted((FIXTURE / "sources").glob("*")):
            texts.append(path.read_text(encoding="utf-8").lower())
        corpus = "\n".join(texts)
        required = [
            "push this through before the committee sees it",
            "report a",
            "report b",
            "report c",
            "separate system, not derived",
            "not collected: personal mobile messages",
            "unknown existence",
            "historical niccolò",
            "mapping disputed",
            "formal approval authority",
            "practical influence",
            "party alpha",
            "party beta",
            "late decisive fragment",
            "effective date is 1 april",
            "pre_collection_authenticity",
            "resolution credit was already active throughout march",
        ]
        # Integrity metadata is JSON outside sources.
        integrity = (FIXTURE / "integrity_manifest.json").read_text(encoding="utf-8").lower()
        corpus += "\n" + integrity
        for marker in required:
            self.assertIn(marker, corpus)

    def test_question_fixture_contains_cheap_and_costly_discriminators(self):
        data = json.loads((FIXTURE / "questions" / "discriminators.json").read_text(encoding="utf-8"))
        by_id = {q["id"]: q for q in data["questions"]}
        cheap = by_id["cheap-high-value"]
        costly = by_id["expensive-low-value"]
        self.assertGreater(cheap["decision_relevance"], costly["decision_relevance"])
        self.assertLess(cheap["acquisition_cost"], costly["acquisition_cost"])
        self.assertGreaterEqual(len(cheap["possible_outcomes"]), 2)
        self.assertTrue(costly["cheaper_alternative"])

    def test_fixture_does_not_encode_expected_verdict(self):
        for path in FIXTURE.rglob("*"):
            if not path.is_file():
                continue
            text = path.read_text(encoding="utf-8").lower()
            self.assertNotIn('"expected_verdict": "kessler', text)
            self.assertNotIn("prove collusion", text)


if __name__ == "__main__":
    unittest.main()
