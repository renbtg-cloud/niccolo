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
- No complete graphical Playground exists yet.

## Validation gate before merge

Run \`rustfmt --edition 2024 --check\` on the changed Rust sources, \`cargo test --locked --lib\`, strict Clippy, all migrations on a fresh PostgreSQL 17, and authenticated HTTP integration tests. The draft PR deliberately stays separate from main until compatibility is checked.
