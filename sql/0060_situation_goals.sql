-- v0.130-v0.134: human-owned Situation goals and explicit success criteria.
BEGIN;
CREATE TABLE niccolo.situation_goals (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  goal_id UUID NOT NULL,
  goal_text TEXT NOT NULL CHECK (length(goal_text) BETWEEN 1 AND 4096 AND btrim(goal_text) <> ''),
  success_criterion TEXT NOT NULL CHECK (length(success_criterion) BETWEEN 1 AND 4096 AND btrim(success_criterion) <> ''),
  owner_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,goal_id),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE TABLE niccolo.situation_goal_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  goal_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('ACHIEVED','REOPENED','ABANDONED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,goal_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,goal_id) REFERENCES niccolo.situation_goals(workspace_id,situation_id,goal_id)
);
GRANT SELECT,INSERT ON niccolo.situation_goals,niccolo.situation_goal_events TO niccolo_app;
ALTER TABLE niccolo.situation_goals ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_goals FORCE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_goal_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_goal_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_goals USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY workspace_filter ON niccolo.situation_goal_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_goals AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_goals AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_read_gate ON niccolo.situation_goal_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_goal_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
