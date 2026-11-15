"""Postprocessing only: quantify which interpretation errors matter downstream."""
from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path

from protocol import RELATIONS, interpret
from run import summarize


def audit(cases, report):
    rows = report["rows"]
    by_case = {c["case_id"]: c for c in cases}
    results = {}
    for treatment in ("lexical", "hybrid", "hybrid_challenge"):
        selected = [r for r in rows if r["treatment"] == treatment]
        pairs = {}
        for r in selected:
            c = by_case[r["case_id"]]
            pairs.setdefault(c["pair_id"], r)
        matrix = {}
        for relation in RELATIONS:
            matrix[relation] = dict(Counter(r["relation"] or "ABSTAIN"
                                            for r in pairs.values()
                                            if r["gold_relation"] == relation))
        results[treatment] = {
            "unique_records": len(pairs),
            "unique_relation_correct": sum(r["relation"] == r["gold_relation"]
                                           for r in pairs.values()),
            "relation_confusion": matrix,
            "wrong_or_missing_parse_correct_verdict": sum(
                r["relation"] != r["gold_relation"] and r["match"] for r in selected),
            "wrong_or_missing_parse_wrong_verdict": sum(
                r["relation"] != r["gold_relation"] and not r["match"] for r in selected),
            "correct_parse_wrong_verdict": sum(
                r["relation"] == r["gold_relation"] and not r["match"] for r in selected),
        }
    # Show how many fixtures actually distinguish possible parse interpretations.
    # A downstream result can be invariant even if the parser made an error.
    sensitivity = Counter(len({interpret(c, rel)[0] for rel in RELATIONS}) for c in cases)
    return {"parser_audit": results, "distinct_verdicts_across_four_parses": dict(sensitivity)}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--report", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    cases = json.loads(Path(__file__).with_name("cases.json").read_text())
    report = json.loads(args.report.read_text())
    result = {"summary": summarize(cases, report["rows"]), **audit(cases, report)}
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
