"""Frozen comparison, actual model calls, atomic incremental observations."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import sys
import time
from pathlib import Path

from protocol import HERE, LABELS, RELATIONS, interpret, lexical, parse_visible, sensitivity

MODEL_SHA = "d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5"
DESCRIPTIONS = {"K": "knowledge or verified fact", "B": "belief, guess or suspicion",
                "R": "report of a claim, not necessarily true", "N": "explicit lack of knowledge",
                "A": "ambiguous: cannot clearly identify the relation"}
SHARED = ("Synthetic records. K(person,time,p) means knows p and entails p, including nested content. "
          "B is belief, R is report; neither entails p. Explicit lack of knowledge means !K, not !p. "
          "Ambiguous records supply no premise. ! negates. Apply only forward IF rules; no time "
          "persistence, no closed world, no inference from unrelated contradiction. "
          "SUPPORTED if q only, REFUTED if !q only, CONFLICT if both, UNRESOLVED if neither.")


def chat(system, user):
    return (f"<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n"
            "<|im_start|>assistant\n<think>\n\n</think>\n")


def choices(mapping, descriptions=None):
    return "\n".join(f"{k}: {descriptions[v] if descriptions else v}" for k, v in mapping.items())


def direct_prompt(case):
    rules = "\n".join("IF " + " AND ".join(r["if"]) + " THEN " + r["then"] for r in case["rules"])
    records = "\n".join(r["text"] for r in case["records"])
    return chat(SHARED, f"Records:\n{records}\nStipulated facts: {', '.join(case['givens']) or '(none)'}\n"
                f"Rules:\n{rules or '(none)'}\nQuery: {case['query']}\n{choices(case['mapping'])}\n"
                "Choose one letter only.")


def parser_prompt(record):
    return chat("Classify only the outer relation expressed by the wording in time | person | wording | content. "
                "Do not infer truth from a report or belief. If wording is ambiguous, abstain.",
                f"{record['text']}\n{choices(record['mapping'], DESCRIPTIONS)}\nChoose one letter only.")


def digest(path):
    with open(path, "rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def save(path, report):
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(report, indent=2) + "\n")
    os.replace(tmp, path)


def infer(runner, tok, encode, prompt, mapping):
    tokens = encode(tok, prompt)
    letters = list(mapping)
    ids = [tok.encode(x) for x in letters]
    assert all(len(x) == 1 for x in ids) and len(set(x[0] for x in ids)) == len(letters)
    runner.cache = []
    start = time.monotonic()
    _, logits = runner.forward(tokens, 0, choices=[x[0] for x in ids])
    elapsed = time.monotonic() - start
    assert all(math.isfinite(x) for x in logits)
    winner = letters[max(range(len(letters)), key=logits.__getitem__)]
    return {"prompt": prompt, "selected": winner, "value": mapping[winner], "tokens": len(tokens),
            "seconds": round(elapsed, 5), "logits_uncalibrated": dict(zip(letters, [round(x, 5) for x in logits]))}


def summarize(cases, report):
    calls, rows = report["calls"], report["rows"]
    pairs = {c["case_id"]: c["pair_id"] for c in cases}
    expected = {c["case_id"]: c["expected"] for c in cases}
    direct_calls = [r["direct_call"] for r in rows]
    gate_calls = sorted({x for r in rows for x in r["gate_calls"]})
    cost = lambda ids: sum(calls[x]["tokens"] for x in ids)
    # Predeclared fixed frozen-case prefix; stop before the first call exceeding
    # the observed gated input-token budget. Remaining cases use lexical alone.
    # Recombination, not a separate timed system run.
    chosen, spent = set(), 0
    for row in rows:
        n = calls[row["direct_call"]]["tokens"]
        if spent + n > cost(gate_calls):
            break
        chosen.add(row["case_id"])
        spent += n
    out = {}
    conditions = {"direct": direct_calls, "lexical": [], "old_lexical": [], "gated": gate_calls,
                  "gated_stable_only": gate_calls,
                  "budget_matched_direct": [r["direct_call"] for r in rows if r["case_id"] in chosen]}
    for condition, ids in conditions.items():
        predictions = {r["case_id"]: (r["direct"] if r["case_id"] in chosen else r["lexical"])
                       if condition == "budget_matched_direct" else r[condition] for r in rows}
        by_pair = {}
        for cid, pred in predictions.items():
            by_pair.setdefault(pairs[cid], []).append(pred == expected[cid])
        correct = sum(pred == expected[cid] for cid, pred in predictions.items())
        definitive = sum(pred in ("SUPPORTED", "REFUTED") and expected[cid] in ("UNRESOLVED", "CONFLICT")
                         for cid, pred in predictions.items())
        eligible = sum(expected[cid] in ("UNRESOLVED", "CONFLICT") for cid in predictions)
        out[condition] = {"correct": correct, "n": len(predictions), "false_certainty": definitive,
                          "false_certainty_eligible": eligible, "abstain": sum(x == "ABSTAIN" for x in predictions.values()),
                          "pairs_both_correct": sum(len(x) == 2 and all(x) for x in by_pair.values()),
                          "model_calls": len(ids), "input_tokens": cost(ids),
                          "model_seconds": round(sum(calls[x]["seconds"] for x in ids), 3),
                          "by_family": {f: {"correct": sum(predictions[c["case_id"]] == c["expected"] for c in cases
                                                           if c["family"] == f and c["case_id"] in predictions),
                                             "n": sum(c["family"] == f and c["case_id"] in predictions for c in cases)}
                                        for f in sorted({c["family"] for c in cases})}}
    out["sensitivity"] = {"stable": sum(r["sensitivity"]["stable"] for r in rows),
                          "unstable": sum(not r["sensitivity"]["stable"] for r in rows)}
    out["paired_gate_vs_lexical"] = {"corrected": sum(r["gated"] == expected[r["case_id"]]
                                                      and r["lexical"] != expected[r["case_id"]] for r in rows),
                                      "regressed": sum(r["gated"] != expected[r["case_id"]]
                                                       and r["lexical"] == expected[r["case_id"]] for r in rows)}
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=Path, required=True)
    ap.add_argument("--cache", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    frozen = HERE / "cases.json"
    cases = json.loads(frozen.read_text())
    assert digest(args.model) == MODEL_SHA
    fingerprint = {"cases_sha256": digest(frozen), "protocol_sha256": digest(HERE / "protocol.py"),
                   "runner_sha256": digest(Path(__file__)), "model_sha256": MODEL_SHA}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    if args.out.exists():
        report = json.loads(args.out.read_text())
        assert report["fingerprint"] == fingerprint
    else:
        report = {"fingerprint": fingerprint, "calls": {}, "rows": [], "summary": {}}
    sys.path.insert(0, str(HERE.parent / "epistemic-qwen-local-2026-11-14" / "runtime"))
    from gguf_qwen3_experimental import GGUF, Runner, Store, Tokenizer
    from eval_choice_model import enc_chat
    from protocol import previous
    import torch
    model = GGUF(args.model)
    runner, tok = Runner(model, Store(model, args.cache)), Tokenizer(model.meta)
    report["runtime"] = {"torch": torch.__version__, "threads": torch.get_num_threads(),
                         "method": "experimental GGUF first-token constrained logits"}
    report["provider"] = "local Qwen3-1.7B-Q4_K_M"

    def call(key, prompt, mapping):
        if key not in report["calls"]:
            report["calls"][key] = infer(runner, tok, enc_chat, prompt, mapping)
            save(args.out, report)
        return report["calls"][key]["value"]

    done = {r["case_id"] for r in report["rows"]}
    for case in cases:
        cid = case["case_id"]
        if cid in done:
            continue
        base = [lexical(parse_visible(r["text"])[2]) for r in case["records"]]
        old = [previous.lexical(parse_visible(r["text"])[2]) or "A" for r in case["records"]]
        selected, ids = list(base), []
        for i, record in enumerate(case["records"]):
            if base[i] == "A":
                key = f"{case['pair_id']}:parse:{i}"
                selected[i] = call(key, parser_prompt(record), record["mapping"])
                ids.append(key)
        direct_id = cid + ":direct"
        direct = call(direct_id, direct_prompt(case), case["mapping"])
        gated, trace = interpret(case, selected)
        sens = sensitivity(case, base, selected)
        row = {"case_id": cid, "expected": case["expected"], "lexical": interpret(case, base)[0],
               "old_lexical": interpret(case, old)[0], "gated": gated, "direct": direct,
               "gated_stable_only": gated if sens["stable"] else "ABSTAIN", "sensitivity": sens,
               "gate_calls": ids, "direct_call": direct_id, "lexical_relations": base,
               "gated_relations": selected, "gold_relations": [r["gold"] for r in case["records"]],
               "gated_trace": trace}
        report["rows"].append(row)
        report["summary"] = summarize(cases, report)
        save(args.out, report)
        print(f"{len(report['rows'])}/{len(cases)} {cid} expected={case['expected']} "
              f"direct={direct} lexical={row['lexical']} gated={gated} stable={sens['stable']}", flush=True)
    print(json.dumps(report["summary"], indent=2), flush=True)


if __name__ == "__main__":
    main()
