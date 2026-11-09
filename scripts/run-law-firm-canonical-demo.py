#!/usr/bin/env python3
"""Package and verify the canonical Aurelius & Finch MVP demonstration."""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "fixtures" / "law-firm-mvp"
ACCEPTANCE = FIXTURE / "acceptance.completed.json"
OPERATOR_REVIEW = FIXTURE / "operator-review.md"

def sha256_path(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(cmd: list[str]) -> dict:
    completed = subprocess.run(
        cmd, cwd=ROOT, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, check=False
    )
    if completed.returncode != 0:
        sys.stdout.write(completed.stdout)
        raise SystemExit(completed.returncode)
    return {
        "command": cmd,
        "status": "PASS",
        "output_tail": completed.stdout[-4000:],
    }

def prepare_fixture(output: Path) -> dict:
    prepared = output / "prepared-fixture"
    result = run([
        sys.executable,
        "scripts/prepare-law-firm-fixture.py",
        "--source", str(FIXTURE.relative_to(ROOT)),
        "--output", str(prepared),
    ])
    manifest_path = prepared / "transform-manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    return {
        "command_result": result,
        "manifest": manifest,
        "prepared_root": prepared,
    }

def source_inventory() -> list[dict]:
    paths = []
    paths.extend(sorted((FIXTURE / "sources").glob("*")))
    paths.extend(sorted((FIXTURE / "questions").glob("*")))
    paths.extend(p for p in [
        FIXTURE / "integrity_manifest.json",
        ACCEPTANCE,
        OPERATOR_REVIEW,
    ] if p.exists())
    return [{
        "path": str(path.relative_to(ROOT)).replace("\\", "/"),
        "sha256": sha256_path(path),
        "bytes": path.stat().st_size,
    } for path in paths if path.is_file()]

def verification_summary(acceptance: dict) -> dict:
    by_kind = {}
    references = 0
    for case in acceptance["acceptance_cases"]:
        for item in case.get("verification", []):
            by_kind[item["kind"]] = by_kind.get(item["kind"], 0) + 1
            references += 1
    return {
        "case_count": len(acceptance["acceptance_cases"]),
        "completed_case_count": sum(
            1 for case in acceptance["acceptance_cases"]
            if str(case.get("actual_output", "")).strip()
        ),
        "verification_reference_count": references,
        "verification_by_kind": dict(sorted(by_kind.items())),
    }

def build_handoff(acceptance: dict) -> dict:
    return {
        "format": "niccolo.canonical-demo-handoff",
        "version": 1,
        "scenario": acceptance["scenario"],
        "acceptance_cases": [{
            "id": case["id"],
            "actual_output": case["actual_output"],
            "verification": case["verification"],
            "operator_intervention_ids": case.get("operator_intervention_ids", []),
            "materially_different_outcome_plausible":
                case["materially_different_outcome_plausible"],
        } for case in acceptance["acceptance_cases"]],
        "decision_matrix": acceptance["decision_matrix"],
        "operator_interventions": acceptance["operator_interventions"],
        "native_adversarial_dossier_contract": acceptance["dossier"],
        "handoff_asserts_verdict": False,
        "handoff_asserts_source_authenticity": False,
        "handoff_asserts_collection_completeness": False,
        "handoff_grants_authority": False,
        "handoff_executes_actions": False,
        "note": (
            "Canonical human-review handoff assembled from the accepted case record. "
            "It is not a substitute for the native adversarial-dossier runtime output; "
            "that runtime path is verified separately by focused Rust integration tests."
        ),
    }

def write_readme(output: Path, execution: dict) -> None:
    summary = execution["acceptance_summary"]
    text = (
        "# Aurelius & Finch canonical MVP demonstration\n\n"
        "Status: PASS when this directory was generated.\n\n"
        "This is a reproducible operator-assisted acceptance artifact, not a "
        "pre-written story verdict. It preserves the synthetic fixture, operator "
        "transformations, acceptance evidence, decision handoff, and hashes.\n\n"
        f"- Acceptance cases: {summary['completed_case_count']}/{summary['case_count']}\n"
        f"- Verification references: {summary['verification_reference_count']}\n"
        "- Expected verdict encoded in fixture: none\n"
        "- Handoff grants authority: no\n"
        "- Handoff executes actions: no\n"
        "- Native adversarial dossier: runtime capability verified separately; "
        "not faked by this packager\n\n"
        "Files:\n"
        "- canonical-execution.json: execution metadata and hashes\n"
        "- canonical-handoff.json: human-review handoff\n"
        "- acceptance.completed.json: evidence-backed 16-case acceptance record\n"
        "- operator-review.md: bounded operator judgments\n"
        "- prepared-fixture/: importable fixture and transform manifest\n\n"
        "Canonical command:\n"
        "  python3 scripts/run-law-firm-canonical-demo.py --output <directory> --verify-rust\n\n"
        "The verify-rust mode requires an initialized PostgreSQL environment with "
        "the normal Niccolo integration environment variables.\n"
    )
    (output / "README.md").write_text(text, encoding="utf-8")

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--verify-rust", action="store_true",
        help="run focused PostgreSQL integration tests before packaging"
    )
    args = parser.parse_args()

    output = args.output if args.output.is_absolute() else ROOT / args.output
    if output.exists():
        shutil.rmtree(output)
    output.mkdir(parents=True)

    checks = []
    prepared = prepare_fixture(output)
    checks.append(prepared["command_result"])
    checks.append(run([
        sys.executable,
        "scripts/check-law-firm-mvp-acceptance.py",
        "--require-complete",
        str(ACCEPTANCE.relative_to(ROOT)),
    ]))

    rust_tests = [
        ["cargo","test","--locked","--features","pg-integration","--test",
         "law_firm_reasoning_capabilities_integration","--","--nocapture"],
        ["cargo","test","--locked","--features","pg-integration","--test",
         "integration_boundary_paranoid","--","--nocapture"],
        ["cargo","test","--locked","--features","pg-integration","--test",
         "grounded_claim_weights_integration",
         "cross_document_common_ancestor_prevents_double_counting","--","--nocapture"],
        ["cargo","test","--locked","--features","pg-integration","--test",
         "identity_integration",
         "explicit_assignment_then_correction_then_retraction_preserves_audit","--","--nocapture"],
        ["cargo","test","--locked","--features","pg-integration","--test",
         "model_guided_observation_integration",
         "a_model_intent_retrieves_late_frozen_evidence_into_the_next_packet","--","--nocapture"],
    ]
    if args.verify_rust:
        checks.extend(run(command) for command in rust_tests)

    acceptance = json.loads(ACCEPTANCE.read_text(encoding="utf-8"))
    handoff = build_handoff(acceptance)
    handoff_path = output / "canonical-handoff.json"
    handoff_path.write_text(
        json.dumps(handoff, indent=2, sort_keys=True) + "\n",
        encoding="utf-8"
    )
    shutil.copy2(ACCEPTANCE, output / ACCEPTANCE.name)
    shutil.copy2(OPERATOR_REVIEW, output / OPERATOR_REVIEW.name)

    commit = subprocess.run(
        ["git","rev-parse","HEAD"], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=False
    ).stdout.strip() or None

    execution = {
        "format": "niccolo.law-firm-canonical-execution",
        "version": 1,
        "scenario": acceptance["scenario"],
        "repository_commit": commit,
        "fixture_root": "fixtures/law-firm-mvp",
        "acceptance_summary": verification_summary(acceptance),
        "source_inventory": source_inventory(),
        "transform_manifest_sha256":
            sha256_path(prepared["prepared_root"] / "transform-manifest.json"),
        "acceptance_sha256": sha256_path(ACCEPTANCE),
        "operator_review_sha256": sha256_path(OPERATOR_REVIEW),
        "canonical_handoff_sha256": sha256_path(handoff_path),
        "checks": checks,
        "rust_runtime_verification_requested": args.verify_rust,
        "native_dossier_endpoint": acceptance["dossier"]["endpoint"],
        "native_dossier_runtime_verified_by": (
            "tests/law_firm_reasoning_capabilities_integration.rs::"
            "adversarial_dossier_packages_review_state_without_claiming_verdict"
        ),
        "execution_asserts_verdict": False,
        "execution_asserts_authenticity": False,
        "execution_asserts_completeness": False,
        "execution_grants_authority": False,
    }
    (output / "canonical-execution.json").write_text(
        json.dumps(execution, indent=2, sort_keys=True) + "\n",
        encoding="utf-8"
    )
    write_readme(output, execution)
    print(
        "canonical Aurelius & Finch demo PASS: "
        f"{execution['acceptance_summary']['completed_case_count']}/"
        f"{execution['acceptance_summary']['case_count']} acceptance cases"
    )
    print(output)
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
