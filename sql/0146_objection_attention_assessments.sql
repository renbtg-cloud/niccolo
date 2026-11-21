-- AT-26: assess objections without deleting them or creating an automatic veto.
BEGIN;
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_dimension_check;
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_dimension_result_check;
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_check
  CHECK (dimension IN ('CREDIBILITY','RELEVANCE','RELIABILITY','INDEPENDENCE','DISCRIMINATORY_POWER','GAP_RESOLUTION',
    'ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE','INTEGRITY_INVESTIGATIVE_INDEPENDENCE','INTEGRITY_EVIDENCE_SELECTION','INTEGRITY_PROVENANCE','INTEGRITY_QUESTION_FRAMING','INTEGRITY_HYPOTHESIS_COVERAGE','INTEGRITY_REASONING_QUALITY','INTEGRITY_CRITERIA_CONSISTENCY','INTEGRITY_DISSENT_PRESERVATION','INTEGRITY_HISTORICAL_TRACEABILITY','INTEGRITY_REVIEWABILITY','MATERIAL_COMPROMISE','RELIANCE_CONTAINMENT','RECOVERY_READINESS','RECOMMENDATION_APPROPRIATENESS','DECISION_CONTEMPORANEOUS_REASONABLENESS','DECISION_ACTUAL_CIRCUMSTANCES_ADEQUACY','DECISION_REASONABLY_OBTAINABLE_INFORMATION',
    'OBJECTION_NOVELTY','OBJECTION_MATERIALITY','OBJECTION_REVIEW_COST','OBJECTION_ATTENTION_DISPOSITION'));
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_result_check
  CHECK (result IN ('UNKNOWN','CONTESTED') OR
    (dimension='CREDIBILITY' AND result IN ('CREDIBLE','QUESTIONABLE','NOT_CREDIBLE')) OR
    (dimension='RELEVANCE' AND result IN ('RELEVANT','LIMITED_RELEVANCE','IRRELEVANT')) OR
    (dimension='RELIABILITY' AND result IN ('RELIABLE','LIMITED_RELIABILITY','UNRELIABLE')) OR
    (dimension='INDEPENDENCE' AND result IN ('INDEPENDENCE_SUPPORTED','DEPENDENCE_INDICATED','DEPENDENT')) OR
    (dimension='DISCRIMINATORY_POWER' AND result IN ('HIGH_POWER','LIMITED_POWER','NO_POWER')) OR
    (dimension='GAP_RESOLUTION' AND result IN ('OPEN','PARTIALLY_ADDRESSED','SUPPORTED','UNRESOLVED','UNRESOLVABLE')) OR
    (dimension IN ('ACQUISITION_COST','OBJECTION_REVIEW_COST') AND result IN ('LOW_COST','MODERATE_COST','HIGH_COST')) OR
    (dimension='ACCESSIBILITY' AND result IN ('ACCESSIBLE','PARTIALLY_ACCESSIBLE','INACCESSIBLE')) OR
    (dimension='CONTAMINATION_RISK' AND result IN ('LOW_RISK','MODERATE_RISK','HIGH_RISK')) OR
    (dimension='URGENCY' AND result IN ('URGENT','TIME_SENSITIVE','DEFERRABLE')) OR
    (dimension='AUTHORIZATION_STATUS' AND result IN ('AUTHORIZATION_REPORTED','AUTHORIZATION_REQUIRED','AUTHORIZATION_DENIED')) OR
    (dimension='INVESTIGATION_COVERAGE' AND result IN ('UNEXAMINED','PARTIALLY_EXAMINED','EXAMINED')) OR
    (dimension IN ('INTEGRITY_INVESTIGATIVE_INDEPENDENCE','INTEGRITY_EVIDENCE_SELECTION','INTEGRITY_PROVENANCE','INTEGRITY_QUESTION_FRAMING','INTEGRITY_HYPOTHESIS_COVERAGE','INTEGRITY_REASONING_QUALITY','INTEGRITY_CRITERIA_CONSISTENCY','INTEGRITY_DISSENT_PRESERVATION','INTEGRITY_HISTORICAL_TRACEABILITY','INTEGRITY_REVIEWABILITY','MATERIAL_COMPROMISE') AND result IN ('NO_CONCERN_INDICATED','CONCERN_INDICATED','COMPROMISE_INDICATED')) OR
    (dimension='RELIANCE_CONTAINMENT' AND result IN ('PAUSE_INDICATED','LIMIT_INDICATED','NO_PAUSE_INDICATED')) OR
    (dimension='RECOVERY_READINESS' AND result IN ('RECOVERY_READY','RECOVERY_PARTIAL','RECOVERY_NOT_READY')) OR
    (dimension IN ('RECOMMENDATION_APPROPRIATENESS','DECISION_CONTEMPORANEOUS_REASONABLENESS','DECISION_ACTUAL_CIRCUMSTANCES_ADEQUACY','DECISION_REASONABLY_OBTAINABLE_INFORMATION') AND result IN ('ADEQUATE','INADEQUATE','PARTIALLY_ADEQUATE')) OR
    (dimension='OBJECTION_NOVELTY' AND result IN ('NOVEL','PARTIALLY_NOVEL','DUPLICATIVE')) OR
    (dimension='OBJECTION_MATERIALITY' AND result IN ('MATERIAL','LIMITED_MATERIALITY','IMMATERIAL')) OR
    (dimension='OBJECTION_ATTENTION_DISPOSITION' AND result IN ('REVIEW_NOW','DEFER_REVIEW','MONITOR','NO_FURTHER_REVIEW_INDICATED')));

-- The Rust revision contract already requires all dimensions to preserve identity,
-- context, attribution and criterion. Also enforce that contract at storage time.
DROP TRIGGER action_assessment_revision_validate ON niccolo.epistemic_assessment_events;
DROP FUNCTION niccolo.validate_action_assessment_revision();
CREATE FUNCTION niccolo.validate_epistemic_assessment_revision() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE prior niccolo.epistemic_assessment_events%ROWTYPE;
BEGIN
  IF NEW.revises_event_id IS NOT NULL THEN
    SELECT * INTO prior FROM niccolo.epistemic_assessment_events WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND event_id=NEW.revises_event_id;
    IF FOUND AND (
      ROW(NEW.target_object_id,NEW.target_evidence_id,NEW.target_relation_id,NEW.target_argument_id,NEW.target_grounding_set_id,NEW.target_assessment_event_id,NEW.target_recommendation_id,NEW.target_decision_id)
      IS DISTINCT FROM ROW(prior.target_object_id,prior.target_evidence_id,prior.target_relation_id,prior.target_argument_id,prior.target_grounding_set_id,prior.target_assessment_event_id,prior.target_recommendation_id,prior.target_decision_id)
      OR NEW.dimension<>prior.dimension OR NEW.criterion<>prior.criterion OR NEW.context<>prior.context OR NEW.recorded_by_principal_id<>prior.recorded_by_principal_id
    ) THEN RAISE EXCEPTION 'Assessment revision must preserve exact target, dimension, criterion, context and attribution' USING ERRCODE='23514'; END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER epistemic_assessment_revision_validate BEFORE INSERT ON niccolo.epistemic_assessment_events FOR EACH ROW EXECUTE FUNCTION niccolo.validate_epistemic_assessment_revision();
COMMIT;
