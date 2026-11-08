"""Tests for the release documentation link checker."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

MODULE_PATH = Path(__file__).parents[1] / "scripts" / "check-doc-links.py"
spec = importlib.util.spec_from_file_location("doc_checker", MODULE_PATH)
doc_checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(doc_checker)


class DocumentationLinksTests(unittest.TestCase):
    def test_relative_links_and_anchor_only_links(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "docs").mkdir()
            (root / "README.md").write_text("[Guide](docs/README.md)\n[Top](#introduction)\n")
            (root / "docs" / "README.md").write_text("[Back](../README.md)\n[Remote](https://example.org/a)\n")
            links, errors = doc_checker.check(root)
            self.assertEqual((links, errors), (2, []))

    def test_detects_broken_links_and_escape(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text("[Missing](missing.md)\n[Outside](../../secret.md)\n")
            links, errors = doc_checker.check(root)
            self.assertEqual(links, 2)
            self.assertEqual(len(errors), 2)

    def test_ignores_fenced_code_and_checks_reference_definitions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text("```md\n[Fake](missing.md)\n```\n[ref]: exists.md\n")
            (root / "exists.md").write_text("# Exists\n")
            links, errors = doc_checker.check(root)
            self.assertEqual((links, errors), (1, []))


if __name__ == "__main__":
    unittest.main()
