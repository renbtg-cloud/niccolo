-- v0.135-v0.140: unranked next-step proposals with explicit human disposition; never execution.
BEGIN;
CREATE TABLE niccolo.situation_analysis_next_steps (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  step_id UUID NOT NULL,
  step_kind TEXT NOT NULL CHECK (step_kind IN ('ASK_HUMAN','GATHER_EVIDENCE','REANALYZE','USER_ACTION')),
  step_text TEXT NOT NULL CHECK (length(step_text) BETWEEN 1 AND 4096 AND btrim(step_text) <> ''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  proposed_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,step_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id) REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE TABLE niccolo.situation_analysis_next_step_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  step_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('ACCEPTED','DEFERRED','DISMISSED','REOPENED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,step_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,step_id) REFERENCES niccolo.situation_analysis_next_steps(workspace_id,situation_id,analysis_id,step_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_next_steps,niccolo.situation_analysis_next_step_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_next_steps ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_next_steps FORCE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_next_step_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_next_step_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_next_steps USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY workspace_filter ON niccolo.situation_analysis_next_step_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_next_steps AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_next_steps AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_read_gate ON niccolo.situation_analysis_next_step_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_next_step_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
