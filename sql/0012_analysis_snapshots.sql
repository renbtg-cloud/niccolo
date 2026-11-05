-- v0.17: immutable analysis-input snapshots.
-- A future reasoning/model invocation can bind to one exact Situation revision;
-- later human context never changes what that invocation is recorded as having seen.
BEGIN;
CREATE TABLE niccolo.situation_analysis_snapshots (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  input_revision BIGINT NOT NULL CHECK (input_revision >= 1),
  input_fingerprint CHAR(64) NOT NULL CHECK (input_fingerprint ~ '^[a-f0-9]{64}$'),
  goal TEXT NOT NULL CHECK (length(goal) BETWEEN 1 AND 4096 AND btrim(goal) <> ''),
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, analysis_id),
  FOREIGN KEY (workspace_id, situation_id, input_revision)
    REFERENCES niccolo.situation_revisions(workspace_id, situation_id, revision)
);
CREATE INDEX situation_analysis_snapshots_by_revision
  ON niccolo.situation_analysis_snapshots(workspace_id, situation_id, input_revision, analysis_id);

GRANT SELECT, INSERT ON niccolo.situation_analysis_snapshots TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_snapshots
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_snapshots AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_snapshots AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_snapshots AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_snapshots AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
