#!/usr/bin/env python3
"""Read a Niccolo presentation export. Standard library only; never calls Niccolo.

This displays recorded claims, declared links, and the published answer. It does
NOT re-run an LLM, estimate truth probabilities, or infer missing source contents.
"""
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent


def read_json(path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def verify_files():
    checksums = read_json(ROOT / "SHA256SUMS.json")
    for relative, expected in checksums.items():
        if relative.startswith("/") or ".." in Path(relative).parts:
            raise ValueError("Unsafe manifest path")
        path = ROOT / relative
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError("Export file integrity mismatch: " + relative)
    return len(checksums)


def read_csv(relative):
    with (ROOT / relative).open("r", newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def explain():
    checked = verify_files()
    export = read_json(ROOT / "data" / "analysis.json")
    bundle = export["bundle"]
    print("Niccolo published-analysis presentation")
    print("Export format:", export["format"], "v", export["format_version"])
    print("Analysis:", bundle["analysis_id"])
    print("Input revision:", bundle["input_revision"])
    print("Integrity-checked files:", checked)
    print("\nOriginal question / goal:\n", bundle["goal"])
    print("\nPublished result (recorded, not recomputed):\n", bundle["answer_text"])
    print("\nCoverage / qualifications:\n", bundle["coverage_note"])

    claims = read_csv("data/claims.csv")
    fragments = read_csv("data/claim_fragments.csv")
    contexts = read_csv("data/claim_contributions.csv")
    evidence_relations = Counter((row["claim_id"], row["relation"]) for row in fragments)
    context_links = Counter(row["claim_id"] for row in contexts)
    print("\nRecorded claims and explicitly linked material:")
    for row in claims:
        claim_id = row["claim_id"]
        print("- [", row["kind"], "]", row["text"])
        print(
            "  evidence relations:", {
                relation: count
                for (id_, relation), count in evidence_relations.items()
                if id_ == claim_id
            },
            "human-context links:", context_links[claim_id],
        )
        if row["qualification"]:
            print("  qualification:", row["qualification"])
    print("\nThese are counts of recorded links, NOT evidence weights,")
    print("independent witnesses, verification, or calibrated probabilities.")

    policy_path = ROOT / "data" / "epistemic_policies.json"
    credence_path = ROOT / "data" / "human_credence.json"
    scope_path = ROOT / "data" / "epistemic_export_scope.json"
    if policy_path.exists() and credence_path.exists() and scope_path.exists():
        policies = read_json(policy_path)
        credence = read_json(credence_path)
        scope = read_json(scope_path)
        print("\nRecorded epistemic controls:")
        print("- policy records relevant to this Situation:", len(policies))
        print("- human credence records for this analysis:", len(credence))
        print("- deterministic grounded scores exported:", scope["deterministic_scores_exported"])
        print("- policy history proves policy used for output:", scope["policy_history_proves_policy_used_for_output"])
        for policy in policies:
            print(
                "  policy", policy["policy_id"],
                "scope", policy["scope"],
                "support x", policy["support_multiplier_pct"],
                "counter x", policy["counter_multiplier_pct"],
                "per-origin cap", policy["per_origin_cap"],
            )
        for entry in credence:
            print(
                "  human credence", entry["credence_id"],
                "claim", entry["claim_id"],
                str(entry["confidence_percent"]) + "%",
                entry["usage_policy"],
            )
        print("Human percentages remain human declarations, not documentary evidence or calibrated truth probabilities.")
        print("Recorded policy history does not prove a policy generated the published answer.")
        grounded_path = ROOT / "data" / "grounded_scores.json"
        if grounded_path.exists():
            grounded = read_json(grounded_path)
            print("- immutable grounded score records:", len(grounded))
            for record in grounded:
                report = record.get("result", {}).get("report", {})
                print(
                    "  grounded score", record["score_id"],
                    "claim", record["claim_id"],
                    "support", report.get("support_points"),
                    "counter", report.get("counter_points"),
                    "net", report.get("net_points"),
                    "result SHA-256", record["result_sha256"],
                )
            if grounded:
                print("Stored grounded scores are deterministic heuristic artifacts, not truth probabilities or proof.")
    source_extract_path = ROOT / "data" / "examined_source_fragments.json"
    if source_extract_path.exists():
        source_extracts = read_json(source_extract_path)
        print("\nExplicitly authorized examined source excerpts:", len(source_extracts))
        print("These excerpts were recorded as examined by this analysis.")
        print("They are NOT original uploaded files, do NOT establish truth,")
        print("and may contain sensitive/private communications.")
        print("Excerpt bodies are not printed to the terminal; inspect the JSON/CSV explicitly.")
        for row in source_extracts:
            print(
                "- evidence", row["evidence_id"],
                "locator", row["locator"],
                "bytes", len(row["content"].encode("utf-8")),
                "fragment SHA-256", row["fragment_sha256"],
                "document SHA-256", row["document_sha256"],
            )
    else:
        print("Source fragment contents were NOT exported.")
    print("Full model reasoning was NOT exported.")
    print("The PostgreSQL state of Niccolo remains authoritative.")


if __name__ == "__main__":
    try:
        explain()
    except (OSError, ValueError, KeyError, TypeError, csv.Error) as exc:
        print("Could not inspect export:", exc, file=sys.stderr)
        sys.exit(1)
