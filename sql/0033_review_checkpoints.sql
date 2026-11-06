-- v0.62: immutable human review checkpoints for "what changed since I looked".
BEGIN;
CREATE TABLE niccolo.situation_review_checkpoints (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  checkpoint_id UUID NOT NULL,
  observed_revision BIGINT NOT NULL CHECK (observed_revision >= 1),
  observed_analysis_id UUID,
  note TEXT CHECK (note IS NULL OR (length(note) <= 4096 AND btrim(note) <> '')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,checkpoint_id),
  FOREIGN KEY (workspace_id,situation_id,observed_revision)
    REFERENCES niccolo.situation_revisions(workspace_id,situation_id,revision),
  FOREIGN KEY (workspace_id,situation_id,observed_analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
GRANT SELECT, INSERT ON niccolo.situation_review_checkpoints TO niccolo_app;
ALTER TABLE niccolo.situation_review_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_review_checkpoints FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_review_checkpoints
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_review_checkpoints AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_review_checkpoints AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
