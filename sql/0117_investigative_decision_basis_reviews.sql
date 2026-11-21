-- Reconsideration concerns an exact changed assessment basis, never execution.
BEGIN;
ALTER TABLE niccolo.epistemic_question_decisions ADD CONSTRAINT epistemic_question_decision_situation_identity UNIQUE (workspace_id,situation_id,decision_id);
CREATE TABLE niccolo.epistemic_question_decision_reviews (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  review_event_id UUID NOT NULL,
  review_no BIGINT GENERATED ALWAYS AS IDENTITY,
  decision_id UUID NOT NULL,
  original_assessment_event_id UUID NOT NULL,
  reviewed_assessment_event_id UUID NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('RETAINED','RECONSIDERATION_REQUIRED','DEFERRED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,review_event_id),
  FOREIGN KEY (workspace_id,situation_id,decision_id) REFERENCES niccolo.epistemic_question_decisions(workspace_id,situation_id,decision_id),
  FOREIGN KEY (workspace_id,situation_id,original_assessment_event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id),
  FOREIGN KEY (workspace_id,situation_id,reviewed_assessment_event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id),
  CHECK (original_assessment_event_id<>reviewed_assessment_event_id)
);
CREATE INDEX epistemic_question_decision_review_order ON niccolo.epistemic_question_decision_reviews(workspace_id,situation_id,decision_id,review_no);
CREATE FUNCTION niccolo.validate_question_decision_review() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM niccolo.epistemic_question_decisions d WHERE d.workspace_id=NEW.workspace_id AND d.situation_id=NEW.situation_id AND d.decision_id=NEW.decision_id AND NEW.original_assessment_event_id=ANY(d.assessment_event_ids)) THEN
    RAISE EXCEPTION 'Original judgment is not in the cited decision basis' USING ERRCODE='23514';
  END IF;
  IF NOT EXISTS (
    WITH RECURSIVE chain(id) AS (
      SELECT NEW.original_assessment_event_id
      UNION SELECT a.event_id FROM niccolo.epistemic_assessment_events a JOIN chain c ON a.revises_event_id=c.id
       WHERE a.workspace_id=NEW.workspace_id AND a.situation_id=NEW.situation_id
    ) SELECT 1 FROM chain WHERE id=NEW.reviewed_assessment_event_id
  ) OR EXISTS (SELECT 1 FROM niccolo.epistemic_assessment_events a WHERE a.workspace_id=NEW.workspace_id AND a.situation_id=NEW.situation_id AND a.revises_event_id=NEW.reviewed_assessment_event_id) THEN
    RAISE EXCEPTION 'Review must identify the current successor of the cited basis' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER question_decision_review_basis BEFORE INSERT ON niccolo.epistemic_question_decision_reviews
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_question_decision_review();
GRANT SELECT,INSERT ON niccolo.epistemic_question_decision_reviews TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_question_decision_reviews_review_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_question_decision_reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_question_decision_reviews FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_question_decision_reviews
 USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_question_decision_reviews AS RESTRICTIVE FOR SELECT TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_question_decision_reviews AS RESTRICTIVE FOR INSERT TO niccolo_api
 WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_question_decision_reviews AS RESTRICTIVE FOR UPDATE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_question_decision_reviews AS RESTRICTIVE FOR DELETE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
