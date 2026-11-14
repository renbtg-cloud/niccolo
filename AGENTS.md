# Instructions for coding assistants

Read [README.md](README.md) for the intended product, verified limitations and build/test guidance. Before modifying evidence intake, source parsing, attachments, document handling, provenance, human steering, model reasoning, or the supporting UI, read the [MVP composite evidence and human-guided circumstances contract](docs/architecture/MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md). Follow related links to focused domain contracts and check executable code/tests for actual implementation status.

This is **MVP scope**, not a post-MVP backlog item; however, **implement or refactor when that is the most technically opportune moment**, including early structural work when beneficial. Do not require the MVP to be almost finished before considering the contract. If deferring meaningful work, document why, what dependencies remain, and what precautions avoid costly rework.

The contract is **intentionally non-exhaustive**. Improve, correct and extend technical details as needed; examples are not an exhaustive checklist or a license to overengineer. Preserve human-guided circumstance-domain authority, attribution/provenance, user control of consequential decisions, and explicit exclusions of autonomous image/video understanding in the MVP. Neither Niccolò nor supporting AIs have sovereign epistemic authority.

Avoid treating normative specifications as already implemented. Before claiming behavior is ready, verify tests and the end-to-end user workflow; update documentation of current coverage only after it is demonstrated.
