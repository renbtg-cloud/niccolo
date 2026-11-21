-- Explicitly consulted integrity judgments may inform a human decision, but do not veto it.
BEGIN;
CREATE OR REPLACE FUNCTION niccolo.validate_epistemic_question_decision() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF (SELECT count(*) FROM niccolo.epistemic_questions q WHERE q.workspace_id=NEW.workspace_id AND q.situation_id=NEW.situation_id AND q.question_id=ANY(NEW.considered_question_ids)) <> cardinality(NEW.considered_question_ids)
  THEN RAISE EXCEPTION 'Considered questions must be distinct and in this Situation' USING ERRCODE='23514'; END IF;
  IF (SELECT count(*) FROM niccolo.epistemic_assessment_events a WHERE a.workspace_id=NEW.workspace_id AND a.situation_id=NEW.situation_id AND a.event_id=ANY(NEW.assessment_event_ids) AND a.target_object_id=ANY(NEW.considered_question_ids) AND (a.dimension IN ('RELEVANCE','DISCRIMINATORY_POWER','ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE') OR left(a.dimension,10)='INTEGRITY_')) <> cardinality(NEW.assessment_event_ids)
  THEN RAISE EXCEPTION 'Decision basis must refer to distinct selection or integrity judgments on considered questions' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
COMMIT;
