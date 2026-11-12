-- GIT-002 bounded slice: immutable local publication intents for one exact
-- operation-checkpoint package. This migration records intent only; it stores no
-- provider credential and makes no claim that a remote publication occurred.
BEGIN;

CREATE TABLE niccolo.checkpoint_git_publication_intents (
  workspace_id UUID NOT NULL,
  publication_id UUID NOT NULL,
  target_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  checkpoint_id UUID NOT NULL,
  checkpoint_seq BIGINT NOT NULL CHECK(checkpoint_seq > 0),
  checkpoint_kind TEXT NOT NULL CHECK(
    checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY','BUDGET_BOUNDARY','FINAL')
  ),
  provisional BOOLEAN NOT NULL,
  terminal BOOLEAN NOT NULL,
  checkpoint_summary_sha256 CHAR(64) NOT NULL CHECK(checkpoint_summary_sha256 ~ '^[a-f0-9]{64}$'),
  package_sha256 CHAR(64) NOT NULL CHECK(package_sha256 ~ '^[a-f0-9]{64}$'),
  file_count INTEGER NOT NULL CHECK(file_count BETWEEN 1 AND 32),
  total_text_bytes BIGINT NOT NULL CHECK(total_text_bytes BETWEEN 1 AND 524288),
  requested_ref TEXT NOT NULL CHECK(length(requested_ref) BETWEEN 1 AND 244),
  scope_kind TEXT NOT NULL DEFAULT 'OPERATION_CHECKPOINT' CHECK(scope_kind='OPERATION_CHECKPOINT'),
  includes_examined_source_text BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_examined_source_text=FALSE),
  includes_original_uploads BOOLEAN NOT NULL DEFAULT FALSE CHECK(includes_original_uploads=FALSE),
  publication_state TEXT NOT NULL DEFAULT 'AWAITING_REMOTE_AUTHORIZATION'
    CHECK(publication_state='AWAITING_REMOTE_AUTHORIZATION'),
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,publication_id),
  FOREIGN KEY(workspace_id,target_id)
    REFERENCES niccolo.git_publication_targets(workspace_id,target_id),
  FOREIGN KEY(workspace_id,situation_id,operation_id,checkpoint_id)
    REFERENCES niccolo.bff_operation_checkpoints(workspace_id,situation_id,operation_id,checkpoint_id),
  CHECK (
    (checkpoint_kind='FINAL' AND terminal AND NOT provisional)
    OR (checkpoint_kind='BUDGET_BOUNDARY' AND terminal AND provisional)
    OR (checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY') AND NOT terminal AND provisional)
  )
);
CREATE INDEX checkpoint_git_publication_intents_by_checkpoint
  ON niccolo.checkpoint_git_publication_intents(
    workspace_id,situation_id,operation_id,checkpoint_seq,created_at DESC,publication_id
  );

GRANT SELECT,INSERT ON niccolo.checkpoint_git_publication_intents TO niccolo_app;
ALTER TABLE niccolo.checkpoint_git_publication_intents ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.checkpoint_git_publication_intents FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.checkpoint_git_publication_intents
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.checkpoint_git_publication_intents AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.checkpoint_git_publication_intents AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.checkpoint_git_publication_intents AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.checkpoint_git_publication_intents AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
