-- GIT-003 first slice: explicit immutable human milestone declarations for
-- durable operation checkpoints. A milestone says "this checkpoint is worth
-- preserving/publishing"; it does not upgrade evidence, truth, completeness,
-- checkpoint finality, or authorization to act.
BEGIN;

CREATE TABLE niccolo.operation_checkpoint_milestones (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  checkpoint_id UUID NOT NULL,
  milestone_id UUID NOT NULL,
  milestone_kind TEXT NOT NULL CHECK(
    milestone_kind IN ('TIMED_PARTIAL','FIRST_PASS','HUMAN_ACKNOWLEDGED_REVIEW','FINAL_SYNTHESIS','OTHER')
  ),
  reason TEXT NOT NULL CHECK(length(btrim(reason)) BETWEEN 1 AND 1000),
  declared_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,milestone_id),
  UNIQUE(workspace_id,situation_id,operation_id,checkpoint_id),
  FOREIGN KEY(workspace_id,situation_id,operation_id,checkpoint_id)
    REFERENCES niccolo.bff_operation_checkpoints(workspace_id,situation_id,operation_id,checkpoint_id)
);
CREATE INDEX operation_checkpoint_milestones_by_operation
  ON niccolo.operation_checkpoint_milestones(
    workspace_id,situation_id,operation_id,created_at,milestone_id
  );

GRANT SELECT,INSERT ON niccolo.operation_checkpoint_milestones TO niccolo_app;
ALTER TABLE niccolo.operation_checkpoint_milestones ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.operation_checkpoint_milestones FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.operation_checkpoint_milestones
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.operation_checkpoint_milestones AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.operation_checkpoint_milestones AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.operation_checkpoint_milestones AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.operation_checkpoint_milestones AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
