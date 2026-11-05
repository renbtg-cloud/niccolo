-- v0.19: immutable completed answers bound to one frozen analysis input.
-- A reanalysis uses a new analysis_id; an earlier answer is never rewritten in place.
BEGIN;
CREATE TABLE niccolo.situation_analysis_outputs (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  answer_text TEXT NOT NULL CHECK (btrim(answer_text) <> ''),
  coverage_note TEXT NOT NULL CHECK (btrim(coverage_note) <> ''),
  published_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, analysis_id),
  FOREIGN KEY (workspace_id, situation_id, analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id, situation_id, analysis_id)
);

GRANT SELECT, INSERT ON niccolo.situation_analysis_outputs TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_outputs ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_outputs FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_outputs
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_outputs AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_outputs AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_outputs AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_outputs AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
