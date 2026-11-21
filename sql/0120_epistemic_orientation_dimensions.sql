-- Interest, curiosity and avoidance are qualified orientations, not a personality score.
BEGIN;
ALTER TABLE niccolo.actor_epistemic_state_events DROP CONSTRAINT actor_epistemic_state_events_dimension_check;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_state_events_dimension_check CHECK (dimension IN (
  'EXPOSURE','ACCESS','PERCEPTION','ATTENTION','EXAMINATION','COMPREHENSION','RECALL','SUSPICION','INCLINATION','DOUBT','BELIEF','SUSPENDED_JUDGMENT','CLAIMED_KNOWLEDGE','PERCEIVED_KNOWLEDGE','EVIDENCE_SUPPORTED_KNOWLEDGE',
  'INTEREST','CURIOSITY','INDIFFERENCE','AVERSION','LIMITED_INQUIRY','DENIABILITY_PRESERVATION','DELEGATED_KNOWING','DOCUMENTATION_AVOIDANCE'));
ALTER TABLE niccolo.actor_epistemic_state_events ADD COLUMN orientation_scope TEXT CHECK (orientation_scope IN ('IMMEDIATE_REACTION','PERSISTENT_DISPOSITION'));
ALTER TABLE niccolo.actor_epistemic_state_events ADD COLUMN orientation_goal TEXT CHECK (length(orientation_goal) BETWEEN 1 AND 2048 AND btrim(orientation_goal)<>'');
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_orientation_qualification CHECK (
  (dimension IN ('INTEREST','CURIOSITY','INDIFFERENCE','AVERSION','LIMITED_INQUIRY','DENIABILITY_PRESERVATION','DELEGATED_KNOWING','DOCUMENTATION_AVOIDANCE') AND orientation_scope IS NOT NULL AND orientation_goal IS NOT NULL)
  OR (dimension NOT IN ('INTEREST','CURIOSITY','INDIFFERENCE','AVERSION','LIMITED_INQUIRY','DENIABILITY_PRESERVATION','DELEGATED_KNOWING','DOCUMENTATION_AVOIDANCE') AND orientation_scope IS NULL AND orientation_goal IS NULL));
CREATE FUNCTION niccolo.validate_actor_orientation_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.revises_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.actor_epistemic_state_events p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.event_id=NEW.revises_event_id AND p.orientation_scope IS NOT DISTINCT FROM NEW.orientation_scope AND p.orientation_goal IS NOT DISTINCT FROM NEW.orientation_goal)
  THEN RAISE EXCEPTION 'Orientation revision must preserve scope and goal' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER actor_orientation_revision BEFORE INSERT ON niccolo.actor_epistemic_state_events FOR EACH ROW EXECUTE FUNCTION niccolo.validate_actor_orientation_revision();
COMMIT;
