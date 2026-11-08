#!/usr/bin/env python3
"""Prepare the synthetic law-firm fixture for the currently supported MVP import formats.

This is an operator transform, not a forensic authenticity operation. Original files remain
authoritative fixture artifacts. Derived WhatsApp-style transcripts carry an explicit transform
manifest so Niccolò can treat them as attributed operator-prepared material.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def wrap_as_whatsapp(original_name: str, data: bytes) -> bytes:
    text = data.decode("utf-8")
    header = (
        "01/01/2026, 00:00 - External operator: "
        f"DERIVED IMPORT TRANSCRIPT of {original_name}; original retained separately.\n"
    )
    return (header + text).encode("utf-8")


def prepare(source_root: Path, output_root: Path) -> dict:
    output_root.mkdir(parents=True, exist_ok=True)
    records = []
    candidates = []
    candidates.extend(sorted((source_root / "sources").glob("*")))
    candidates.extend(sorted((source_root / "questions").glob("*")))
    integrity = source_root / "integrity_manifest.json"
    if integrity.exists():
        candidates.append(integrity)

    for src in candidates:
        if not src.is_file():
            continue
        original = src.read_bytes()
        rel = src.relative_to(source_root)
        if src.suffix.lower() == ".eml":
            derived = original
            out_rel = Path("importable") / rel
            transformation = "NONE: EML already supported; byte-exact copy"
            source_format = "EML"
        else:
            derived = wrap_as_whatsapp(str(rel), original)
            out_rel = Path("importable") / rel.with_suffix(rel.suffix + ".whatsapp.txt")
            transformation = (
                "Wrapped exact UTF-8 content in one operator-attributed WhatsApp-export message; "
                "the derived transcript is not the original source"
            )
            source_format = "WHATSAPP_EXPORT"
        target = output_root / out_rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(derived)
        records.append(
            {
                "original_path": str(rel).replace("\\", "/"),
                "derived_path": str(out_rel).replace("\\", "/"),
                "source_format": source_format,
                "original_sha256": sha256(original),
                "derived_sha256": sha256(derived),
                "transformation": transformation,
                "original_retained": True,
                "derived_is_original": src.suffix.lower() == ".eml",
                "truth_or_authenticity_upgraded": False,
            }
        )

    manifest = {
        "format": "niccolo.operator-transform-manifest",
        "version": 1,
        "operator_role": "external fixture preparer",
        "records": records,
        "manifest_asserts_authenticity": False,
        "manifest_asserts_completeness": False,
    }
    (output_root / "transform-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--source",
        type=Path,
        default=Path("fixtures/law-firm-mvp"),
        help="synthetic fixture root",
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        shutil.rmtree(args.output)
    manifest = prepare(args.source, args.output)
    print(f"prepared {len(manifest['records'])} operator-derived import artifacts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
