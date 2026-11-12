-- GIT-001 first slice: user-declared GitHub publication target metadata and
-- explicit immutable publication intents. No provider token, OAuth secret, source
-- text or remote-publish claim is stored here. A later publisher must verify the
-- remote repository/ref before changing publication state in a later migration.
BEGIN;

CREATE TABLE niccolo.git_publication_targets (
  workspace_id UUID NOT NULL,
  target_id UUID NOT NULL,
  provider TEXT NOT NULL CHECK(provider='GITHUB'),
  repository_owner TEXT NOT NULL CHECK(length(repository_owner) BETWEEN 1 AND 100),
  repository_name TEXT NOT NULL CHECK(length(repository_name) BETWEEN 1 AND 100),
  ref_name TEXT NOT NULL CHECK(length(ref_name) BETWEEN 1 AND 244),
  repository_visibility TEXT NOT NULL CHECK(repository_visibility IN ('PRIVATE','PUBLIC','UNKNOWN')),
  sensitive_exports_allowed BOOLEAN NOT NULL DEFAULT FALSE,
  authorization_mode TEXT NOT NULL DEFAULT 'TRANSIENT_PROVIDER_AUTHORIZATION'
    CHECK(authorization_mode='TRANSIENT_PROVIDER_AUTHORIZATION'),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,target_id),
  UNIQUE(workspace_id,repository_owner,repository_name,ref_name)
);

CREATE TABLE niccolo.git_publication_intents (
  workspace_id UUID NOT NULL,
  publication_id UUID NOT NULL,
  target_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  scope_kind TEXT NOT NULL CHECK(scope_kind='STANDARD_PRESENTATION'),
  package_sha256 CHAR(64) NOT NULL CHECK(package_sha256 ~ '^[a-f0-9]{64}$'),
  file_count INTEGER NOT NULL CHECK(file_count BETWEEN 1 AND 10000),
  total_text_bytes BIGINT NOT NULL CHECK(total_text_bytes BETWEEN 1 AND 8388608),
  requested_ref TEXT NOT NULL CHECK(length(requested_ref) BETWEEN 1 AND 244),
  includes_examined_source_text BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_examined_source_text=FALSE),
  includes_original_uploads BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_original_uploads=FALSE),
  publication_state TEXT NOT NULL DEFAULT 'AWAITING_REMOTE_AUTHORIZATION'
    CHECK(publication_state='AWAITING_REMOTE_AUTHORIZATION'),
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,publication_id),
  FOREIGN KEY(workspace_id,target_id)
    REFERENCES niccolo.git_publication_targets(workspace_id,target_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE INDEX git_publication_intents_by_analysis
  ON niccolo.git_publication_intents(workspace_id,situation_id,analysis_id,created_at DESC,publication_id);

GRANT SELECT,INSERT ON niccolo.git_publication_targets TO niccolo_app;
GRANT SELECT,INSERT ON niccolo.git_publication_intents TO niccolo_app;

ALTER TABLE niccolo.git_publication_targets ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.git_publication_targets FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.git_publication_targets
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.git_publication_targets AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.git_publication_targets AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.git_publication_targets AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.git_publication_targets AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

ALTER TABLE niccolo.git_publication_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.git_publication_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.git_publication_intents
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.git_publication_intents AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.git_publication_intents AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.git_publication_intents AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.git_publication_intents AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
