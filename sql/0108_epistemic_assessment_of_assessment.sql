-- An attributed judgment can itself be the exact target of another judgment.
BEGIN;
ALTER TABLE niccolo.epistemic_assessment_events ADD COLUMN target_assessment_event_id UUID;
DO $$ DECLARE constraint_name TEXT; BEGIN
 SELECT conname INTO STRICT constraint_name FROM pg_constraint
 WHERE conrelid='niccolo.epistemic_assessment_events'::regclass AND contype='c'
   AND pg_get_constraintdef(oid) LIKE '%num_nonnulls(target_object_id, target_evidence_id, target_relation_id, target_argument_id)%';
 EXECUTE format('ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT %I',constraint_name);
END $$;
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_exact_target
 CHECK (num_nonnulls(target_object_id,target_evidence_id,target_relation_id,target_argument_id,target_assessment_event_id)=1);
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_target_not_self
 CHECK (target_assessment_event_id IS NULL OR target_assessment_event_id<>event_id);
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_target_event_fk
 FOREIGN KEY (workspace_id,situation_id,target_assessment_event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id);
COMMIT;
