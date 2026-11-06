-- v0.106: decision points expose choices to the human; they do not decide automatically.
BEGIN;
CREATE TABLE niccolo.situation_analysis_decision_points (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  question TEXT NOT NULL CHECK (length(question) BETWEEN 1 AND 4096 AND btrim(question) <> ''),
  context_note TEXT CHECK (context_note IS NULL OR (length(context_note) <= 4096 AND btrim(context_note) <> '')),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,decision_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_decision_points TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_decision_points ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_decision_points FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_decision_points USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_decision_points AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_decision_points AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
