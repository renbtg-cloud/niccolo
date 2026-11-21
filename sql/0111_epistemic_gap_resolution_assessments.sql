-- Gap resolution is an attributed assessment, not a consequence of receiving an answer.
BEGIN;
DO $$ DECLARE c RECORD; BEGIN
  FOR c IN SELECT conname FROM pg_constraint
    WHERE conrelid='niccolo.epistemic_assessment_events'::regclass AND contype='c'
      AND pg_get_constraintdef(oid) LIKE '%DISCRIMINATORY_POWER%'
  LOOP
    EXECUTE format('ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT %I',c.conname);
  END LOOP;
END $$;
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_check
  CHECK (dimension IN ('CREDIBILITY','RELEVANCE','RELIABILITY','INDEPENDENCE','DISCRIMINATORY_POWER','GAP_RESOLUTION'));
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_result_check
  CHECK (result IN ('UNKNOWN','CONTESTED') OR
    (dimension='CREDIBILITY' AND result IN ('CREDIBLE','QUESTIONABLE','NOT_CREDIBLE')) OR
    (dimension='RELEVANCE' AND result IN ('RELEVANT','LIMITED_RELEVANCE','IRRELEVANT')) OR
    (dimension='RELIABILITY' AND result IN ('RELIABLE','LIMITED_RELIABILITY','UNRELIABLE')) OR
    (dimension='INDEPENDENCE' AND result IN ('INDEPENDENCE_SUPPORTED','DEPENDENCE_INDICATED','DEPENDENT')) OR
    (dimension='DISCRIMINATORY_POWER' AND result IN ('HIGH_POWER','LIMITED_POWER','NO_POWER')) OR
    (dimension='GAP_RESOLUTION' AND result IN ('OPEN','PARTIALLY_ADDRESSED','SUPPORTED','UNRESOLVED','UNRESOLVABLE')));

CREATE FUNCTION niccolo.validate_epistemic_gap_resolution_target() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.dimension='GAP_RESOLUTION' AND NOT EXISTS (
    SELECT 1 FROM niccolo.epistemic_information_gaps g
    WHERE g.workspace_id=NEW.workspace_id AND g.situation_id=NEW.situation_id
      AND g.gap_id=NEW.target_object_id
  ) THEN
    RAISE EXCEPTION 'Gap resolution must target a gap in this Situation' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER epistemic_gap_resolution_target BEFORE INSERT ON niccolo.epistemic_assessment_events
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_epistemic_gap_resolution_target();

-- Basis rows are inserted after the event, in the same transaction.
CREATE FUNCTION niccolo.validate_epistemic_gap_resolution_basis() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.dimension='GAP_RESOLUTION' AND NEW.result IN ('PARTIALLY_ADDRESSED','SUPPORTED') AND NOT EXISTS (
    SELECT 1 FROM niccolo.epistemic_assessment_basis b
    LEFT JOIN niccolo.epistemic_objects o ON o.workspace_id=b.workspace_id AND o.object_id=b.ground_object_id
    WHERE b.workspace_id=NEW.workspace_id AND b.event_id=NEW.event_id
      AND (b.ground_evidence_id IS NOT NULL OR o.object_kind NOT IN ('QUESTION','INFORMATION_GAP'))
  ) THEN
    RAISE EXCEPTION 'Gap resolution requires an explicit non-question basis' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER epistemic_gap_resolution_basis AFTER INSERT ON niccolo.epistemic_assessment_events
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION niccolo.validate_epistemic_gap_resolution_basis();
COMMIT;
