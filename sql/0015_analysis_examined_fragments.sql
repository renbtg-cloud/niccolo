-- v0.22: exact source-fragment scope that an analysis says it examined.
-- Presence in a Situation is not the same as examination. Each row is an explicit,
-- attributed coverage claim and must belong to the analysis's frozen input revision.
BEGIN;
CREATE TABLE niccolo.situation_analysis_examined_fragments (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  evidence_id UUID NOT NULL,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, analysis_id, evidence_id),
  FOREIGN KEY (workspace_id, situation_id, analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id, situation_id, analysis_id),
  FOREIGN KEY (workspace_id, evidence_id)
    REFERENCES niccolo.source_fragments(workspace_id, evidence_id)
);
CREATE INDEX situation_analysis_examined_fragments_by_evidence
  ON niccolo.situation_analysis_examined_fragments(workspace_id, situation_id, analysis_id, evidence_id);

GRANT SELECT, INSERT ON niccolo.situation_analysis_examined_fragments TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_examined_fragments ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_examined_fragments FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_examined_fragments
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_examined_fragments AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_examined_fragments AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_examined_fragments AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_examined_fragments AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
