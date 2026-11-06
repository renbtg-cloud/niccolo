-- v0.87: explicit conditions that could justify revisiting a claim. They do not auto-change the claim.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_revision_conditions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  condition_id UUID NOT NULL,
  trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('NEW_EVIDENCE','CLARIFICATION','CONTEXT_CORRECTION','OTHER')),
  condition_text TEXT NOT NULL CHECK (length(condition_text) BETWEEN 1 AND 4096 AND btrim(condition_text) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,condition_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_claim_revision_conditions TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_claim_revision_conditions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_claim_revision_conditions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_revision_conditions USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_revision_conditions AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_revision_conditions AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
