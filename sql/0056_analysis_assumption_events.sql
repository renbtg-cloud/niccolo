-- v0.113-v0.114: append-only assumption challenge/status history.
BEGIN;
CREATE TABLE niccolo.situation_analysis_assumption_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  assumption_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('CHALLENGED','RETAINED_FOR_ANALYSIS','RETRACTED_FOR_ANALYSIS')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,assumption_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,assumption_id)
    REFERENCES niccolo.situation_analysis_assumptions(workspace_id,situation_id,analysis_id,assumption_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_assumption_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_assumption_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_assumption_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_assumption_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_assumption_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_assumption_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
