#!/usr/bin/env python3
"""Validate per-logical-unit WIP -> DONE lifecycles in commits unique to a PR.

Draft PRs may end in WIP; the ready-to-merge PR workflow runs this check.
No history is rewritten by this script.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass

WIP = re.compile(r"^WIP\s+(\d+):\s+(.+)$", re.IGNORECASE)
DONE = re.compile(r"^DONE:\s+(.+)$", re.IGNORECASE)


@dataclass(frozen=True)
class Commit:
    sha: str
    subject: str


def check_commits(commits: list[Commit]) -> list[str]:
    errors: list[str] = []
    pending: list[Commit] = []
    expected = 1
    for commit in commits:
        subject = commit.subject
        wip = WIP.fullmatch(subject)
        if wip:
            number_text = wip.group(1)
            number = int(number_text)
            if number_text != f"{number:03d}":
                errors.append(f"{commit.sha[:12]}: use WIP {number:03d}: (three digits)")
            if pending and number == 1:
                errors.append(
                    f"{pending[0].sha[:12]}: unfinished logical unit before "
                    f"new WIP 001 at {commit.sha[:12]}"
                )
                pending = []
                expected = 1
            if number != expected:
                errors.append(
                    f"{commit.sha[:12]}: expected WIP {expected:03d}, got WIP {number:03d}"
                )
            pending.append(commit)
            expected = number + 1
        elif DONE.fullmatch(subject):
            pending = []
            expected = 1
        elif re.match(r"^(?:WIP|DONE)\b", subject, re.IGNORECASE):
            errors.append(
                f"{commit.sha[:12]}: malformed lifecycle subject {subject!r}; "
                "use 'WIP 001: description' or 'DONE: description'"
            )
    if pending:
        errors.append(
            f"{pending[0].sha[:12]}: dangling WIP unit at PR head: "
            + ", ".join(f"{c.sha[:12]} {c.subject}" for c in pending)
            + "; finish with DONE or reword a singleton WIP 001 to DONE"
        )
    return errors


def read_commit_range(base: str, head: str) -> list[Commit]:
    # First-parent matches the PR branch sequence. base..head excludes
    # commits already reachable from the base, including old shared WIPs.
    text = subprocess.check_output(
        ["git", "log", "--first-parent", "--reverse", "--format=%H%x09%s", f"{base}..{head}"],
        text=True,
    )
    return [Commit(*line.split("\t", 1)) for line in text.splitlines() if "\t" in line]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True, help="fetched base branch reference")
    parser.add_argument("--head", default="HEAD", help="PR head ref or SHA")
    args = parser.parse_args()
    commits = read_commit_range(args.base, args.head)
    errors = check_commits(commits)
    if errors:
        print(f"FAIL: {len(errors)} WIP/DONE lifecycle violation(s):", file=sys.stderr)
        for error in errors:
            print(" - " + error, file=sys.stderr)
        return 1
    print(f"PASS: {len(commits)} PR commit(s) have no dangling or misnumbered WIP units.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
