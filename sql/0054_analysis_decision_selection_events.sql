-- v0.108-v0.110: append-only human selection history; selection is not execution.
BEGIN;
CREATE TABLE niccolo.situation_analysis_decision_selection_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('SELECTED','CLEARED','REOPENED')),
  option_id UUID,
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,decision_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,decision_id)
    REFERENCES niccolo.situation_analysis_decision_points(workspace_id,situation_id,analysis_id,decision_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,decision_id,option_id)
    REFERENCES niccolo.situation_analysis_decision_options(workspace_id,situation_id,analysis_id,decision_id,option_id),
  CHECK ((event_kind='SELECTED' AND option_id IS NOT NULL) OR (event_kind<>'SELECTED' AND option_id IS NULL))
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_decision_selection_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_decision_selection_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_decision_selection_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_decision_selection_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_decision_selection_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_decision_selection_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
