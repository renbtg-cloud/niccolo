-- v0.26: explicit record of Situation contributions actually used by a frozen analysis.
BEGIN;
ALTER TABLE niccolo.situation_analysis_snapshots
  ADD CONSTRAINT situation_analysis_snapshot_input_revision_unique
  UNIQUE (workspace_id,situation_id,analysis_id,input_revision);

CREATE TABLE niccolo.situation_analysis_used_contributions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  input_revision BIGINT NOT NULL CHECK (input_revision >= 1),
  contribution_id UUID NOT NULL,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,contribution_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,input_revision)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision),
  FOREIGN KEY (workspace_id,situation_id,input_revision,contribution_id)
    REFERENCES niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id)
);
CREATE INDEX situation_analysis_used_contributions_by_input
  ON niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,input_revision,contribution_id);
GRANT SELECT, INSERT ON niccolo.situation_analysis_used_contributions TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_used_contributions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_used_contributions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_used_contributions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_used_contributions AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_used_contributions AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_used_contributions AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_used_contributions AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
