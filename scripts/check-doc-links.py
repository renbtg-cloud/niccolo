#!/usr/bin/env python3
"""Verify repository-local Markdown links and keep the public root uncluttered.

External URLs and in-page anchor fragments are not checked.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

LINK = re.compile(r"!?\[[^\]\n]*\]\((?:<([^>]+)>|([^\n)]+))\)")
REFERENCE = re.compile(r"^\s{0,3}\[[^\]\n]+\]:\s*(?:<([^>]+)>|(\S+))", re.MULTILINE)
FENCE = re.compile(r"^\s{0,3}(\`{3,}|~{3,})")


def outside_fences(text: str) -> str:
    parts = []
    marker_char = ""
    marker_length = 0
    for line in text.splitlines(keepends=True):
        match = FENCE.match(line)
        if match:
            marker = match.group(1)
            if not marker_char:
                marker_char, marker_length = marker[0], len(marker)
            elif marker[0] == marker_char and len(marker) >= marker_length and not line.lstrip()[len(marker):].strip():
                marker_char, marker_length = "", 0
            parts.append("\n")
        elif marker_char:
            parts.append("\n")
        else:
            parts.append(line)
    return "".join(parts)


def check(root: Path) -> tuple[int, list[str]]:
    root = root.resolve()
    errors = []
    checked = 0
    for file in sorted(root.rglob("*.md")):
        if any(part in {".git", "node_modules", "target", "dist"} for part in file.relative_to(root).parts):
            continue
        text = outside_fences(file.read_text(encoding="utf-8"))
        for match in list(LINK.finditer(text)) + list(REFERENCE.finditer(text)):
            raw = (match.group(1) or match.group(2) or "").strip()
            raw = raw.split(' "', 1)[0].split(" '", 1)[0]
            if not raw or raw.startswith("#"):
                continue
            url = urlsplit(raw)
            if url.scheme or url.netloc or not url.path:
                continue
            checked += 1
            target = (file.parent / unquote(url.path)).resolve()
            if not target.is_relative_to(root) or not target.exists():
                line = text.count("\n", 0, match.start()) + 1
                errors.append(f"{file.relative_to(root)}:{line}: broken local link {raw}")
    return checked, errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    links, failures = check(args.root)
    if (args.root / "Cargo.toml").exists():
        extra_root_docs = sorted(p.name for p in args.root.glob("*.md") if p.name != "README.md")
        failures.extend(f"root Markdown must be organized: {name}" for name in extra_root_docs)
    for message in failures:
        print(message, file=sys.stderr)
    print(f"Checked {links} local Markdown links and root layout; {len(failures)} errors.")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
