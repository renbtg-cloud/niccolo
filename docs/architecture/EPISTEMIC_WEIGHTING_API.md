# Reasoning weights: API-first milestone

These endpoints are experimental. They are **not** a declaration that Niccolò computes calibrated probability of truth.

## Human-declared confidence on an existing claim

A user can say, for example: “I believe with 90% subjective confidence that André will leave for a competitor.” The percentage is not extracted from the PDF, and it does not overwrite what the PDF actually says.

Requires an existing \`workspace\`, \`situation\`, \`analysis\` and \`claim\`. The authenticated principal becomes the author.

\`\`\`http
POST /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/credence/{credence}
Authorization: Bearer <token>
Content-Type: application/json

{
  "confidence_percent": 90,
  "usage_policy": "SUBJECTIVE_BELIEF",
  "rationale": "I know contextual details not available in the attached document."
}
\`\`\`

The \`usage_policy\` may be \`SUBJECTIVE_BELIEF\` or \`OPERATIONAL_PREMISE\`. The latter means a human chose to proceed conditionally; it does **not** convert a hypothesis into a verified fact.

A repeat with exactly the same ID, actor and content is idempotent; a changed repeat returns a conflict. The record is append-only. To change one's position, submit **another** assessment with another ID; never silently edit historical judgments.

Read the record list:

\`\`\`http
GET /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/credence
Authorization: Bearer <token>
\`\`\`

These assessments are surfaced in \`reasoning_packet.current_human_control.claim_credence\`, never in \`frozen_input\`. This first implementation is claim- and analysis-scoped: cross-analysis propagation and pre-analysis declarations still require a separate design and validation.

## Epistemic weights — harmless preview

\`\`\`http
POST /v1/workspaces/{workspace}/epistemic/preview
Authorization: Bearer <token>
Content-Type: application/json

{
  "policy": {
    "support_multiplier_pct": 100,
    "counter_multiplier_pct": 150,
    "per_origin_cap": 100
  },
  "human_declared_confidence_percent": 90,
  "observations": [
    {
      "evidence_id": "00000000-0000-0000-0000-000000000001",
      "documented_origin_id": "00000000-0000-0000-0000-000000000011",
      "stance": "SUPPORTS",
      "declared_strength": 80,
      "rationale": "Illustrative statement attributed to a document"
    },
    {
      "evidence_id": "00000000-0000-0000-0000-000000000002",
      "documented_origin_id": "00000000-0000-0000-0000-000000000011",
      "stance": "SUPPORTS",
      "declared_strength": 80,
      "rationale": "Copy of the first statement"
    }
  ]
}
\`\`\`

Both observations share one documented origin: their supportive strength is counted once (80), not twice (160). The separate 90% human assessment does **not** increase the 80-point support score. The response explicitly says \`inputs_checked_against_database: false\`, \`changes_any_persisted_state: false\` and \`score_is_probability_of_truth: false\`.

**Limitations:**

- Preview inputs are **unverified**; arbitrary evidence IDs can be provided without claiming they exist in the database.
- Two distinct source roots are not sufficient to establish witness independence.
- The arithmetic is a configurable **heuristic evidence balance**, not Bayesian or calibrated probability.
- No score is evidence of motive, proof of guilt, or authorization for a consequential decision.
- The endpoint is read-only with respect to application state, but authentication and normal request processing remain in force.
- A **heuristic** graphical Playground is available under **Technical → Playground**. Full LLM-replay simulation is **not** available.

## Workspace and Situation heuristic policies

Policies are append-only, authored by the authenticated user and explicitly **heuristic**.
If there is no user policy, Niccolò returns a neutral built-in default.
A Situation override wins over a workspace default. New policies replace the *effective future selection*, not the immutable historical record.

\`\`\`http
POST /v1/workspaces/{workspace}/epistemic/policies/{new_policy_id}
Authorization: Bearer <token>
Content-Type: application/json

{
  "situation_id": null,
  "policy": {
    "support_multiplier_pct": 100,
    "counter_multiplier_pct": 150,
    "per_origin_cap": 100
  },
  "rationale": "Workspace default; examine counterevidence more closely"
}
\`\`\`

For investigation-only control, use an existing Situation UUID in \`situation_id\`.
The same policy ID cannot be repurposed with different values or a different actor.

\`\`\`http
GET /v1/workspaces/{workspace}/epistemic/policy?situation_id={situation}
Authorization: Bearer <token>
\`\`\`

Omit \`situation_id\` to inspect the workspace default. The response identifies the effective scope: \`SITUATION\`, \`WORKSPACE\` or \`BUILTIN_DEFAULT\`.

The preview endpoint allows the policy object to be omitted; when omitted it uses the effective stored policy. With a \`situation_id\` it chooses the Situation override if present. If a policy object is supplied, it trials those weights, without changing the stored policy; \`policy_source\` describes which rule was used.

This is **not** an automatic statistical fitting process. No weighting result is permitted to convert subjective belief into documentary truth. Policy records must not rewrite frozen historical analyses.

## Source-grounded claim scoring (EPI-013 and EPI-014)

This route is different from the unverified synthetic preview:

\`\`\`http
POST /v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/epistemic/evaluate
Authorization: Bearer <token>
Content-Type: application/json

{
  "human_declared_confidence_percent": 90,
  "evidence": [
    {
      "evidence_id": "00000000-0000-4000-8000-000000000001",
      "declared_strength": 80,
      "rationale": "The user considers this a strong indication"
    }
  ]
}
\`\`\`

The UUID above is just a shape example; real use requires an actual *examined and claim-linked* evidence UUID. An unknown, unrelated, or invented evidence link is rejected, not silently accepted.

The backend obtains stance from stored claim/evidence relations. It retrieves source-document membership, follows **recorded** multi-parent derivation ancestry in PostgreSQL under the same repeatable-read transaction, and conservatively groups selected fragments sharing a documented ancestor or source document.

The response exposes:
- per-evidence recorded relation, declared strength and structural dependence group;
- known lineage component membership and recorded roots, never a guarantee of witness independence;
- \`source_graph_revision\` from the database snapshot;
- support, contradiction, net *heuristic* points, and explicit non-probability/non-fact flags.

A source group key is a **computational component representative**. It must never be displayed as if it were a verified original source. Unknown unrecorded dependencies are still possible. Recorded withdrawn ancestors and cycles fail closed.

## User-configurable policy in the Playground

The Technical Playground uses fictional IDs against the read-only \`/epistemic/compare\` route. It does **not** read or write documentary evidence or call an LLM.

After comparing, a human can optionally invoke a separate, authenticated **write** operation to create an append-only policy record for the active Situation or the workspace default. The human supplies a rationale. This changes future effective weighting, not frozen historical analyses.

This is not an automatic statistical fitting process and not a full model-reasoning replay. A future isolated replay feature needs frozen input snapshots, reproducible provider settings and transparent stochastic variability.

## Validation gate before merge

Run \`rustfmt --edition 2024 --check\` on the changed Rust sources, \`cargo test --locked --lib\`, strict Clippy, all migrations on a fresh PostgreSQL 17, and authenticated HTTP integration tests. The implemented EPI-001–011, EPI-013/014 and initial EPI-017 slices passed Linux CI and were merged into main. Future changes still require their own verification.
