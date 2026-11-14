# Niccolò documentation

Read [the root README](../README.md) first for the product, limitations and local setup.

This directory is for material useful to understand or integrate the **current intended MVP**. A design contract is **not** a guarantee that every feature described has been deployed, verified or enabled; inspect the README's implemented-versus-planned section and the tests.

- **[Product](product/)** — competitive evaluation, portability and intended product boundaries.
- **[Composite evidence and human-guided circumstances — MVP contract](architecture/MVP_COMPOSITE_EVIDENCE_AND_HUMAN_GUIDED_CONTEXT.md)** — normative, non-exhaustive contract for nested evidence, contextual interpretation and opportunistic refactoring; intended scope, not verified implementation.
- **[Architecture](architecture/)** — architectural notes and interface contracts, including the [system-integrator boundary and near-term hardening contract](architecture/SYSTEM_INTEGRATOR_BOUNDARIES_AND_HARDENING.md).
- **[Technical reference](reference/)** — detailed domain and backend references, some written at earlier version checkpoints.
- **[Development history](../dev-flow/README.md)** — milestone logs, checkpoint records and postponed work.

When source code, migrations, CI and older documents disagree, do not infer implementation status from a document's filename; verify against executable code and tests.


## First-user path

- [First-user MVP walkthrough](product/FIRST_USER_MVP_WALKTHROUGH.md) — shortest supported local path from bootstrap to first useful result, plus a human usability-test checklist.
