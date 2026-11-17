"""DeepSeek runner for frozen Epistemic Challenge 004.

Pure stdlib. Resumable. Every completed API call is atomically persisted.
DeepSeek is a research surrogate for strong local-model capability, not a
production dependency assumption.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import time
import urllib.error
import urllib.request
from pathlib import Path

from protocol import (
    HERE, build_cases, corpus_sha256, corpus_summary,
    deterministic_relations, escalation_sets, interpret,
)

API_URL = "https://api.deepseek.com/chat/completions"
MODEL = "deepseek-flash"
PEAK_INPUT_MISS_PER_M = 0.30
PEAK_OUTPUT_PER_M = 1.20
NORMAL_MAX_TOKENS = 16
HIGH_MAX_TOKENS = 2048
MAX_MAX_TOKENS = 8192

RELATION_DESCRIPTIONS = {
    "K": "knowledge or verified fact; this entails the payload",
    "B": "belief, guess, suspicion or working assumption; does not entail the payload",
    "R": "report of somebody's claim; does not entail the payload",
    "N": "explicit lack of knowledge of the payload",
    "A": "ambiguous or insufficient wording: abstain",
}
SHARED = (
    "Synthetic case. K(person,time,p) is factive and entails p. B is belief and R is report; "
    "neither entails p. N means explicit lack of knowledge (!K), not falsity of p. "
    "A/ambiguous contributes no premise. Apply only the listed forward rules. No time persistence, "
    "no closed-world assumption, no contraposition, and no explosion from unrelated contradictions. "
    "SUPPORTED means q only; REFUTED means !q only; CONFLICT means both; UNRESOLVED means neither."
)


class BudgetStop(RuntimeError):
    pass


def stable_json(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def file_sha(path: Path) -> str:
    data = path.read_bytes().replace(b"\r\n", b"\n")
    return hashlib.sha256(data).hexdigest()


def save_atomic(path: Path, report: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    os.replace(tmp, path)


def choices(mapping: dict, descriptions: dict | None = None) -> str:
    return "\n".join(
        f"{letter}: {descriptions[value] if descriptions else value}"
        for letter, value in mapping.items()
    )


def direct_messages(case: dict) -> list[dict]:
    records = "\n".join(record["text"] for record in case["records"])
    rules = "\n".join(
        "IF " + " AND ".join(item["if"]) + " THEN " + item["then"]
        for item in case["rules"]
    ) or "(none)"
    givens = ", ".join(case["givens"]) or "(none)"
    user = (
        f"Records:\n{records}\n"
        f"Stipulated facts:\n{givens}\n"
        f"Forward rules:\n{rules}\n"
        f"Query: {case['query']}\n"
        f"{choices(case['mapping'])}\n"
        "Return the correct choice letter only in the final answer."
    )
    return [{"role": "system", "content": SHARED}, {"role": "user", "content": user}]


def parser_messages(record: dict) -> list[dict]:
    system = (
        "Classify only the OUTER epistemic relation expressed by the record wording. "
        "Do not infer truth from a report or belief. Do not classify the nested payload. "
        "If the wording does not clearly determine one relation, choose ambiguous/abstain."
    )
    user = (
        f"{record['text']}\n"
        f"{choices(record['mapping'], RELATION_DESCRIPTIONS)}\n"
        "Return the correct choice letter only in the final answer."
    )
    return [{"role": "system", "content": system}, {"role": "user", "content": user}]


def extract_letter(content: str, allowed: list[str]) -> str:
    cleaned = (content or "").strip().upper()
    if cleaned in allowed:
        return cleaned
    match = re.search(r"\b(" + "|".join(map(re.escape, allowed)) + r")\b", cleaned)
    if match:
        return match.group(1)
    raise ValueError(f"No allowed answer letter {allowed!r} in final content: {content!r}")


def usage_reasoning_tokens(usage: dict) -> int:
    details = usage.get("completion_tokens_details") or {}
    return int(details.get("reasoning_tokens") or usage.get("reasoning_tokens") or 0)


def peak_cost_from_usage(usage: dict, input_price: float, output_price: float) -> float:
    prompt = int(usage.get("prompt_tokens") or 0)
    completion = int(usage.get("completion_tokens") or 0)
    return prompt * input_price / 1_000_000 + completion * output_price / 1_000_000


def accumulated_peak_cost(calls: dict, input_price: float, output_price: float) -> float:
    return sum(peak_cost_from_usage(call.get("usage", {}), input_price, output_price)
               for call in calls.values())


def api_call(api_key: str, messages: list[dict], mapping: dict, effort: str,
             timeout: int, retries: int, max_tokens: int) -> dict:
    payload = {"model": MODEL, "messages": messages, "max_tokens": max_tokens, "stream": False}
    if effort == "normal":
        payload["thinking"] = {"type": "disabled"}
        payload["temperature"] = 0
    else:
        payload["thinking"] = {"type": "enabled"}
        payload["reasoning_effort"] = effort

    body = json.dumps(payload).encode("utf-8")
    last_error = None
    for attempt in range(retries + 1):
        request = urllib.request.Request(
            API_URL, data=body, method="POST",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
                "User-Agent": "niccolo-epistemic-challenge-004/1",
            },
        )
        start = time.monotonic()
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                raw = response.read().decode("utf-8")
            elapsed = time.monotonic() - start
            data = json.loads(raw)
            message = data["choices"][0]["message"]
            final = message.get("content") or ""
            selected = extract_letter(final, list(mapping))
            usage = data.get("usage") or {}
            return {
                "selected": selected,
                "value": mapping[selected],
                "final_content": final,
                "reasoning_content": message.get("reasoning_content"),
                "usage": usage,
                "reasoning_tokens": usage_reasoning_tokens(usage),
                "seconds": round(elapsed, 5),
                "finish_reason": data["choices"][0].get("finish_reason"),
                "model_returned": data.get("model"),
                "effort": effort,
                "max_tokens": max_tokens,
                "messages": messages,
            }
        except urllib.error.HTTPError as exc:
            raw = exc.read().decode("utf-8", "replace")
            last_error = RuntimeError(f"HTTP {exc.code}: {raw}")
            if exc.code not in (408, 409, 429, 500, 502, 503, 504) or attempt >= retries:
                raise last_error
        except (urllib.error.URLError, TimeoutError) as exc:
            last_error = exc
            if attempt >= retries:
                raise
        time.sleep(min(2 ** attempt, 16))
    raise RuntimeError(last_error)


def call_key(kind: str, effort: str, messages: list[dict]) -> str:
    digest = hashlib.sha256(stable_json(messages).encode("utf-8")).hexdigest()[:24]
    return f"{kind}:{effort}:{digest}"


def false_certainty(prediction: str, expected: str) -> bool:
    return prediction in ("SUPPORTED", "REFUTED") and expected in ("UNRESOLVED", "CONFLICT")


def metric(rows: list[dict], field: str) -> dict:
    selected = [row for row in rows if field in row and row[field] is not None]
    correct = sum(row[field] == row["expected"] for row in selected)
    holdout_n = sum(row["holdout"] for row in selected)
    holdout_correct = sum(row["holdout"] and row[field] == row["expected"] for row in selected)
    return {
        "correct": correct,
        "n": len(selected),
        "accuracy": round(correct / len(selected), 6) if selected else None,
        "false_certainty": sum(false_certainty(row[field], row["expected"]) for row in selected),
        "holdout_correct": holdout_correct,
        "holdout_n": holdout_n,
        "holdout_accuracy": round(holdout_correct / holdout_n, 6) if holdout_n else None,
    }


def summarize(cases: list[dict], report: dict) -> dict:
    rows = report["rows"]
    result = {
        name: metric(rows, name)
        for name in (
            "deterministic", "direct_normal", "gated_normal",
            "gated_adaptive_high", "gated_adaptive_max",
            "direct_high", "direct_max",
        )
    }
    result["gated_normal_vs_deterministic"] = {
        "corrected": sum(r["gated_normal"] == r["expected"] and r["deterministic"] != r["expected"] for r in rows),
        "regressed": sum(r["gated_normal"] != r["expected"] and r["deterministic"] == r["expected"] for r in rows),
    }
    result["adaptive_max_vs_normal"] = {
        "corrected": sum(r["gated_adaptive_max"] == r["expected"] and r["gated_normal"] != r["expected"] for r in rows),
        "regressed": sum(r["gated_adaptive_max"] != r["expected"] and r["gated_normal"] == r["expected"] for r in rows),
    }
    calls = report["calls"]
    usage = {
        "calls": len(calls),
        "prompt_tokens": sum(int(c.get("usage", {}).get("prompt_tokens") or 0) for c in calls.values()),
        "completion_tokens": sum(int(c.get("usage", {}).get("completion_tokens") or 0) for c in calls.values()),
        "reasoning_tokens": sum(int(c.get("reasoning_tokens") or 0) for c in calls.values()),
        "api_seconds": round(sum(float(c.get("seconds") or 0) for c in calls.values()), 3),
    }
    usage["peak_price_estimate_usd"] = round(accumulated_peak_cost(
        calls, report["pricing"]["input_per_million"], report["pricing"]["output_per_million"]), 6)
    result["usage"] = usage
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=HERE / "results" / "deepseek-observations.json")
    parser.add_argument("--budget-usd", type=float, default=20.0,
                        help="Conservative stop based on configured peak cache-miss prices.")
    parser.add_argument("--max-new-calls", type=int, default=0,
                        help="0 means unlimited; otherwise stop cleanly after this many new API calls.")
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--retries", type=int, default=4)
    parser.add_argument("--input-price-per-million", type=float, default=PEAK_INPUT_MISS_PER_M)
    parser.add_argument("--output-price-per-million", type=float, default=PEAK_OUTPUT_PER_M)
    args = parser.parse_args()

    api_key = os.environ.get("DEEPSEEK_API_KEY")
    if not api_key:
        raise SystemExit("DEEPSEEK_API_KEY is not set")

    cases = build_cases()
    frozen = corpus_summary(cases)
    manifest = json.loads((HERE / "FROZEN_MANIFEST.json").read_text(encoding="utf-8"))
    for key in ("cases", "corpus_sha256", "high_ids_sha256", "max_ids_sha256"):
        if manifest[key] != frozen[key]:
            raise SystemExit(f"Frozen corpus mismatch for {key}: manifest={manifest[key]} generated={frozen[key]}")
    high_ids, max_ids = escalation_sets(cases)

    fingerprint = {
        "corpus_sha256": corpus_sha256(cases),
        "protocol_sha256": file_sha(HERE / "protocol.py"),
        "runner_sha256": file_sha(Path(__file__)),
        "manifest_sha256": file_sha(HERE / "FROZEN_MANIFEST.json"),
        "model": MODEL,
        "normal_max_tokens": NORMAL_MAX_TOKENS,
        "high_max_tokens": HIGH_MAX_TOKENS,
        "max_max_tokens": MAX_MAX_TOKENS,
    }

    if args.out.exists():
        report = json.loads(args.out.read_text(encoding="utf-8"))
        if report.get("fingerprint") != fingerprint:
            raise SystemExit("Existing output fingerprint differs. Preserve it and use a new output path.")
    else:
        report = {
            "fingerprint": fingerprint,
            "provider": "DeepSeek API surrogate",
            "api_url": API_URL,
            "pricing": {
                "basis": "conservative configured peak cache-miss estimate",
                "input_per_million": args.input_price_per_million,
                "output_per_million": args.output_price_per_million,
            },
            "corpus": frozen,
            "calls": {}, "rows": [], "summary": {},
        }
        save_atomic(args.out, report)

    new_calls = 0

    def call(kind: str, effort: str, messages: list[dict], mapping: dict):
        nonlocal new_calls
        key = call_key(kind, effort, messages)
        if key not in report["calls"]:
            if args.max_new_calls and new_calls >= args.max_new_calls:
                raise BudgetStop(f"max-new-calls reached: {args.max_new_calls}")
            spent = accumulated_peak_cost(report["calls"], args.input_price_per_million, args.output_price_per_million)
            if spent >= args.budget_usd:
                raise BudgetStop(f"budget reached: estimated peak cost ${spent:.4f} >= ${args.budget_usd:.4f}")
            cap = {"normal": NORMAL_MAX_TOKENS, "high": HIGH_MAX_TOKENS, "max": MAX_MAX_TOKENS}[effort]
            report["calls"][key] = api_call(api_key, messages, mapping, effort, args.timeout, args.retries, cap)
            new_calls += 1
            save_atomic(args.out, report)
        return key, report["calls"][key]["value"]

    done = {row["case_id"] for row in report["rows"]}
    try:
        for case in cases:
            if case["case_id"] in done:
                continue
            deterministic = deterministic_relations(case)
            normal = list(deterministic)
            normal_keys = []
            for i, relation in enumerate(deterministic):
                if relation == "A":
                    key, value = call("parse", "normal", parser_messages(case["records"][i]), case["records"][i]["mapping"])
                    normal[i] = value
                    normal_keys.append(key)

            deterministic_verdict = interpret(case, deterministic)[0]
            gated_normal = interpret(case, normal)[0]

            high = list(normal)
            high_keys = []
            direct_high = None
            if case["case_id"] in high_ids:
                for i, relation in enumerate(deterministic):
                    if relation == "A":
                        key, value = call("parse", "high", parser_messages(case["records"][i]), case["records"][i]["mapping"])
                        high[i] = value
                        high_keys.append(key)
                _, direct_high = call("direct", "high", direct_messages(case), case["mapping"])
            adaptive_high = interpret(case, high)[0]

            maximum = list(high)
            max_keys = []
            direct_max = None
            if case["case_id"] in max_ids:
                for i, relation in enumerate(deterministic):
                    if relation == "A":
                        key, value = call("parse", "max", parser_messages(case["records"][i]), case["records"][i]["mapping"])
                        maximum[i] = value
                        max_keys.append(key)
                _, direct_max = call("direct", "max", direct_messages(case), case["mapping"])
            adaptive_max = interpret(case, maximum)[0]

            direct_key, direct_normal = call("direct", "normal", direct_messages(case), case["mapping"])

            row = {
                "case_id": case["case_id"], "base_id": case["base_id"], "phase": case["phase"],
                "family": case["family"], "holdout": case["holdout"], "expected": case["expected"],
                "structural_score": __import__("protocol").structural_score(case),
                "high_selected": case["case_id"] in high_ids, "max_selected": case["case_id"] in max_ids,
                "deterministic": deterministic_verdict, "direct_normal": direct_normal,
                "gated_normal": gated_normal, "gated_adaptive_high": adaptive_high,
                "gated_adaptive_max": adaptive_max, "direct_high": direct_high, "direct_max": direct_max,
                "deterministic_relations": deterministic, "normal_relations": normal,
                "high_relations": high if case["case_id"] in high_ids else None,
                "max_relations": maximum if case["case_id"] in max_ids else None,
                "normal_parse_calls": normal_keys, "high_parse_calls": high_keys,
                "max_parse_calls": max_keys, "direct_normal_call": direct_key,
            }
            report["rows"].append(row)
            report["summary"] = summarize(cases, report)
            save_atomic(args.out, report)
            if len(report["rows"]) % 10 == 0 or len(report["rows"]) == len(cases):
                print(
                    f"{len(report['rows'])}/{len(cases)} "
                    f"cost<=${report['summary']['usage']['peak_price_estimate_usd']:.4f} "
                    f"direct={report['summary']['direct_normal']['accuracy']} "
                    f"gated={report['summary']['gated_normal']['accuracy']} "
                    f"adaptive={report['summary']['gated_adaptive_max']['accuracy']}",
                    flush=True,
                )
    except BudgetStop as stop:
        report["summary"] = summarize(cases, report)
        report["stopped"] = str(stop)
        save_atomic(args.out, report)
        print(f"STOPPED CLEANLY: {stop}")
        print(json.dumps(report["summary"], indent=2))
        return

    report.pop("stopped", None)
    report["summary"] = summarize(cases, report)
    save_atomic(args.out, report)
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
