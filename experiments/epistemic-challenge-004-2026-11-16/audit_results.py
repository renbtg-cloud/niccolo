"""Replay Challenge 004 observations without a model or network."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import protocol
import run_deepseek as runner


def audit(report: dict) -> dict:
    cases = protocol.build_cases()
    summary = protocol.corpus_summary(cases)
    manifest = json.loads((protocol.HERE / "FROZEN_MANIFEST.json").read_text(encoding="utf-8"))
    for key in ("cases", "corpus_sha256", "high_ids_sha256", "max_ids_sha256"):
        assert manifest[key] == summary[key]
    assert report["fingerprint"]["corpus_sha256"] == summary["corpus_sha256"]
    assert report["fingerprint"]["protocol_sha256"] == runner.file_sha(protocol.HERE / "protocol.py")
    assert report["fingerprint"]["manifest_sha256"] == runner.file_sha(protocol.HERE / "FROZEN_MANIFEST.json")
    assert report["fingerprint"]["model"] == runner.MODEL

    high_ids, max_ids = protocol.escalation_sets(cases)
    case_by_id = {case["case_id"]: case for case in cases}
    used = set()

    def check_call(kind: str, effort: str, messages: list[dict], mapping: dict):
        key = runner.call_key(kind, effort, messages)
        used.add(key)
        call = report["calls"][key]
        assert call["messages"] == messages
        assert call["effort"] == effort
        assert call["selected"] in mapping
        assert call["value"] == mapping[call["selected"]]
        assert call["usage"].get("prompt_tokens", 0) > 0
        assert call["seconds"] >= 0
        return key, call["value"]

    seen_ids = set()
    for row in report["rows"]:
        case = case_by_id[row["case_id"]]
        assert row["case_id"] not in seen_ids
        seen_ids.add(row["case_id"])
        assert row["expected"] == case["expected"]
        assert row["holdout"] == case["holdout"]
        assert row["high_selected"] == (case["case_id"] in high_ids)
        assert row["max_selected"] == (case["case_id"] in max_ids)

        deterministic = protocol.deterministic_relations(case)
        assert row["deterministic_relations"] == deterministic
        assert row["deterministic"] == protocol.interpret(case, deterministic)[0]

        normal = list(deterministic)
        normal_keys = []
        for i, relation in enumerate(deterministic):
            if relation == "A":
                key, value = check_call("parse", "normal", runner.parser_messages(case["records"][i]), case["records"][i]["mapping"])
                normal[i] = value
                normal_keys.append(key)
        assert row["normal_parse_calls"] == normal_keys
        assert row["normal_relations"] == normal
        assert row["gated_normal"] == protocol.interpret(case, normal)[0]

        high = list(normal)
        high_keys = []
        direct_high = None
        if case["case_id"] in high_ids:
            for i, relation in enumerate(deterministic):
                if relation == "A":
                    key, value = check_call("parse", "high", runner.parser_messages(case["records"][i]), case["records"][i]["mapping"])
                    high[i] = value
                    high_keys.append(key)
            _, direct_high = check_call("direct", "high", runner.direct_messages(case), case["mapping"])
            assert row["high_relations"] == high
        else:
            assert row["high_relations"] is None
        assert row["high_parse_calls"] == high_keys
        assert row["direct_high"] == direct_high
        assert row["gated_adaptive_high"] == protocol.interpret(case, high)[0]

        maximum = list(high)
        max_keys = []
        direct_max = None
        if case["case_id"] in max_ids:
            for i, relation in enumerate(deterministic):
                if relation == "A":
                    key, value = check_call("parse", "max", runner.parser_messages(case["records"][i]), case["records"][i]["mapping"])
                    maximum[i] = value
                    max_keys.append(key)
            _, direct_max = check_call("direct", "max", runner.direct_messages(case), case["mapping"])
            assert row["max_relations"] == maximum
        else:
            assert row["max_relations"] is None
        assert row["max_parse_calls"] == max_keys
        assert row["direct_max"] == direct_max
        assert row["gated_adaptive_max"] == protocol.interpret(case, maximum)[0]

        direct_key, direct_normal = check_call("direct", "normal", runner.direct_messages(case), case["mapping"])
        assert row["direct_normal_call"] == direct_key
        assert row["direct_normal"] == direct_normal

    if len(report["rows"]) == len(cases):
        assert used == set(report["calls"])
    computed = runner.summarize(cases, report)
    assert report["summary"] == computed
    return {
        "validated_rows": len(report["rows"]),
        "validated_calls_referenced": len(used),
        "stored_calls": len(report["calls"]),
        "complete": len(report["rows"]) == len(cases),
        "summary": computed,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    result = audit(json.loads(args.report.read_text(encoding="utf-8")))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
