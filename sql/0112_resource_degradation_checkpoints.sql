-- ARM-002: visible non-terminal resource degradation checkpoints.
-- Resource pressure may defer one explicitly bounded nonessential reasoning pass,
-- but never upgrades the resulting analysis to complete/truth/authority.
BEGIN;
ALTER TABLE niccolo.bff_operation_checkpoints
  DROP CONSTRAINT bff_operation_checkpoints_checkpoint_kind_check,
  DROP CONSTRAINT bff_operation_checkpoints_check;
ALTER TABLE niccolo.bff_operation_checkpoints
  ADD CONSTRAINT bff_operation_checkpoints_checkpoint_kind_check CHECK (
    checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY','BUDGET_BOUNDARY','RESOURCE_DEGRADATION','FINAL')
  ),
  ADD CONSTRAINT bff_operation_checkpoints_check CHECK (
    (checkpoint_kind='FINAL' AND terminal AND NOT provisional)
    OR (checkpoint_kind='BUDGET_BOUNDARY' AND terminal AND provisional)
    OR (checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY','RESOURCE_DEGRADATION') AND NOT terminal AND provisional)
  );
COMMIT;
