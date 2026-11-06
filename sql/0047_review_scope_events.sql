-- v0.91-v0.92: append-only, human-selected scope for a review session.
BEGIN;
CREATE TABLE niccolo.situation_review_scope_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  session_id UUID NOT NULL,
  event_id UUID NOT NULL,
  scope_kind TEXT NOT NULL CHECK (scope_kind IN ('ALL','CLAIMS','SOURCES','CONTEXT','ACTIONS','QUESTIONS','CONDITIONS')),
  event_kind TEXT NOT NULL CHECK (event_kind IN ('SELECTED','DESELECTED')),
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 2048 AND btrim(reason) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,session_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,session_id)
    REFERENCES niccolo.situation_review_sessions(workspace_id,situation_id,session_id)
);
GRANT SELECT,INSERT ON niccolo.situation_review_scope_events TO niccolo_app;
ALTER TABLE niccolo.situation_review_scope_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_review_scope_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_review_scope_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_review_scope_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_review_scope_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
