-- GIT-003: optionally bind an exact checkpoint publication intent to one
-- immutable human milestone declaration. Existing checkpoint-only intents stay
-- valid; milestone-linked intents publish a package that includes milestone.json.
BEGIN;

ALTER TABLE niccolo.checkpoint_git_publication_intents
  ADD COLUMN milestone_id UUID;

ALTER TABLE niccolo.checkpoint_git_publication_intents
  DROP CONSTRAINT checkpoint_git_publication_intents_scope_kind_check;
ALTER TABLE niccolo.checkpoint_git_publication_intents
  ADD CONSTRAINT checkpoint_git_publication_intents_scope_kind_check
    CHECK(scope_kind IN ('OPERATION_CHECKPOINT','OPERATION_MILESTONE'));
ALTER TABLE niccolo.operation_checkpoint_milestones
  ADD CONSTRAINT operation_checkpoint_milestones_exact_scope_key
    UNIQUE(workspace_id,situation_id,operation_id,checkpoint_id,milestone_id);
ALTER TABLE niccolo.checkpoint_git_publication_intents
  ADD CONSTRAINT checkpoint_git_publication_intents_milestone_fk
    FOREIGN KEY(workspace_id,situation_id,operation_id,checkpoint_id,milestone_id)
    REFERENCES niccolo.operation_checkpoint_milestones(
      workspace_id,situation_id,operation_id,checkpoint_id,milestone_id
    );
ALTER TABLE niccolo.checkpoint_git_publication_intents
  ADD CONSTRAINT checkpoint_git_publication_intents_milestone_scope_check
    CHECK(
      (scope_kind='OPERATION_CHECKPOINT' AND milestone_id IS NULL)
      OR (scope_kind='OPERATION_MILESTONE' AND milestone_id IS NOT NULL)
    );
CREATE INDEX checkpoint_git_publication_intents_by_milestone
  ON niccolo.checkpoint_git_publication_intents(workspace_id,milestone_id)
  WHERE milestone_id IS NOT NULL;

COMMIT;
