-- v1.401-v1.440: durable pre-synthesis competing interpretations.
-- These are model-generated candidates for consideration, not claims, facts, rankings, or decisions.
BEGIN;
CREATE TABLE niccolo.situation_analysis_candidate_interpretations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  interpretation_text TEXT NOT NULL CHECK(length(interpretation_text) BETWEEN 1 AND 4096 AND btrim(interpretation_text) <> ''),
  rationale TEXT NOT NULL CHECK(length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,interpretation_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE INDEX candidate_interpretations_by_analysis
  ON niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,created_at,interpretation_id);
GRANT SELECT,INSERT ON niccolo.situation_analysis_candidate_interpretations TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_candidate_interpretations ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_candidate_interpretations FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_candidate_interpretations
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_candidate_interpretations AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_candidate_interpretations AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_candidate_interpretations AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_candidate_interpretations AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
