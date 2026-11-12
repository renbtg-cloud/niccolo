-- GIT-002: append-only remote observations for checkpoint publications.
-- Provider credentials and raw provider response bodies are deliberately absent.
BEGIN;

CREATE TABLE niccolo.checkpoint_git_publication_events (
  workspace_id UUID NOT NULL,
  publication_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK(event_kind IN ('PUSH_ACCEPTED','REF_VERIFIED','PUBLISH_FAILED')),
  remote_commit_sha TEXT,
  observed_ref TEXT,
  failure_class TEXT,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  CHECK(remote_commit_sha IS NULL OR remote_commit_sha ~ '^[a-f0-9]{40,64}$'),
  CHECK(observed_ref IS NULL OR length(observed_ref) BETWEEN 1 AND 244),
  CHECK(failure_class IS NULL OR failure_class IN ('TRANSPORT','PROVIDER_REJECTED','VERIFICATION_MISMATCH','LOCAL_INTEGRITY')),
  CHECK(
    (event_kind='PUSH_ACCEPTED' AND remote_commit_sha IS NOT NULL AND observed_ref IS NULL AND failure_class IS NULL)
    OR
    (event_kind='REF_VERIFIED' AND remote_commit_sha IS NOT NULL AND observed_ref IS NOT NULL AND failure_class IS NULL)
    OR
    (event_kind='PUBLISH_FAILED' AND remote_commit_sha IS NULL AND observed_ref IS NULL AND failure_class IS NOT NULL)
  ),
  PRIMARY KEY(workspace_id,publication_id,event_id,event_kind),
  FOREIGN KEY(workspace_id,publication_id)
    REFERENCES niccolo.checkpoint_git_publication_intents(workspace_id,publication_id)
);
CREATE INDEX checkpoint_git_publication_events_by_publication
  ON niccolo.checkpoint_git_publication_events(workspace_id,publication_id,created_at,event_id);

GRANT SELECT,INSERT ON niccolo.checkpoint_git_publication_events TO niccolo_app;
ALTER TABLE niccolo.checkpoint_git_publication_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.checkpoint_git_publication_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.checkpoint_git_publication_events
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.checkpoint_git_publication_events AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.checkpoint_git_publication_events AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.checkpoint_git_publication_events AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.checkpoint_git_publication_events AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
