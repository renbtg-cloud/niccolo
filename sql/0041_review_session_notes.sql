-- v0.82: append-only human review notes and bookmarks.
BEGIN;
CREATE TABLE niccolo.situation_review_session_notes (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  session_id UUID NOT NULL,
  note_id UUID NOT NULL,
  note_kind TEXT NOT NULL CHECK (note_kind IN ('NOTE','BOOKMARK')),
  target_type TEXT CHECK (target_type IS NULL OR target_type IN ('CLAIM','UNCERTAINTY','ACTION_PROPOSAL','CONTEXT','SOURCE','ANALYSIS')),
  target_id UUID,
  note_text TEXT NOT NULL CHECK (length(note_text) BETWEEN 1 AND 4096 AND btrim(note_text) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,session_id,note_id),
  FOREIGN KEY (workspace_id,situation_id,session_id)
    REFERENCES niccolo.situation_review_sessions(workspace_id,situation_id,session_id),
  CHECK ((target_type IS NULL) = (target_id IS NULL))
);
GRANT SELECT,INSERT ON niccolo.situation_review_session_notes TO niccolo_app;
ALTER TABLE niccolo.situation_review_session_notes ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_review_session_notes FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_review_session_notes USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_review_session_notes AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_review_session_notes AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
