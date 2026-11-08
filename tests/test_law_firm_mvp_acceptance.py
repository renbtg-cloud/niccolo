import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "check-law-firm-mvp-acceptance.py"
spec = importlib.util.spec_from_file_location("lawfirm_acceptance", SCRIPT)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

def fixture():
    return json.loads((ROOT / "fixtures" / "law-firm-mvp" / "acceptance.example.json").read_text())

def test_example_is_structurally_complete():
    assert mod.validate(fixture()) == []

def test_expected_story_verdict_is_rejected():
    doc = fixture()
    doc["scenario"]["expected_verdict"] = "Kessler and Julian guilty"
    assert any("expected_verdict" in e for e in mod.validate(doc))

def test_missing_case_is_rejected():
    doc = fixture()
    doc["acceptance_cases"] = doc["acceptance_cases"][1:]
    assert any("missing acceptance cases" in e for e in mod.validate(doc))

def test_old_niccolo_is_an_actor_not_a_truth_source():
    doc = fixture()
    doc["actors"] = [a for a in doc["actors"] if a["id"] != "old-niccolo"]
    assert any("old-niccolo" in e for e in mod.validate(doc))
