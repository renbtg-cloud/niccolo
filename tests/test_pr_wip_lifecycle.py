import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "pr_wip_lifecycle", ROOT / "scripts" / "check-pr-wip-lifecycle.py"
)
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
Commit = module.Commit


def audit(*subjects):
    return module.check_commits(
        [Commit(f"{index:040x}", subject) for index, subject in enumerate(subjects, 1)]
    )


class PrWipLifecycleTests(unittest.TestCase):
    def test_empty_and_independent_done_commits_are_valid(self):
        self.assertEqual(audit(), [])
        self.assertEqual(audit("DONE: finished feature", "DONE: regression"), [])

    def test_one_and_multiple_checkpoints_closed_by_done(self):
        self.assertEqual(audit("WIP 001: scaffold", "DONE: feature"), [])
        self.assertEqual(audit("WIP 001: scaffold", "WIP 002: add tests", "DONE: feature"), [])
        self.assertEqual(audit("WIP 001: a", "DONE: a", "WIP 001: b", "DONE: b"), [])

    def test_singleton_requires_done_or_reword(self):
        self.assertIn("dangling WIP", " ".join(audit("WIP 001: incomplete")))

    def test_second_wip_001_cannot_abandon_first_unit(self):
        errors = audit("WIP 001: a", "WIP 001: b", "DONE: b")
        self.assertTrue(any("unfinished logical unit" in e for e in errors))

    def test_numbering_must_be_contiguous_and_restart_per_unit(self):
        self.assertTrue(any("expected WIP 001" in e for e in audit("WIP 002: invalid")))
        self.assertTrue(any("expected WIP 002" in e for e in audit(
            "WIP 001: a", "WIP 003: c", "DONE: a"
        )))

    def test_requires_zero_padded_subject_and_colon(self):
        self.assertTrue(any("three digits" in e for e in audit("WIP 1: legacy", "DONE: closed")))
        self.assertTrue(any("malformed" in e for e in audit("WIP unfinished")))
        self.assertTrue(any("malformed" in e for e in audit("DONE unfinished")))

    def test_similarly_prefixed_ordinary_subjects_are_not_lifecycle_markers(self):
        self.assertEqual(audit("WIPER: maintenance", "DONEDOCKER: release"), [])

    def test_unrelated_subjects_do_not_close_wip(self):
        self.assertTrue(any("dangling" in e for e in audit(
            "WIP 001: backend", "docs: notes", "fix: unrelated"
        )))


if __name__ == "__main__":
    unittest.main()
