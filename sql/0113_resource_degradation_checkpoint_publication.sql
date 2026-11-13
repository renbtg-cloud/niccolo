-- GIT-002 / ARM-002: resource-degradation checkpoints remain publishable as
-- bounded provisional checkpoint packages. Publication never upgrades them to
-- complete/truth/authority.
BEGIN;
ALTER TABLE niccolo.checkpoint_git_publication_intents
  DROP CONSTRAINT checkpoint_git_publication_intents_checkpoint_kind_check,
  DROP CONSTRAINT checkpoint_git_publication_intents_check;
ALTER TABLE niccolo.checkpoint_git_publication_intents
  ADD CONSTRAINT checkpoint_git_publication_intents_checkpoint_kind_check CHECK (
    checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY','BUDGET_BOUNDARY','RESOURCE_DEGRADATION','FINAL')
  ),
  ADD CONSTRAINT checkpoint_git_publication_intents_check CHECK (
    (checkpoint_kind='FINAL' AND terminal AND NOT provisional)
    OR (checkpoint_kind='BUDGET_BOUNDARY' AND terminal AND provisional)
    OR (checkpoint_kind IN ('STEP_COMPLETED','HUMAN_BOUNDARY','RESOURCE_DEGRADATION') AND NOT terminal AND provisional)
  );
COMMIT;
