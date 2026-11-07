-- Epistemic weighting milestone: append-only HUMAN credence about a concrete claim.
-- A number here is someone's subjective estimate, not truth probability certified by Niccolò.
BEGIN;
CREATE TABLE niccolo.situation_analysis_human_credence (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  credence_id UUID NOT NULL,
  confidence_percent SMALLINT NOT NULL CHECK(confidence_percent BETWEEN 0 AND 100),
  usage_policy TEXT NOT NULL CHECK(usage_policy IN ('SUBJECTIVE_BELIEF','OPERATIONAL_PREMISE')),
  rationale TEXT NOT NULL CHECK(length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,credence_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
CREATE INDEX human_credence_by_claim
 ON niccolo.situation_analysis_human_credence(workspace_id,situation_id,analysis_id,claim_id,created_at,credence_id);
GRANT SELECT,INSERT ON niccolo.situation_analysis_human_credence TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_human_credence ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_human_credence FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_human_credence
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_human_credence AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_human_credence AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_human_credence AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_human_credence AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
