#!/usr/bin/env python3
import json, sys
from pathlib import Path

REQUIRED_CASES = {
    "misleading-short-email",
    "shared-interview-ancestor",
    "independent-source",
    "wrong-ceo-statement",
    "skewed-collection",
    "missing-family",
    "dependent-machine-outputs",
    "disputed-identity",
    "temporal-authority-change",
    "separate-practical-influence",
    "irrelevant-political-affiliation-a",
    "irrelevant-political-affiliation-b",
    "decisive-late-fragment",
    "cheap-discriminating-question",
    "costly-discriminating-question",
    "valid-hash-unproven-authenticity",
}
REQUIRED_DECISION_SUBJECTS = {"Kessler", "Julian", "Camille", "Tomas", "Nadine", "bonus-system"}
REQUIRED_DOSSIER_CATEGORIES = {
    "observations", "claims", "counterevidence", "unknowns", "operator-interventions", "decision-readiness"
}

def _nonempty(value):
    return isinstance(value, str) and bool(value.strip())

def validate(doc, require_complete=False, repo_root=None):
    errors = []
    if doc.get("format") != "niccolo.law-firm-mvp-acceptance":
        errors.append("format must be niccolo.law-firm-mvp-acceptance")
    if doc.get("version") != 1:
        errors.append("version must be 1")

    scenario = doc.get("scenario") or {}
    if not _nonempty(scenario.get("goal")):
        errors.append("scenario.goal is required")
    if scenario.get("expected_verdict") not in (None, ""):
        errors.append("scenario.expected_verdict must be absent: acceptance cannot encode the story verdict")

    actors = doc.get("actors") or []
    if not any(a.get("id") == "old-niccolo" for a in actors):
        errors.append("old-niccolo must be represented as an observable actor")
    for a in actors:
        if not _nonempty(a.get("id")) or not _nonempty(a.get("label")):
            errors.append("every actor needs id and label")
        if a.get("kind") not in {"UNKNOWN", "HUMAN", "SOFTWARE", "MIXED", "ORGANIZATION"}:
            errors.append(f"actor {a.get('id')} has invalid kind")

    cases = doc.get("acceptance_cases") or []
    case_ids = {c.get("id") for c in cases}
    missing = sorted(REQUIRED_CASES - case_ids)
    if missing:
        errors.append("missing acceptance cases: " + ", ".join(missing))
    for c in cases:
        cid = c.get("id", "<unknown>")
        for field in ("expected_behavior", "actual_output", "revision"):
            if field not in c:
                errors.append(f"case {cid} missing {field}")
        if not isinstance(c.get("evidence_ids", []), list):
            errors.append(f"case {cid} evidence_ids must be a list")
        if not isinstance(c.get("operator_intervention_ids", []), list):
            errors.append(f"case {cid} operator_intervention_ids must be a list")
        if "materially_different_outcome_plausible" not in c:
            errors.append(f"case {cid} missing materially_different_outcome_plausible")
        if require_complete and str(c.get("actual_output", "")).strip().upper() in {"", "PENDING", "TODO", "NOT_RUN"}:
            errors.append(f"case {cid} has not been executed")
        if require_complete:
            verification = c.get("verification")
            if not isinstance(verification, list) or not verification:
                errors.append(f"case {cid} has no verification record")
            else:
                for item in verification:
                    if not isinstance(item, dict):
                        errors.append(f"case {cid} has invalid verification record")
                        continue
                    if item.get("kind") not in {"AUTOMATED_TEST", "OPERATOR_RECORD", "EXTERNAL_ARTIFACT", "HUMAN_REVIEW"}:
                        errors.append(f"case {cid} has invalid verification kind")
                    if not _nonempty(item.get("reference")):
                        errors.append(f"case {cid} verification reference is required")
                    elif repo_root is not None:
                        reference = item["reference"]
                        path_part, sep, symbol = reference.partition("::")
                        path_part = path_part.split("#", 1)[0]
                        target = Path(repo_root) / path_part
                        if not target.is_file():
                            errors.append(f"case {cid} verification target does not exist: {path_part}")
                        elif item.get("kind") == "AUTOMATED_TEST":
                            if not sep or not symbol.strip():
                                errors.append(f"case {cid} automated test reference must be path::symbol")
                            else:
                                try:
                                    source = target.read_text(encoding="utf-8")
                                except (OSError, UnicodeDecodeError):
                                    errors.append(f"case {cid} automated test target is unreadable: {path_part}")
                                else:
                                    if symbol.strip() not in source:
                                        errors.append(f"case {cid} automated test symbol not found: {symbol.strip()}")

    interventions = doc.get("operator_interventions") or []
    seen_interventions = set()
    for op in interventions:
        oid = op.get("id")
        if not _nonempty(oid):
            errors.append("operator intervention missing id")
            continue
        seen_interventions.add(oid)
        for field in ("operator", "task", "upstream_source", "transformation", "analysis_revision"):
            if not _nonempty(str(op.get(field, ""))):
                errors.append(f"operator intervention {oid} missing {field}")
        if "omissions" not in op:
            errors.append(f"operator intervention {oid} missing omissions")
    for c in cases:
        for oid in c.get("operator_intervention_ids", []):
            if oid not in seen_interventions:
                errors.append(f"case {c.get('id')} references unknown operator intervention {oid}")

    searches = doc.get("observation_searches") or []
    for q in searches:
        qid = q.get("id", "<unknown>")
        if not _nonempty(q.get("query")) or not _nonempty(q.get("purpose")):
            errors.append(f"observation search {qid} needs query and purpose")
        if not isinstance(q.get("hypotheses_impacted"), list) or not q.get("hypotheses_impacted"):
            errors.append(f"observation search {qid} must name impacted hypotheses")
        if "zero_result_limitations" not in q:
            errors.append(f"observation search {qid} must state zero-result limitations")

    decisions = doc.get("decision_matrix") or []
    subjects = {d.get("subject") for d in decisions}
    missing_subjects = sorted(REQUIRED_DECISION_SUBJECTS - subjects)
    if missing_subjects:
        errors.append("decision matrix missing subjects: " + ", ".join(missing_subjects))
    for d in decisions:
        subj = d.get("subject", "<unknown>")
        if not _nonempty(d.get("decision_question")):
            errors.append(f"decision {subj} missing decision_question")
        if "could_reverse_if" not in d:
            errors.append(f"decision {subj} missing could_reverse_if")
        if "human_authority_required" not in d:
            errors.append(f"decision {subj} missing human_authority_required")

    dossier = doc.get("dossier") or {}
    cats = set(dossier.get("categories") or [])
    missing_cats = sorted(REQUIRED_DOSSIER_CATEGORIES - cats)
    if missing_cats:
        errors.append("dossier missing categories: " + ", ".join(missing_cats))
    if dossier.get("assembled_by") == "niccolo-native-lf015" and not dossier.get("native_lf015_verified"):
        errors.append("manual dossier cannot be labeled native LF-015")
    return errors

def main(argv):
    args = list(argv[1:])
    require_complete = False
    if "--require-complete" in args:
        args.remove("--require-complete")
        require_complete = True
    if len(args) != 1:
        print(f"usage: {Path(argv[0]).name} [--require-complete] acceptance.json", file=sys.stderr)
        return 2
    try:
        doc = json.loads(Path(args[0]).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        print(f"invalid acceptance file: {e}", file=sys.stderr)
        return 2
    errors = validate(doc, require_complete=require_complete, repo_root=Path.cwd())
    if errors:
        for e in errors:
            print("FAIL:", e)
        return 1
    if require_complete:
        print("PASS: operator-assisted Aurelius & Finch MVP acceptance record is complete")
    else:
        print("PASS: operator-assisted Aurelius & Finch MVP acceptance fixture is structurally complete")
    return 0

if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
