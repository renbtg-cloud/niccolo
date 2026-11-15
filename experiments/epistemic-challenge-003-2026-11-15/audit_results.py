"""Replay stored observations without a model or network; fail on inconsistencies."""
from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path

from protocol import HERE, interpret, lexical, parse_visible, previous, sensitivity
from run import MODEL_SHA, digest, direct_prompt, parser_prompt, summarize


def audit(report):
    cases = json.loads((HERE / "cases.json").read_text())
    assert report["fingerprint"] == {"cases_sha256": digest(HERE / "cases.json"),
                                     "protocol_sha256": digest(HERE / "protocol.py"),
                                     "runner_sha256": digest(HERE / "run.py"), "model_sha256": MODEL_SHA}
    assert [r["case_id"] for r in report["rows"]] == [c["case_id"] for c in cases]
    used = set()
    parser_gold, parser_pred = {}, {}

    def check_call(key, prompt, mapping):
        used.add(key)
        call = report["calls"][key]
        assert call["prompt"] == prompt
        assert set(call["logits_uncalibrated"]) == set(mapping)
        assert call["selected"] == max(mapping, key=call["logits_uncalibrated"].get)
        assert call["value"] == mapping[call["selected"]]
        assert call["tokens"] > 0 and call["seconds"] >= 0
        return call["value"]

    for case, row in zip(cases, report["rows"]):
        gold = [r["gold"] for r in case["records"]]
        assert row["expected"] == case["expected"] == interpret(case, gold)[0]
        base = [lexical(parse_visible(r["text"])[2]) for r in case["records"]]
        old = [previous.lexical(parse_visible(r["text"])[2]) or "A" for r in case["records"]]
        selected = list(base)
        keys = []
        for i, record in enumerate(case["records"]):
            if base[i] == "A":
                key = f"{case['pair_id']}:parse:{i}"
                selected[i] = check_call(key, parser_prompt(record), record["mapping"])
                parser_gold[key], parser_pred[key] = record["gold"], selected[i]
                keys.append(key)
        assert row["gate_calls"] == keys
        assert row["direct_call"] == case["case_id"] + ":direct"
        assert row["direct"] == check_call(row["direct_call"], direct_prompt(case), case["mapping"])
        assert row["gold_relations"] == gold and row["lexical_relations"] == base and row["gated_relations"] == selected
        assert row["lexical"] == interpret(case, base)[0]
        assert row["old_lexical"] == interpret(case, old)[0]
        verdict, trace = interpret(case, selected)
        assert row["gated"] == verdict and row["gated_trace"] == trace
        sens = sensitivity(case, base, selected)
        assert row["sensitivity"] == sens
        assert row["gated_stable_only"] == (verdict if sens["stable"] else "ABSTAIN")
    assert used == set(report["calls"])
    summary = summarize(cases, report)
    assert summary == report["summary"]
    pairs = {(c["pair_id"], c["phase"]): r for c, r in zip(cases, report["rows"])}
    unchanged = [c["pair_id"] for c in cases if c["phase"] == 0 and
                 pairs[c["pair_id"], 0]["expected"] == pairs[c["pair_id"], 1]["expected"]]
    # Distinguish explicit refusal from UNRESOLVED on ambiguous records.
    return {"validated_rows": len(cases), "validated_calls": len(used), "summary": summary,
            "routed_parser": {"n": len(parser_gold), "correct": sum(parser_gold[k] == parser_pred[k] for k in parser_gold),
                              "abstained": sum(v == "A" for v in parser_pred.values()),
                              "gold_ambiguous": sum(v == "A" for v in parser_gold.values()),
                              "confusion": {g: dict(Counter(parser_pred[k] for k, v in parser_gold.items() if v == g))
                                            for g in sorted(set(parser_gold.values()))}},
            "unchanged_gold_pairs": len(unchanged),
            "spurious_revisions": {t: sum(pairs[p, 0][t] != pairs[p, 1][t] for p in unchanged)
                                   for t in ("direct", "lexical", "gated", "gated_stable_only")}}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--report", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    result = audit(json.loads(args.report.read_text()))
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
