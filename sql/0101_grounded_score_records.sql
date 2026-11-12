-- REX-003 / EPI: immutable record of an explicitly requested deterministic
-- grounded claim-weight evaluation. The score is a heuristic balance, not truth
-- probability, proof, documentary evidence, or a mutation of the published analysis.
BEGIN;
CREATE TABLE niccolo.situation_analysis_grounded_score_records (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  score_id UUID NOT NULL,
  policy_source TEXT NOT NULL CHECK(length(policy_source) BETWEEN 1 AND 128),
  policy JSONB NOT NULL CHECK(jsonb_typeof(policy)='object'),
  human_declared_confidence_percent SMALLINT
    CHECK(human_declared_confidence_percent IS NULL OR human_declared_confidence_percent BETWEEN 0 AND 100),
  declared_evidence JSONB NOT NULL CHECK(jsonb_typeof(declared_evidence)='array'),
  result JSONB NOT NULL CHECK(jsonb_typeof(result)='object'),
  result_sha256 TEXT NOT NULL CHECK(result_sha256 ~ '^[0-9a-f]{64}$'),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,claim_id,score_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
CREATE INDEX grounded_score_records_by_analysis
  ON niccolo.situation_analysis_grounded_score_records(
    workspace_id,situation_id,analysis_id,created_at,claim_id,score_id
  );

GRANT SELECT,INSERT ON niccolo.situation_analysis_grounded_score_records TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_grounded_score_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_grounded_score_records FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_grounded_score_records
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_grounded_score_records AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_grounded_score_records AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_grounded_score_records AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_grounded_score_records AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
