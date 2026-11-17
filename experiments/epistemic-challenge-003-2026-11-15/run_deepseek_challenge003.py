#!/usr/bin/env python3
"""Run Epistemic Challenge 003 against the DeepSeek Chat Completions API.

This is a post-Qwen extension of the frozen Challenge 003 protocol. It does not
modify the frozen cases or deterministic verifier. No third-party Python package
is required. DEEPSEEK_API_KEY is read from the environment and is never written
to the report.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from protocol import interpret, lexical, parse_visible, sensitivity  # noqa: E402

API_URL = "https://api.deepseek.com/chat/completions"
DESCRIPTIONS = {
    "K": "knowledge or verified fact",
    "B": "belief, guess or suspicion",
    "R": "report of a claim, not necessarily true",
    "N": "explicit lack of knowledge",
    "A": "ambiguous: cannot clearly identify the relation",
}
SHARED = (
    "Synthetic records. K(person,time,p) means knows p and entails p, including nested content. "
    "B is belief, R is report; neither entails p. Explicit lack of knowledge means !K, not !p. "
    "Ambiguous records supply no premise. ! negates. Apply only forward IF rules; no time "
    "persistence, no closed world, no inference from unrelated contradiction. "
    "SUPPORTED if q only, REFUTED if !q only, CONFLICT if both, UNRESOLVED if neither."
)


def digest(path: Path) -> str:
    """Canonical text hash: repository LF and Windows CRLF represent the same experiment source."""
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
    rules = "\n".join(
        "IF " + " AND ".join(rule["if"]) + " THEN " + rule["then"]
        for rule in case["rules"]
    )
    records = "\n".join(record["text"] for record in case["records"])
    user = (
        f"Records:\n{records}\n"
        f"Stipulated facts: {', '.join(case['givens']) or '(none)'}\n"
        f"Rules:\n{rules or '(none)'}\n"
        f"Query: {case['query']}\n"
        f"{choices(case['mapping'])}\n"
        "Choose one letter only."
    )
    return [
        {"role": "system", "content": SHARED},
        {"role": "user", "content": user},
    ]


def parser_messages(record: dict) -> list[dict]:
    system = (
        "Classify only the outer relation expressed by the wording in "
        "time | person | wording | content. Do not infer truth from a report or belief. "
        "If wording is ambiguous, abstain."
    )
    user = (
        f"{record['text']}\n"
        f"{choices(record['mapping'], DESCRIPTIONS)}\n"
        "Choose one letter only."
    )
    return [
        {"role": "system", "content": system},
        {"role": "user", "content": user},
    ]


def extract_letter(content: str, allowed: list[str]) -> str:
    cleaned = content.strip().upper()
    if cleaned in allowed:
        return cleaned
    match = re.search(r"\b(" + "|".join(map(re.escape, allowed)) + r")\b", cleaned)
    if not match:
        raise ValueError(
            f"DeepSeek did not return one of {allowed!r}. Raw content: {content!r}"
        )
    return match.group(1)


def api_call(
    api_key: str,
    model: str,
    messages: list[dict],
    allowed_letters: list[str],
    thinking: str,
    timeout: int,
    retries: int,
) -> dict:
    payload = {
        "model": model,
        "messages": messages,
        "thinking": {"type": thinking},
        "max_tokens": 8,
        "temperature": 0,
        "stream": False,
    }
    body = json.dumps(payload).encode("utf-8")
    last_error: Exception | None = None

    for attempt in range(retries + 1):
        request = urllib.request.Request(
            API_URL,
            data=body,
            method="POST",
            headers={
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json",
                "User-Agent": "niccolo-epistemic-challenge-003-deepseek/1",
            },
        )
        start = time.monotonic()
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                raw = response.read().decode("utf-8")
            elapsed = time.monotonic() - start
            data = json.loads(raw)
            content = data["choices"][0]["message"]["content"]
            selected = extract_letter(content, allowed_letters)
            return {
                "selected": selected,
                "raw_content": content,
                "seconds": round(elapsed, 5),
                "usage": data.get("usage", {}),
                "model_returned": data.get("model"),
                "finish_reason": data["choices"][0].get("finish_reason"),
                "messages": messages,
            }
        except urllib.error.HTTPError as exc:
            raw = exc.read().decode("utf-8", "replace")
            last_error = RuntimeError(f"HTTP {exc.code}: {raw}")
            retryable = exc.code in (408, 409, 429, 500, 502, 503, 504)
            if not retryable or attempt >= retries:
                raise last_error
        except (urllib.error.URLError, TimeoutError) as exc:
            last_error = exc
            if attempt >= retries:
                raise
        time.sleep(min(2 ** attempt, 16))

    raise RuntimeError(last_error)


def false_certainty(prediction: str, expected: str) -> bool:
    return (
        prediction in ("SUPPORTED", "REFUTED")
        and expected in ("UNRESOLVED", "CONFLICT")
    )


def summarize(cases: list[dict], rows: list[dict], calls: dict) -> dict:
    expected = {case["case_id"]: case["expected"] for case in cases}
    pair_ids = {case["case_id"]: case["pair_id"] for case in cases}

    def one(name: str) -> dict:
        predictions = {row["case_id"]: row[name] for row in rows}
        by_pair: dict[str, list[bool]] = {}
        for case_id, prediction in predictions.items():
            by_pair.setdefault(pair_ids[case_id], []).append(
                prediction == expected[case_id]
            )

        call_ids: list[str] = []
        if name == "direct":
            call_ids = [row["direct_call"] for row in rows]
        elif name in ("gated", "gated_stable_only"):
            call_ids = sorted({key for row in rows for key in row["gate_calls"]})

        usage = [calls[key].get("usage", {}) for key in call_ids]
        correct = sum(
            prediction == expected[case_id]
            for case_id, prediction in predictions.items()
        )
        return {
            "correct": correct,
            "n": len(predictions),
            "accuracy": round(correct / len(predictions), 4) if predictions else None,
            "false_certainty": sum(
                false_certainty(prediction, expected[case_id])
                for case_id, prediction in predictions.items()
            ),
            "abstain": sum(x == "ABSTAIN" for x in predictions.values()),
            "pairs_both_correct": sum(
                len(values) == 2 and all(values) for values in by_pair.values()
            ),
            "model_calls": len(call_ids),
            "prompt_tokens": sum(x.get("prompt_tokens", 0) for x in usage),
            "completion_tokens": sum(x.get("completion_tokens", 0) for x in usage),
            "total_tokens": sum(x.get("total_tokens", 0) for x in usage),
            "api_seconds": round(sum(calls[key]["seconds"] for key in call_ids), 3),
        }

    result = {
        "direct": one("direct"),
        "lexical": one("lexical"),
        "gated": one("gated"),
        "gated_stable_only": one("gated_stable_only"),
    }
    result["paired_gate_vs_lexical"] = {
        "corrected": sum(
            row["gated"] == expected[row["case_id"]]
            and row["lexical"] != expected[row["case_id"]]
            for row in rows
        ),
        "regressed": sum(
            row["gated"] != expected[row["case_id"]]
            and row["lexical"] == expected[row["case_id"]]
            for row in rows
        ),
    }
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--model",
        default=os.environ.get("DEEPSEEK_MODEL", "deepseek-flash"),
        help="DeepSeek model (default: DEEPSEEK_MODEL or deepseek-flash)",
    )
    parser.add_argument(
        "--thinking",
        choices=("enabled", "disabled"),
        default="disabled",
        help="Keep disabled for the first comparable/economical run.",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=HERE / "results" / "deepseek-observations.json",
    )
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--retries", type=int, default=4)
    args = parser.parse_args()

    api_key = os.environ.get("DEEPSEEK_API_KEY")
    if not api_key:
        raise SystemExit(
            "DEEPSEEK_API_KEY is not set. In Git Bash:\n"
            '  read -rsp "DeepSeek API key: " DEEPSEEK_API_KEY; echo\n'
            "  export DEEPSEEK_API_KEY"
        )

    cases_path = HERE / "cases.json"
    protocol_path = HERE / "protocol.py"
    cases = json.loads(cases_path.read_text(encoding="utf-8"))

    fingerprint = {
        "cases_sha256": digest(cases_path),
        "protocol_sha256": digest(protocol_path),
        "runner_sha256": digest(Path(__file__)),
        "model_requested": args.model,
        "thinking": args.thinking,
    }

    if args.out.exists():
        report = json.loads(args.out.read_text(encoding="utf-8"))
        if report.get("fingerprint") != fingerprint:
            raise SystemExit(
                f"{args.out} was created with a different cases/protocol/runner/model "
                "configuration. Move or rename it before starting this run."
            )
    else:
        report = {
            "fingerprint": fingerprint,
            "provider": "DeepSeek API",
            "api_url": API_URL,
            "calls": {},
            "rows": [],
            "summary": {},
        }
        save_atomic(args.out, report)

    def call(key: str, messages: list[dict], mapping: dict) -> str:
        if key not in report["calls"]:
            result = api_call(
                api_key=api_key,
                model=args.model,
                messages=messages,
                allowed_letters=list(mapping.keys()),
                thinking=args.thinking,
                timeout=args.timeout,
                retries=args.retries,
            )
            result["value"] = mapping[result["selected"]]
            report["calls"][key] = result
            save_atomic(args.out, report)
        return report["calls"][key]["value"]

    done = {row["case_id"] for row in report["rows"]}

    for case in cases:
        case_id = case["case_id"]
        if case_id in done:
            continue

        deterministic = [
            lexical(parse_visible(record["text"])[2]) for record in case["records"]
        ]
        selected = list(deterministic)
        gate_call_ids: list[str] = []

        for index, record in enumerate(case["records"]):
            if deterministic[index] == "A":
                key = f"{case['pair_id']}:parse:{index}"
                selected[index] = call(
                    key, parser_messages(record), record["mapping"]
                )
                gate_call_ids.append(key)

        direct_key = case_id + ":direct"
        direct = call(direct_key, direct_messages(case), case["mapping"])

        lexical_verdict = interpret(case, deterministic)[0]
        gated, trace = interpret(case, selected)
        sens = sensitivity(case, deterministic, selected)

        row = {
            "case_id": case_id,
            "expected": case["expected"],
            "lexical": lexical_verdict,
            "gated": gated,
            "direct": direct,
            "gated_stable_only": gated if sens["stable"] else "ABSTAIN",
            "sensitivity": sens,
            "gate_calls": gate_call_ids,
            "direct_call": direct_key,
            "lexical_relations": deterministic,
            "gated_relations": selected,
            "gold_relations": [record["gold"] for record in case["records"]],
            "gated_trace": trace,
        }
        report["rows"].append(row)
        report["summary"] = summarize(cases, report["rows"], report["calls"])
        save_atomic(args.out, report)

        print(
            f"{len(report['rows'])}/{len(cases)} {case_id} "
            f"expected={case['expected']} direct={direct} "
            f"lexical={lexical_verdict} gated={gated} stable={sens['stable']}",
            flush=True,
        )

    report["summary"] = summarize(cases, report["rows"], report["calls"])
    save_atomic(args.out, report)
    print("\nSUMMARY")
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    main()
