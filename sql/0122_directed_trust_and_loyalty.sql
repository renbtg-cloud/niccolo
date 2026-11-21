-- Directed, attributed trust and loyalty, distinct from access control.
BEGIN;
ALTER TABLE niccolo.actor_epistemic_state_events ADD COLUMN target_actor_id UUID;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_target_actor_fk FOREIGN KEY(workspace_id,target_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id);
ALTER TABLE niccolo.actor_epistemic_state_events DROP CONSTRAINT actor_epistemic_state_target_exclusive;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_target_exclusive CHECK (num_nonnulls(target_object_id,target_state_event_id,target_actor_id)=1 AND target_state_event_id IS DISTINCT FROM event_id);
ALTER TABLE niccolo.actor_epistemic_state_events DROP CONSTRAINT actor_epistemic_state_events_dimension_check;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_events_dimension_check CHECK (dimension IN (
  'EXPOSURE','ACCESS','PERCEPTION','ATTENTION','EXAMINATION','COMPREHENSION','RECALL','SUSPICION','INCLINATION','DOUBT','BELIEF','SUSPENDED_JUDGMENT','CLAIMED_KNOWLEDGE','PERCEIVED_KNOWLEDGE','EVIDENCE_SUPPORTED_KNOWLEDGE',
  'INTEREST','CURIOSITY','INDIFFERENCE','AVERSION','LIMITED_INQUIRY','DENIABILITY_PRESERVATION','DELEGATED_KNOWING','DOCUMENTATION_AVOIDANCE',
  'TRUST_COMPETENCE','TRUST_CONFIDENTIALITY','TRUST_TRUTHFULNESS','LOYALTY'));
ALTER TABLE niccolo.actor_epistemic_state_events DROP CONSTRAINT actor_epistemic_state_events_judgment_check;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_events_judgment_check CHECK (judgment IN ('INDICATED','NOT_INDICATED','UNKNOWN','CONTESTED','DISTRUST_INDICATED'));
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_relational_qualification CHECK (
  ((dimension IN ('TRUST_COMPETENCE','TRUST_CONFIDENTIALITY','TRUST_TRUTHFULNESS') AND target_actor_id IS NOT NULL)
    OR (dimension='LOYALTY' AND target_state_event_id IS NULL)
    OR (dimension NOT IN ('TRUST_COMPETENCE','TRUST_CONFIDENTIALITY','TRUST_TRUTHFULNESS','LOYALTY') AND target_actor_id IS NULL))
  AND (judgment<>'DISTRUST_INDICATED' OR dimension IN ('TRUST_COMPETENCE','TRUST_CONFIDENTIALITY','TRUST_TRUTHFULNESS')));
CREATE INDEX actor_epistemic_state_target_actor ON niccolo.actor_epistemic_state_events(workspace_id,situation_id,target_actor_id) WHERE target_actor_id IS NOT NULL;
CREATE FUNCTION niccolo.validate_actor_relational_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.revises_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.actor_epistemic_state_events p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.event_id=NEW.revises_event_id AND p.target_actor_id IS NOT DISTINCT FROM NEW.target_actor_id)
  THEN RAISE EXCEPTION 'Relational revision must preserve the directed actor target' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER actor_relational_revision BEFORE INSERT ON niccolo.actor_epistemic_state_events FOR EACH ROW EXECUTE FUNCTION niccolo.validate_actor_relational_revision();
COMMIT;
