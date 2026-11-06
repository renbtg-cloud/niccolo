-- v0.81: explicit human-owned review sessions. Ending a session is append-only.
BEGIN;
CREATE TABLE niccolo.situation_review_sessions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  session_id UUID NOT NULL,
  analysis_id UUID,
  purpose TEXT NOT NULL CHECK (length(purpose) BETWEEN 1 AND 2048 AND btrim(purpose) <> ''),
  started_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  started_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,session_id),
  FOREIGN KEY (workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE TABLE niccolo.situation_review_session_ends (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  session_id UUID NOT NULL,
  end_id UUID NOT NULL,
  note TEXT CHECK (note IS NULL OR (length(note) <= 4096 AND btrim(note) <> '')),
  ended_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  ended_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,session_id,end_id),
  UNIQUE (workspace_id,situation_id,session_id),
  FOREIGN KEY (workspace_id,situation_id,session_id)
    REFERENCES niccolo.situation_review_sessions(workspace_id,situation_id,session_id)
);
GRANT SELECT,INSERT ON niccolo.situation_review_sessions,niccolo.situation_review_session_ends TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['situation_review_sessions','situation_review_session_ends'] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
END LOOP; END $$;
COMMIT;
