-- Attributed judgments about a question, not permission grants or a ranking.
BEGIN;
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_dimension_check;
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_dimension_result_check;
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_check
  CHECK (dimension IN ('CREDIBILITY','RELEVANCE','RELIABILITY','INDEPENDENCE','DISCRIMINATORY_POWER','GAP_RESOLUTION',
    'ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE'));
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_result_check
  CHECK (result IN ('UNKNOWN','CONTESTED') OR
    (dimension='CREDIBILITY' AND result IN ('CREDIBLE','QUESTIONABLE','NOT_CREDIBLE')) OR
    (dimension='RELEVANCE' AND result IN ('RELEVANT','LIMITED_RELEVANCE','IRRELEVANT')) OR
    (dimension='RELIABILITY' AND result IN ('RELIABLE','LIMITED_RELIABILITY','UNRELIABLE')) OR
    (dimension='INDEPENDENCE' AND result IN ('INDEPENDENCE_SUPPORTED','DEPENDENCE_INDICATED','DEPENDENT')) OR
    (dimension='DISCRIMINATORY_POWER' AND result IN ('HIGH_POWER','LIMITED_POWER','NO_POWER')) OR
    (dimension='GAP_RESOLUTION' AND result IN ('OPEN','PARTIALLY_ADDRESSED','SUPPORTED','UNRESOLVED','UNRESOLVABLE')) OR
    (dimension='ACQUISITION_COST' AND result IN ('LOW_COST','MODERATE_COST','HIGH_COST')) OR
    (dimension='ACCESSIBILITY' AND result IN ('ACCESSIBLE','PARTIALLY_ACCESSIBLE','INACCESSIBLE')) OR
    (dimension='CONTAMINATION_RISK' AND result IN ('LOW_RISK','MODERATE_RISK','HIGH_RISK')) OR
    (dimension='URGENCY' AND result IN ('URGENT','TIME_SENSITIVE','DEFERRABLE')) OR
    (dimension='AUTHORIZATION_STATUS' AND result IN ('AUTHORIZATION_REPORTED','AUTHORIZATION_REQUIRED','AUTHORIZATION_DENIED')) OR
    (dimension='INVESTIGATION_COVERAGE' AND result IN ('UNEXAMINED','PARTIALLY_EXAMINED','EXAMINED')));

CREATE FUNCTION niccolo.validate_investigative_selection_target() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.dimension IN ('ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE')
    AND NOT EXISTS (SELECT 1 FROM niccolo.epistemic_questions q
      WHERE q.workspace_id=NEW.workspace_id AND q.situation_id=NEW.situation_id AND q.question_id=NEW.target_object_id)
  THEN RAISE EXCEPTION 'Selection criterion must target a question in this Situation' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER investigative_selection_target BEFORE INSERT ON niccolo.epistemic_assessment_events
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_investigative_selection_target();
COMMIT;
