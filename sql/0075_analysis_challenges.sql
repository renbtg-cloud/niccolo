-- v0.926-v1.000: append-only adversarial challenges and explicit human disposition events.
-- A challenge is a recorded criticism/hypothesis, never proof that the challenged output is wrong.
BEGIN;
CREATE TABLE niccolo.situation_analysis_challenges (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  challenge_id UUID NOT NULL,
  challenge_kind TEXT NOT NULL CHECK (challenge_kind IN ('COUNTEREVIDENCE','ALTERNATIVE','ASSUMPTION_RISK','COVERAGE_GAP','OTHER')),
  challenge_text TEXT NOT NULL CHECK (length(challenge_text) BETWEEN 1 AND 4096 AND btrim(challenge_text)<>''),
  materiality INTEGER NOT NULL CHECK (materiality BETWEEN 0 AND 100),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,challenge_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id) REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE TABLE niccolo.situation_analysis_challenge_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  challenge_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('ACKNOWLEDGED','WAIVED','ADDRESSED','REOPENED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
  actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,challenge_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,challenge_id) REFERENCES niccolo.situation_analysis_challenges(workspace_id,situation_id,analysis_id,challenge_id)
);
CREATE INDEX situation_analysis_challenge_events_latest ON niccolo.situation_analysis_challenge_events(workspace_id,situation_id,analysis_id,challenge_id,created_at DESC,event_id DESC);
GRANT SELECT,INSERT ON niccolo.situation_analysis_challenges,niccolo.situation_analysis_challenge_events TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['situation_analysis_challenges','situation_analysis_challenge_events'] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
 END LOOP;
END $$;
COMMIT;
