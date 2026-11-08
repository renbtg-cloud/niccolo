# Niccolò — Domain-Specific Evolution Harness (v0.3 POC)

## Purpose and scope

This is a **real Rust compilation and execution experiment** built on the v0.2 epistemic provenance/competitive-evaluation POC. It does **not** claim Niccolò is an autonomous software engineer, and has not used a frontier LLM.

The harness generates **three executable Rust candidate programs**, from a **trusted, fixed template**, for a small provenance-lineage problem. The code is compiled independently, run as independent OS processes, checked against training and held-out synthetic fixtures, and measured for end-to-end process runtime. The current goal is an executable core of a later self-evolution engine, not a deployed algorithm.

## Candidate strategies

| Candidate | Technique | Known limitation |
| --- | --- | --- |
| `naive_report_count` | Number of repeated reports | Confuses reports and sources; cannot detect invalid lineage |
| `one_step_parent` | At most one source-transmission edge | Fails multi-hop, cycles, missing ancestors |
| `transitive_trace_root` | Trace each report to documented origin, detect cycles/missing links | Still **does not establish independence of minds or truth of statements** |

The third candidate embodies a property already present in the v0.2 Rust `EvidenceIndex`; **it is not a newly discovered algorithm or evidence of improvement over v0.2**.

## Reproduce with Rust 1.99.1 or newer

```bash
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo run --offline --bin niccolo-evolution-demo -- --processes 8 --reps 31
```

This requires an accessible Rust `rustc` compiler to produce candidate executables. It has **zero third-party Cargo dependencies**, making it suitable for offline Linux use once the Rust toolchain is installed.

Outputs:

- `target/evolution-candidates/`: generated candidate `.rs` sources and compiled **independent Linux binaries**.
- `reports/evolution-v003.tsv`: measured quality, process concurrency outcome, process-level median and p95 wall time.
- `fixtures/train.txt`: 8 known examples.
- `fixtures/holdout.txt`: 16 separately labeled adversarial examples, never embedded in candidate sources.

**Metrics are per machine / execution**, not portable promises. The measured latency includes launching a short-lived process, writing input, reading output and reaping it; it does not represent internal algorithm latency. Rejected quality candidates skip latency and concurrency testing rather than being misreported as concurrent failures.

## Promotion rules

Candidates must pass **8/8 training** and **16/16 holdout** cases and **all configured concurrent process checks** before they are eligible for *review*. An eligible candidate is **not deployed**. A real evolving Niccolò would additionally need independent/scientific evaluation of human reasoning, a regression suite, stronger sandboxing, stable evaluation budgets, staged rollouts and rollback.

## Trust and limitations

- All fixture facts and expected outputs are **synthetic**, with deliberately unambiguous source-lineage topology. They are not evidence about real people's beliefs or intentions.
- There are **no LLM calls**, no self-generated algorithms beyond enumerated trusted variations, no frontier baseline, no API keys, no user files/data and no authorized autonomous production changes.
- **Never allow arbitrary uploaded Rust or LLM-generated Rust through this bare harness.** The subprocess timeout is not a security sandbox. Real extension requires filesystem/network isolation, CPU/RAM quotas, identity separation, integrity controls and explicit authorization before code can execute.
- A future independent frontier-agent evaluation must use raw original evidence only, without Niccolò's derivations, as documented in `COMPETITIVE_EVALUATION.md`.
- The PostgreSQL v0.2 migration files are retained unchanged. This POC does not need a database running and does not update production.

## Next technical increments

1. A genuine domain-specific specification language or typed IR for constrained user/candidate reasoning transformations; require code-generation tests and measured equivalence.
2. Persist reproducible candidate artifacts, hashes, configurations, outcome logs and approval decisions in PostgreSQL.
3. Replace trusted-template-only execution with an actual sandbox for generated Rust/WASM candidates and enforce per-candidate CPU/memory/filesystem/network budgets.
4. Introduce a permitted model-assisted proposer with read-only ability to suggest candidate implementations. No production binary is replaced without passing objective gates.
5. Compare domain-quality and performance under fixed, auditable budgets against v0.2, candidate incumbent, independently operating frontier agents and held-out cases.
6. Distribute isolated candidate runners across multiple machines once a secure job protocol and reliable measurement environment exist.
