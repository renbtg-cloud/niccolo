-- GIT-002 generic intake slice: immutable intent to publish one deterministic
-- RECEIVED source receipt. This table stores no provider credential and does not
-- claim that source bytes, source text, analysis, authenticity or truth were published.
BEGIN;

CREATE TABLE niccolo.intake_git_publication_intents (
  workspace_id UUID NOT NULL,
  publication_id UUID NOT NULL,
  target_id UUID NOT NULL,
  document_id UUID NOT NULL,
  source_sha256 CHAR(64) NOT NULL CHECK(source_sha256 ~ '^[a-f0-9]{64}$'),
  package_sha256 CHAR(64) NOT NULL CHECK(package_sha256 ~ '^[a-f0-9]{64}$'),
  file_count INTEGER NOT NULL CHECK(file_count BETWEEN 1 AND 16),
  total_text_bytes BIGINT NOT NULL CHECK(total_text_bytes BETWEEN 1 AND 65536),
  requested_ref TEXT NOT NULL CHECK(length(requested_ref) BETWEEN 1 AND 244),
  scope_kind TEXT NOT NULL DEFAULT 'INTAKE_RECEIPT' CHECK(scope_kind='INTAKE_RECEIPT'),
  includes_original_uploads BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_original_uploads=FALSE),
  includes_source_text BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_source_text=FALSE),
  includes_analysis BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_analysis=FALSE),
  publication_state TEXT NOT NULL DEFAULT 'AWAITING_REMOTE_AUTHORIZATION'
    CHECK(publication_state='AWAITING_REMOTE_AUTHORIZATION'),
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,publication_id),
  FOREIGN KEY(workspace_id,target_id)
    REFERENCES niccolo.git_publication_targets(workspace_id,target_id),
  FOREIGN KEY(workspace_id,document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id)
);
CREATE INDEX intake_git_publication_intents_by_document
  ON niccolo.intake_git_publication_intents(
    workspace_id,document_id,created_at DESC,publication_id
  );

GRANT SELECT,INSERT ON niccolo.intake_git_publication_intents TO niccolo_app;
GRANT SELECT,INSERT ON niccolo.intake_git_publication_intents TO niccolo_api;

ALTER TABLE niccolo.intake_git_publication_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.intake_git_publication_intents FORCE ROW LEVEL SECURITY;

CREATE POLICY workspace_filter ON niccolo.intake_git_publication_intents
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);

CREATE POLICY api_read_gate ON niccolo.intake_git_publication_intents AS RESTRICTIVE
  FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));

CREATE POLICY api_write_insert_gate ON niccolo.intake_git_publication_intents AS RESTRICTIVE
  FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
