-- The exact event describing an epistemic state can itself be a target.
-- Existing events remain immutable. A reference must already exist, so it cannot
-- create a cycle even in a multi-row insert. No fixed epistemic-order ceiling.
BEGIN;
ALTER TABLE niccolo.actor_epistemic_state_events ALTER COLUMN target_object_id DROP NOT NULL;
ALTER TABLE niccolo.actor_epistemic_state_events ADD COLUMN target_state_event_id UUID;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_target_exclusive CHECK (num_nonnulls(target_object_id,target_state_event_id)=1 AND target_state_event_id IS DISTINCT FROM event_id);
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_target_state_fk FOREIGN KEY(workspace_id,situation_id,target_state_event_id) REFERENCES niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id);
CREATE INDEX actor_epistemic_state_target_state ON niccolo.actor_epistemic_state_events(workspace_id,situation_id,target_state_event_id) WHERE target_state_event_id IS NOT NULL;
CREATE OR REPLACE FUNCTION niccolo.validate_actor_epistemic_state() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.target_object_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM niccolo.epistemic_situation_propositions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND proposition_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_hypotheses WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND hypothesis_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_assumptions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND assumption_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_questions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND question_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_information_gaps WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND gap_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_findings WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND finding_id=NEW.target_object_id
  ) THEN RAISE EXCEPTION 'State target must belong to this Situation' USING ERRCODE='23514'; END IF;
  IF NEW.target_state_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.actor_epistemic_state_events e WHERE e.workspace_id=NEW.workspace_id AND e.situation_id=NEW.situation_id AND e.event_id=NEW.target_state_event_id)
  THEN RAISE EXCEPTION 'Higher-order target must be a previously recorded state in this Situation' USING ERRCODE='23514'; END IF;
  IF NEW.basis_evidence_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.source_fragments f JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id WHERE f.workspace_id=NEW.workspace_id AND s.situation_id=NEW.situation_id AND f.evidence_id=NEW.basis_evidence_id)
  THEN RAISE EXCEPTION 'State basis must belong to this Situation' USING ERRCODE='23514'; END IF;
  IF NEW.revises_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.actor_epistemic_state_events p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.event_id=NEW.revises_event_id AND p.subject_actor_id=NEW.subject_actor_id AND p.target_object_id IS NOT DISTINCT FROM NEW.target_object_id AND p.target_state_event_id IS NOT DISTINCT FROM NEW.target_state_event_id AND p.dimension=NEW.dimension AND p.context=NEW.context AND p.temporal_scope=NEW.temporal_scope AND p.recorded_by_principal_id=NEW.recorded_by_principal_id)
  THEN RAISE EXCEPTION 'Revision must preserve the attributed state identity' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
COMMIT;
