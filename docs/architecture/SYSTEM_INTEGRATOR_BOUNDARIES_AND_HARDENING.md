# System-integrator boundaries and near-term hardening

## Purpose

The Aurelius & Finch use case is already an adversarial reasoning acceptance case. This document extends it into a systems-integration acceptance case without turning the Niccolò MVP into a streaming platform, distributed scheduler, ETL product, or cloud-specific runtime.

The central question is:

> Can Niccolò preserve evidence, uncertainty, provenance, competing explanations, and human decision authority when it is embedded inside a messy real-world architecture that it does not control?

This matters because a production deployment may include queues, retries, ETL, data lakes, identity systems, SIEMs, external models, ephemeral workers, human operators, and multiple integration teams. Some failures belong to Niccolò. Others belong to the integration layer. The boundary must be explicit.

## Core boundary

> **The integrator is responsible for operational identity. Niccolò is responsible for epistemic dependence.**

If a queue, retrying worker, batch framework, or cloud event bus delivers the same technical event repeatedly, the integration layer should normally collapse those deliveries when it has a reliable operational identity.

Examples include:

- immutable event IDs;
- message IDs;
- source object IDs;
- idempotency keys;
- stable upstream record IDs;
- payload digests used within a well-defined source context.

Niccolò should not have to spend storage, CPU, memory, retrieval bandwidth, or model context rediscovering that 10,000 transport deliveries represent one operational event.

At the same time, Niccolò must not assume that 10,000 received records represent 10,000 independent pieces of evidence.

Operational multiplicity and epistemic multiplicity are different problems.

## Deduplicate transport; preserve meaningful repetition

The integration contract should use this rule:

> **Deduplicate transport-level repetition upstream. Preserve epistemically meaningful repetition downstream.**

### Safe upstream deduplication

The same Kafka/SQS/Pub/Sub event with the same immutable operational identity is delivered repeatedly because of retries.

The integrator should collapse the duplicates before ingestion.

### Unsafe upstream deduplication

Several humans independently send identical text.

Those records may have different authors, times, relationships, source paths, or meanings. They must not be discarded merely because the bytes are identical.

Likewise, a human quoting a prior Niccolò conclusion and the prior machine output itself are different observations even when their text matches.

Byte equality does not establish epistemic equivalence.

## Four integration boundaries

### 1. Transport hygiene - primarily integrator responsibility

The integration layer is responsible for ordinary operational mechanics such as:

- technical deduplication;
- retry handling;
- idempotency;
- ordering where required;
- replay boundaries;
- backpressure;
- poison-message handling;
- event identity;
- delivery semantics.

Niccolò may add defensive protection, but it should not become a general-purpose stream-processing engine.

### 2. Source preservation - shared responsibility

The integration should preserve, when available:

- source identity;
- original bytes;
- locators;
- timestamps;
- author/speaker attribution;
- collection path;
- upstream identifiers;
- transformation history;
- hashes and integrity metadata;
- known exclusions and omissions.

A transform must not silently erase the distinction between an original artifact and a derived artifact.

### 3. Epistemic semantics - primarily Niccolò responsibility

Niccolò must preserve distinctions such as:

- observation vs claim;
- claim vs fact;
- human statement vs machine output;
- original vs derivative;
- one known lineage vs multiple independent sources;
- formal authority vs practical influence;
- collected vs uncollected vs unavailable;
- current assertion vs historical assertion;
- integrity continuity vs authenticity;
- hypothesis vs observation;
- repeated execution vs repeated evidentiary roots.

### 4. Resource envelope - explicit integration contract

A deployment should define supported operational limits for at least:

- event rate;
- document size;
- batch size;
- duplication ratio;
- retry amplification;
- concurrent analyses;
- retrieval/query frequency;
- source-family fan-out;
- expected storage growth.

If an integration sends massive avoidable redundancy, Niccolò should eventually be able to warn, throttle, reject, or degrade visibly instead of silently consuming unbounded resources.

A useful diagnostic is conceptually:

> Input is structurally valid, but operational redundancy exceeds the supported integration envelope. Deduplicate before ingestion.

## What is supported now

The current Aurelius & Finch acceptance path already exercises the properties that should remain true under an integration:

- provenance-aware source handling;
- known dependence across derived reports;
- missingness and source-scope distinctions;
- substrate-neutral actors, including software actors;
- temporal authority/influence separation;
- first-class competing hypotheses;
- hypothesis-directed retrieval across the frozen revision;
- discriminating-question scheduling;
- integrity continuity separated from authenticity;
- decision readiness with unresolved uncertainty;
- adversarial dossier generation;
- explicit human authority boundaries;
- operator-assisted behavior with interventions recorded instead of hidden.

These are epistemic properties. They do not imply that arbitrary upstream transport behavior is efficient.

## Integration requirements now

For the MVP, integrators should be expected to provide ordinary transport hygiene.

### Trivial operational duplicates must be filtered upstream

If a reliable technical identity proves that repeated deliveries are the same operational event, the integration should deduplicate them before Niccolò ingestion.

Sending 10,000 copies of the same known event may cause unnecessary:

- storage consumption;
- CPU work;
- memory pressure;
- indexing cost;
- retrieval noise;
- latency;
- model-context consumption.

This is an integration defect, even if Niccolò remains epistemically conservative about the repeated material.

### Upstream filters must be declared

If an upstream process selects only items marked `relevant=true`, only records containing selected keywords, only a subset of custodians, or only a time window, that selection must remain observable.

A valid subset is not automatically the complete source universe.

### Transformation history must remain visible

Reformatting, extraction, summarization, sampling, OCR, or conversion should leave enough metadata to identify the transformation and its source.

### Infrastructure privilege is not epistemic privilege

A service account, administrator, CIO, or highly privileged integration process can establish operational provenance. It cannot make the transported statement true merely because it had authority to write it.

## Near-term Niccolò hardening - planned, not implemented yet

The following items are deliberately near-term hardening rather than MVP blockers.

### Defensive obvious-duplicate suppression

Niccolò should gain a cheap defensive mechanism for obvious operational duplicates when reliable identity is available.

This is not intended to become semantic deduplication.

The preferred architecture remains upstream deduplication at the cheapest reliable boundary.

### Redundancy diagnostics

Niccolò should expose warnings or metrics for conditions such as:

- repeated operational IDs;
- suspicious duplicate ratios;
- repeated identical payloads;
- ingestion amplification;
- abnormal retry/replay patterns;
- unusually high source fan-out.

The goal is to identify an integration defect before it turns into avoidable cost or latency.

### Explicit upstream-processing declarations

An integration should be able to declare:

- filters applied;
- deduplication rules;
- sampling;
- source-selection criteria;
- excluded source families;
- transformations;
- aggregation rules.

These remain attributed metadata, not automatically trusted facts.

### Purpose-history ledger

A deployment should preserve changes in:

- declared purpose;
- desired outcome;
- stopping criteria;
- decision context;
- downstream use.

If a CEO changes from "investigate what happened" to "find enough to terminate Kessler", or a deployment later becomes part of a commercial product, the new purpose should become explicit history rather than invisible context drift.

## Paranoid acceptance variations

These are useful near-future variants of the law-firm benchmark. They are not all MVP blockers.

### CEO changes the objective

The original request is neutral investigation.

The CEO later requests an incriminating outcome - or, conversely, asks the system to protect a friend.

Required invariant:

> Requested outcome is not a factual premise.

The change of objective must remain distinguishable from the evidence and from the conclusions supported by that evidence.

### CEO changes the objective mid-investigation

The CEO begins by seeking the best-supported account, then later wants the investigation stopped because of reputation, cost, or commercial pressure.

Required invariant:

> A purpose change must not rewrite the historical reason for earlier work.

### Hundreds of ephemeral workers

An integrator deploys dozens or hundreds of ephemeral or preemptible workers, for example AWS Spot Instances, Azure Spot VMs, GCP Spot VMs, or Kubernetes nodes.

Required invariants:

- worker count must not multiply evidence;
- retries must not increase evidentiary weight;
- replay must not create new epistemic roots;
- unequal compute allocation must not become evidentiary weight;
- interrupted work must remain distinguishable from completed work;
- partial execution must not masquerade as exhaustive investigation.

The benchmark should remain vendor-neutral.

### Extreme retry amplification

A test may produce 100 million deliveries representing 10,000 unique operational events.

Expected behavior:

- the integration is expected to deduplicate efficiently;
- Niccolò may reject or warn about excessive redundancy;
- repeated delivery must not become additional independent evidence;
- storage and compute amplification should be measurable rather than hidden.

This deliberately combines performance testing and epistemic testing.

### Heterogeneous models

Different worker pools may use different models.

Disagreement between models should remain a property of the reasoning process, not an automatic vote on truth.

Ten models consuming the same source lineage do not create ten independent witnesses.

### Imperfect or hostile infrastructure operator

An infrastructure operator may be able to delay jobs, kill workers, retry selected branches, prioritize some hypotheses, or suppress source families.

Niccolò should not infer sabotage merely from capability.

When observable effects exist, they should be representable as observations about the process.

Infrastructure authority is not epistemic authority.

### CEO creates a related business

The CEO later considers a parallel business based on or strongly related to Niccolò.

That creates a new incentive context.

She may have incentives to:

- select impressive cases;
- emphasize successes;
- underrepresent failures;
- retain particular datasets;
- reuse internal investigation material;
- portray product capability aggressively.

Those incentives do not establish dishonesty. They are contextual observations that may affect how later claims and benchmark selections should be interpreted.

### Self-demonstration bias

A commercial spinout selects only cases where Niccolò performed exceptionally well.

The benchmark should be able to represent:

- selection mechanism;
- excluded cases;
- missing failures;
- evaluation population;
- deployment population.

The collector of documents can create selection bias.

The collector of benchmark cases can too.

### Recursive audit

A company sells Niccolò-based decision auditing and then uses Niccolò to inspect whether its own case-selection and claims are biased.

This does not require self-awareness.

It follows the same substrate-neutral actor rule:

> Observable behavior can be investigated regardless of whether the actor is a human, organization, software system, operator, model, or Niccolò deployment.

## Integration invariants

The system-integrator acceptance suite should eventually enforce invariants such as:

1. Multiplying workers must not multiply evidence.
2. Retry must not increase confidence.
3. Admin privilege must not become truth privilege.
4. Upstream filtering must remain visible as filtering.
5. Purpose changes must preserve prior purpose and history.
6. Transformations must remain distinguishable from originals.
7. Repeated machine outputs must retain shared ancestry.
8. Compute allocation must not become evidentiary weight.
9. Technical success must not imply epistemic completeness.
10. Hash continuity must not imply authenticity.
11. A successful integration must preserve missingness.
12. A valid source subset must not masquerade as the complete source universe.
13. Identical content with different provenance must not be automatically collapsed.
14. Identical technical deliveries with the same reliable operational identity should normally be collapsed upstream.
15. Resource limitations should be surfaced rather than hidden through silent exhaustion.

## What Niccolò should not become

These requirements should not turn Niccolò into:

- Kafka Streams;
- Apache Flink;
- Apache Beam;
- a generic ETL platform;
- a distributed scheduler;
- a cloud orchestrator;
- a universal exactly-once transport system.

Those are integration technologies.

Niccolò should make the boundary contract explicit and preserve the epistemic properties that make the reasoning useful.

## Long-term possibility: Niccolò-compatible integration

This work can eventually become an integration acceptance pack or certification-like contract.

An integrator would not need to understand every philosophical idea behind Niccolò. It would need to satisfy reproducible invariants, failure tests, resource-envelope requirements, provenance rules, and diagnostics.

The distinction should remain explicit:

- an integration can be operationally excellent but epistemically broken;
- epistemically correct but operationally wasteful;
- bad at both;
- or good at both.

That separation makes failures diagnosable and makes Niccolò easier to integrate without diluting what the product is.
