-- Preserve a qualified counterparty without establishing intent or causation.
BEGIN;
ALTER TABLE niccolo.actor_epistemic_state_events ADD COLUMN counterparty_actor_id UUID;
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_counterparty_fk FOREIGN KEY(workspace_id,counterparty_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id);
ALTER TABLE niccolo.actor_epistemic_state_events ADD CONSTRAINT actor_epistemic_counterparty_qualification CHECK (
 counterparty_actor_id IS NULL OR (counterparty_actor_id<>subject_actor_id AND dimension IN ('INCENTIVE','PRESSURE','COMMUNICATED_THREAT','ADVERSE_CONDITION','COERCION','CONSTRAINT','DEPENDENCY','LEVERAGE')));
CREATE INDEX actor_epistemic_counterparty ON niccolo.actor_epistemic_state_events(workspace_id,situation_id,counterparty_actor_id) WHERE counterparty_actor_id IS NOT NULL;
CREATE FUNCTION niccolo.validate_actor_counterparty_revision() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
 IF NEW.revises_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.actor_epistemic_state_events p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.event_id=NEW.revises_event_id AND p.counterparty_actor_id IS NOT DISTINCT FROM NEW.counterparty_actor_id)
 THEN RAISE EXCEPTION 'Revision must preserve the attributed counterparty' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER actor_counterparty_revision BEFORE INSERT ON niccolo.actor_epistemic_state_events FOR EACH ROW EXECUTE FUNCTION niccolo.validate_actor_counterparty_revision();
COMMIT;
