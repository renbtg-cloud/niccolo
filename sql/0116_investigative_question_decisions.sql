-- A human's recorded next-question decision is not execution or authorization.
BEGIN;
CREATE TABLE niccolo.epistemic_question_decisions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  decision_no BIGINT GENERATED ALWAYS AS IDENTITY,
  question_id UUID NOT NULL,
  considered_question_ids UUID[] NOT NULL,
  assessment_event_ids UUID[] NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('PURSUE_NEXT','DEFER','DO_NOT_PURSUE')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,decision_id),
  FOREIGN KEY (workspace_id,situation_id,question_id) REFERENCES niccolo.epistemic_questions(workspace_id,situation_id,question_id),
  CHECK (cardinality(considered_question_ids) BETWEEN 1 AND 3 AND array_ndims(considered_question_ids)=1 AND array_position(considered_question_ids,NULL) IS NULL AND question_id=ANY(considered_question_ids)),
  CHECK (cardinality(assessment_event_ids)<=64 AND (cardinality(assessment_event_ids)=0 OR array_ndims(assessment_event_ids)=1) AND array_position(assessment_event_ids,NULL) IS NULL)
);
CREATE INDEX epistemic_question_decisions_situation ON niccolo.epistemic_question_decisions(workspace_id,situation_id,decision_no);
CREATE FUNCTION niccolo.validate_epistemic_question_decision() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF (SELECT count(*) FROM niccolo.epistemic_questions q WHERE q.workspace_id=NEW.workspace_id AND q.situation_id=NEW.situation_id AND q.question_id=ANY(NEW.considered_question_ids)) <> cardinality(NEW.considered_question_ids)
  THEN RAISE EXCEPTION 'Considered questions must be distinct and in this Situation' USING ERRCODE='23514'; END IF;
  IF (SELECT count(*) FROM niccolo.epistemic_assessment_events a WHERE a.workspace_id=NEW.workspace_id AND a.situation_id=NEW.situation_id AND a.event_id=ANY(NEW.assessment_event_ids) AND a.target_object_id=ANY(NEW.considered_question_ids) AND a.dimension IN ('RELEVANCE','DISCRIMINATORY_POWER','ACQUISITION_COST','ACCESSIBILITY','CONTAMINATION_RISK','URGENCY','AUTHORIZATION_STATUS','INVESTIGATION_COVERAGE')) <> cardinality(NEW.assessment_event_ids)
  THEN RAISE EXCEPTION 'Decision basis must refer to distinct selection judgments on considered questions' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER epistemic_question_decision_basis BEFORE INSERT ON niccolo.epistemic_question_decisions
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_epistemic_question_decision();
GRANT SELECT,INSERT ON niccolo.epistemic_question_decisions TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_question_decisions_decision_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_question_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_question_decisions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_question_decisions
 USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_question_decisions AS RESTRICTIVE FOR SELECT TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_question_decisions AS RESTRICTIVE FOR INSERT TO niccolo_api
 WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_question_decisions AS RESTRICTIVE FOR UPDATE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_question_decisions AS RESTRICTIVE FOR DELETE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
