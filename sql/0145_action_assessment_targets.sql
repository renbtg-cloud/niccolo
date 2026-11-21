-- Reuse general assessment history and grounds for recommendations and historical decisions.
BEGIN;
ALTER TABLE niccolo.epistemic_assessment_events ADD COLUMN target_recommendation_id UUID, ADD COLUMN target_decision_id UUID;
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_exact_target;
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_exact_target CHECK(num_nonnulls(target_object_id,target_evidence_id,target_relation_id,target_argument_id,target_assessment_event_id,target_recommendation_id,target_decision_id)=1);
ALTER TABLE niccolo.epistemic_assessment_events ADD FOREIGN KEY(workspace_id,situation_id,target_recommendation_id) REFERENCES niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id);
ALTER TABLE niccolo.epistemic_assessment_events ADD FOREIGN KEY(workspace_id,situation_id,target_decision_id) REFERENCES niccolo.epistemic_action_decisions(workspace_id,situation_id,decision_id);
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT action_assessment_target_dimension CHECK (
  (target_recommendation_id IS NOT NULL)=(dimension='RECOMMENDATION_APPROPRIATENESS') AND
  (target_decision_id IS NOT NULL)=(dimension IN ('DECISION_CONTEMPORANEOUS_REASONABLENESS','DECISION_ACTUAL_CIRCUMSTANCES_ADEQUACY','DECISION_REASONABLY_OBTAINABLE_INFORMATION'))
);
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_dimension_check;
ALTER TABLE niccolo.epistemic_assessment_events DROP CONSTRAINT epistemic_assessment_dimension_result_check;
ALTER TABLE niccolo.epistemic_assessment_events ADD CONSTRAINT epistemic_assessment_dimension_check
  CHECK (dimension IN ('CREDIBILITY','RELEVANCE','RELIABILITY','INDEPENDENCE','DISCRIMINATORY_POWER','GAP_RESOLUTION',
    'ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE','INTEGRITY_INVESTIGATIVE_INDEPENDENCE','INTEGRITY_EVIDENCE_SELECTION','INTEGRITY_PROVENANCE','INTEGRITY_QUESTION_FRAMING','INTEGRITY_HYPOTHESIS_COVERAGE','INTEGRITY_REASONING_QUALITY','INTEGRITY_CRITERIA_CONSISTENCY','INTEGRITY_DISSENT_PRESERVATION','INTEGRITY_HISTORICAL_TRACEABILITY','INTEGRITY_REVIEWABILITY','MATERIAL_COMPROMISE','RELIANCE_CONTAINMENT','RECOVERY_READINESS','RECOMMENDATION_APPROPRIATENESS','DECISION_CONTEMPORANEOUS_REASONABLENESS','DECISION_ACTUAL_CIRCUMSTANCES_ADEQUACY','DECISION_REASONABLY_OBTAINABLE_INFORMATION'));
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
    (dimension='INVESTIGATION_COVERAGE' AND result IN ('UNEXAMINED','PARTIALLY_EXAMINED','EXAMINED')) OR
    (dimension IN ('INTEGRITY_INVESTIGATIVE_INDEPENDENCE','INTEGRITY_EVIDENCE_SELECTION','INTEGRITY_PROVENANCE','INTEGRITY_QUESTION_FRAMING','INTEGRITY_HYPOTHESIS_COVERAGE','INTEGRITY_REASONING_QUALITY','INTEGRITY_CRITERIA_CONSISTENCY','INTEGRITY_DISSENT_PRESERVATION','INTEGRITY_HISTORICAL_TRACEABILITY','INTEGRITY_REVIEWABILITY','MATERIAL_COMPROMISE') AND result IN ('NO_CONCERN_INDICATED','CONCERN_INDICATED','COMPROMISE_INDICATED')) OR
    (dimension='RELIANCE_CONTAINMENT' AND result IN ('PAUSE_INDICATED','LIMIT_INDICATED','NO_PAUSE_INDICATED')) OR
    (dimension='RECOVERY_READINESS' AND result IN ('RECOVERY_READY','RECOVERY_PARTIAL','RECOVERY_NOT_READY')) OR
    (dimension IN ('RECOMMENDATION_APPROPRIATENESS','DECISION_CONTEMPORANEOUS_REASONABLENESS','DECISION_ACTUAL_CIRCUMSTANCES_ADEQUACY','DECISION_REASONABLY_OBTAINABLE_INFORMATION') AND result IN ('ADEQUATE','INADEQUATE','PARTIALLY_ADEQUATE')));
CREATE FUNCTION niccolo.validate_action_assessment_revision() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE prior niccolo.epistemic_assessment_events%ROWTYPE;
BEGIN
  IF NEW.revises_event_id IS NOT NULL THEN
    SELECT * INTO prior FROM niccolo.epistemic_assessment_events WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND event_id=NEW.revises_event_id;
    IF FOUND AND (NEW.target_recommendation_id IS NOT NULL OR NEW.target_decision_id IS NOT NULL OR prior.target_recommendation_id IS NOT NULL OR prior.target_decision_id IS NOT NULL) AND (
      NEW.target_recommendation_id IS DISTINCT FROM prior.target_recommendation_id OR NEW.target_decision_id IS DISTINCT FROM prior.target_decision_id OR NEW.dimension<>prior.dimension OR NEW.criterion<>prior.criterion OR NEW.context<>prior.context OR NEW.recorded_by_principal_id<>prior.recorded_by_principal_id
    ) THEN RAISE EXCEPTION 'Action assessment revision must preserve exact target, perspective and attribution' USING ERRCODE='23514'; END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER action_assessment_revision_validate BEFORE INSERT ON niccolo.epistemic_assessment_events FOR EACH ROW EXECUTE FUNCTION niccolo.validate_action_assessment_revision();
COMMIT;
