"""Run paired baseline/hybrid experiment with real CPU Qwen logits."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import sys
import time
from collections import Counter
from pathlib import Path

from protocol import LABELS, LETTERS, RELATIONS, interpret, lexical, parse_visible

HERE = Path(__file__).resolve().parent
MODEL_SHA = "d2387ca2dbfee2ffabce7120d3770dadca0b293052bc2f0e138fdc940d9bc7b5"
TREATMENTS = ("direct", "lexical", "hybrid", "hybrid_challenge")
DESCRIPTIONS = {
    "K": "knowledge (factive; content true)",
    "B": "belief (not necessarily true)",
    "R": "report or communication (not necessarily true)",
    "N": "explicit absence of knowledge (not a denial of the content)",
}
SHARED = (
    "Synthetic controlled records. K(a,t,p)=a knows p at time t; B=believes; "
    "R=reports. !p explicitly negates p. K entails its content, even nested. "
    "B and R do not entail content. Absence of knowledge negates K, not p. "
    "Apply listed IF rules forward only. No temporal persistence, no closed-world "
    "negation, no inference from unrelated contradictions. "
    "Both q and !q means CONFLICT; q only SUPPORTED; !q only REFUTED; "
    "neither UNRESOLVED."
)
PARSE = (
    "Classify the relation expressed by the wording of a synthetic record "
    "time | person | wording | content. Do not infer truth from belief or a report. "
    "Absence of knowledge is distinct from a belief in the opposite."
)
CHALLENGE_EXAMPLES = (
    " Treat as a test challenge. Test definitions and verified expected relations: "
    "'knows' => K; 'guesses' => B; 'read aloud the claim' => R; "
    "'does not know' => N. Check only the record's outer relation, not inner content."
)


def chat(system, user):
    return (f"<|im_start|>system\n{system}<|im_end|>\n"
            f"<|im_start|>user\n{user}<|im_end|>\n"
            "<|im_start|>assistant\n<think>\n\n</think>\n")


def choices(mapping, descriptions=None):
    return "\n".join(f"{letter}: {descriptions[v] if descriptions else v}"
                     for letter, v in mapping.items())


def direct_prompt(case):
    rules = "\n".join("IF " + " AND ".join(r["if"]) + " THEN " + r["then"]
                      for r in case["rules"]) or "(none)"
    user = (f"Record: {case['record']}\n"
            f"Other stipulated facts: {', '.join(case['givens']) or '(none)'}\n"
            f"Rules:\n{rules}\nQuery: {case['query']}\n"
            f"{choices(case['answer_mapping'])}\nChoose one letter only.")
    return chat(SHARED, user)


def parser_prompt(case, challenge=False):
    system = PARSE + (CHALLENGE_EXAMPLES if challenge else "")
    return chat(system, f"Record: {case['record']}\n"
                f"{choices(case['parse_mapping'], DESCRIPTIONS)}\nChoose one letter only.")


def file_hash(path):
    with open(path, "rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def save(path, data):
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    os.replace(tmp, path)


def inference(runner, tok, encode, prompt, mapping):
    tokens = encode(tok, prompt)
    ids = [tok.encode(x) for x in LETTERS]
    assert all(len(x) == 1 for x in ids) and len({x[0] for x in ids}) == 4
    runner.cache = []
    started = time.monotonic()
    _, logits = runner.forward(tokens, 0, choices=[x[0] for x in ids])
    assert all(math.isfinite(x) for x in logits)
    letter = LETTERS[max(range(4), key=logits.__getitem__)]
    return {"selected": letter, "value": mapping[letter],
            "logits_uncalibrated": dict(zip(LETTERS, [round(x, 5) for x in logits])),
            "seconds": round(time.monotonic() - started, 4),
            "tokens": len(tokens), "prompt": prompt}


def summarize(cases, rows):
    by_case = {c["case_id"]: c for c in cases}
    treatments = {}
    for treatment in TREATMENTS:
        selected = [r for r in rows if r["treatment"] == treatment]
        by_pair = {}
        for r in selected:
            c = by_case[r["case_id"]]
            by_pair.setdefault(c["pair_id"], {})[c["phase"]] = r
        full = [p for p in by_pair.values() if set(p) == {0, 1}]
        changes = [p for p in full if p[0]["expected"] != p[1]["expected"]]
        unchanged = [p for p in full if p[0]["expected"] == p[1]["expected"]]
        unique_calls = {r["call_id"]: r["inference"] for r in selected
                        if r.get("inference") is not None}
        by_family = {}
        for family in sorted({by_case[r["case_id"]]["family"] for r in selected}):
            f = [r for r in selected if by_case[r["case_id"]]["family"] == family]
            by_family[family] = {"n": len(f), "correct": sum(x["match"] for x in f)}
        treatments[treatment] = {
            "n": len(selected), "correct": sum(r["match"] for r in selected),
            "false_certainty": sum(r["expected"] in ("UNRESOLVED", "CONFLICT")
                                   and r["observed"] in ("SUPPORTED", "REFUTED") for r in selected),
            "false_certainty_eligible": sum(r["expected"] in ("UNRESOLVED", "CONFLICT") for r in selected),
            "unresolved_or_conflict": sum(r["observed"] in ("UNRESOLVED", "CONFLICT") for r in selected),
            "pairs_complete": len(full),
            "pairs_both_correct": sum(p[0]["match"] and p[1]["match"] for p in full),
            "changed_gold_pairs": len(changes),
            "changed_pairs_both_correct": sum(p[0]["match"] and p[1]["match"] for p in changes),
            "unchanged_gold_pairs": len(unchanged),
            "unchanged_pairs_both_correct": sum(p[0]["match"] and p[1]["match"] for p in unchanged),
            "spurious_revisions": sum(p[0]["observed"] != p[1]["observed"] for p in unchanged),
            "inference_calls": len(unique_calls),
            "model_seconds": round(sum(r["seconds"] for r in unique_calls.values()), 3),
            "model_tokens": sum(r["tokens"] for r in unique_calls.values()),
            "parse_correct": sum(r.get("relation") == r.get("gold_relation") for r in selected
                                 if "relation" in r),
            "parse_abstentions": sum(r.get("relation") is None for r in selected if "relation" in r),
            "by_family": by_family,
            "confusion": {g: dict(Counter(r["observed"] for r in selected if r["expected"] == g))
                          for g in LABELS},
        }
    comparisons = {}
    rows_by = {(r["case_id"], r["treatment"]): r for r in rows}
    for a, b in (("direct", "hybrid"), ("lexical", "hybrid"),
                 ("hybrid", "hybrid_challenge")):
        matched = [c for c in cases if all((c["case_id"], t) in rows_by for t in (a, b))]
        groups = {}
        for c in matched:
            ar, br = (rows_by[c["case_id"], t]["match"] for t in (a, b))
            groups.setdefault(c["pair_id"], []).append(int(br) - int(ar))
        # Paired cluster bootstrap across narrative pairs; descriptive only,
        # since scenario modes are reused and the fixture population is synthetic.
        effects = [sum(v) / len(v) for v in groups.values() if len(v) == 2]
        ci = None
        if effects:
            rng = random.Random(91203)
            samples = sorted(sum(rng.choices(effects, k=len(effects))) / len(effects)
                             for _ in range(3000))
            ci = [round(samples[75], 3), round(samples[2924], 3)]
        comparisons[f"{b}_vs_{a}"] = {
            "matched_cases": len(matched),
            "b_correct_a_wrong": sum(not rows_by[c["case_id"], a]["match"]
                                      and rows_by[c["case_id"], b]["match"] for c in matched),
            "a_correct_b_wrong": sum(rows_by[c["case_id"], a]["match"]
                                      and not rows_by[c["case_id"], b]["match"] for c in matched),
            "cluster_bootstrap_interval_descriptive": ci,
        }
    return {"treatments": treatments, "comparisons": comparisons}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--model", type=Path, required=True)
    p.add_argument("--cache", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--limit", type=int, default=100)
    args = p.parse_args()
    frozen = HERE / "cases.json"
    cases = json.loads(frozen.read_text())[:args.limit]
    digest = file_hash(frozen)
    assert file_hash(args.model) == MODEL_SHA
    args.out.parent.mkdir(parents=True, exist_ok=True)
    if args.out.exists():
        report = json.loads(args.out.read_text())
        assert report["cases_sha256"] == digest
        assert report["model_sha256"] == MODEL_SHA
        assert report["limit"] == args.limit
    else:
        report = {"experiment": "NICCOLO-EPISTEMIC-CHALLENGE-002",
                  "cases_sha256": digest, "model_sha256": MODEL_SHA,
                  "limit": args.limit, "rows": [], "summary": {}}
    sys.path.insert(0, str(HERE.parent / "epistemic-qwen-local-2026-11-14" / "runtime"))
    from gguf_qwen3_experimental import GGUF, Runner, Store, Tokenizer
    from eval_choice_model import enc_chat
    import torch
    model = GGUF(args.model)
    runner = Runner(model, Store(model, args.cache))
    tok = Tokenizer(model.meta)
    report["runtime"] = {"torch": torch.__version__, "threads": torch.get_num_threads(),
                         "method": "experimental FP32 GGUF runtime, constrained first-token logits"}
    rows = report["rows"]
    done = {(r["case_id"], r["treatment"]) for r in rows}
    cache = {r["call_id"]: r["inference"] for r in rows if r.get("inference") is not None}
    for case in cases:
        for treatment in TREATMENTS:
            if (case["case_id"], treatment) in done:
                continue
            row = {"case_id": case["case_id"], "treatment": treatment,
                   "expected": case["expected"]}
            if treatment == "direct":
                call_id = case["case_id"] + ":direct"
                result = inference(runner, tok, enc_chat, direct_prompt(case),
                                   case["answer_mapping"])
                row.update(call_id=call_id, inference=result, observed=result["value"])
            else:
                if treatment == "lexical":
                    relation = lexical(parse_visible(case["record"])[2])
                else:
                    # The stable record, mapping and prompt are identical across
                    # revisions. Reuse exactly the same prior parse, not its verdict.
                    call_id = case["pair_id"] + ":" + treatment
                    if call_id not in cache:
                        cache[call_id] = inference(runner, tok, enc_chat,
                                                  parser_prompt(case, treatment == "hybrid_challenge"),
                                                  case["parse_mapping"])
                    result = cache[call_id]
                    relation = result["value"]
                    row.update(call_id=call_id, inference=result)
                observed, trace = interpret(case, relation)
                row.update(relation=relation, gold_relation=case["gold_relation"],
                           observed=observed, verifier_trace=trace)
            row["match"] = row["observed"] == row["expected"]
            rows.append(row)
            print("RESULT", len(rows), case["case_id"], treatment,
                  row["observed"], row["expected"], "PASS" if row["match"] else "FAIL", flush=True)
            # Atomic incremental record; can resume without repeating inference.
            save(args.out, report)
    report["summary"] = summarize(cases, rows)
    save(args.out, report)
    print(json.dumps(report["summary"], indent=2), flush=True)


if __name__ == "__main__":
    main()
