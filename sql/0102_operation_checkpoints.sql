-- ARM-003: immutable anytime-operation checkpoints.
-- A checkpoint is a durable progress observation, not proof of correctness,
-- completeness, truth, or a promise that the operation will eventually succeed.
BEGIN;
CREATE TABLE niccolo.bff_operation_checkpoints (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  checkpoint_id UUID NOT NULL,
  checkpoint_seq BIGINT GENERATED ALWAYS AS IDENTITY,
  analysis_id UUID,
  checkpoint_kind TEXT NOT NULL CHECK (
    checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY','BUDGET_BOUNDARY','FINAL')
  ),
  step_id UUID,
  summary_json JSONB NOT NULL CHECK (
    jsonb_typeof(summary_json)='object'
    AND octet_length(summary_json::text) BETWEEN 2 AND 262144
  ),
  summary_sha256 CHAR(64) NOT NULL CHECK (summary_sha256 ~ '^[a-f0-9]{64}$'),
  provisional BOOLEAN NOT NULL,
  terminal BOOLEAN NOT NULL,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,operation_id,checkpoint_id),
  UNIQUE(workspace_id,situation_id,operation_id,checkpoint_seq),
  FOREIGN KEY(workspace_id,situation_id,operation_id)
    REFERENCES niccolo.bff_operations(workspace_id,situation_id,operation_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  CHECK (
    (checkpoint_kind='FINAL' AND terminal AND NOT provisional)
    OR (checkpoint_kind='BUDGET_BOUNDARY' AND terminal AND provisional)
    OR (checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY') AND NOT terminal AND provisional)
  )
);
CREATE INDEX bff_operation_checkpoints_cursor
  ON niccolo.bff_operation_checkpoints(
    workspace_id,situation_id,operation_id,checkpoint_seq
  );

GRANT SELECT,INSERT ON niccolo.bff_operation_checkpoints TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.bff_operation_checkpoints_checkpoint_seq_seq TO niccolo_app;
ALTER TABLE niccolo.bff_operation_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.bff_operation_checkpoints FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.bff_operation_checkpoints
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.bff_operation_checkpoints AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.bff_operation_checkpoints AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.bff_operation_checkpoints AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.bff_operation_checkpoints AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
