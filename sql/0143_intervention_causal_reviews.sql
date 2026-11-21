-- Review a changed causal basis without modifying the historical assessment.
BEGIN;
CREATE TABLE niccolo.epistemic_intervention_causal_reviews (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  review_id UUID NOT NULL CHECK(review_id<>'00000000-0000-0000-0000-000000000000'),
  review_no BIGINT GENERATED ALWAYS AS IDENTITY,
  assessment_id UUID NOT NULL,
  reviewed_execution_record_id UUID NOT NULL,
  reviewed_event_record_id UUID NOT NULL,
  disposition TEXT NOT NULL CHECK(disposition IN ('RETAINED','RECONSIDERATION_REQUIRED','DEFERRED')),
  note TEXT NOT NULL CHECK(octet_length(note) BETWEEN 1 AND 4096 AND btrim(note,U&'\0009\000A\000B\000C\000D\0020\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000')<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,review_id),
  FOREIGN KEY(workspace_id,situation_id,assessment_id) REFERENCES niccolo.epistemic_intervention_causal_assessments(workspace_id,situation_id,assessment_id),
  FOREIGN KEY(workspace_id,situation_id,reviewed_execution_record_id) REFERENCES niccolo.epistemic_intervention_records(workspace_id,situation_id,record_id),
  FOREIGN KEY(workspace_id,situation_id,reviewed_event_record_id) REFERENCES niccolo.epistemic_intervention_records(workspace_id,situation_id,record_id)
);
CREATE INDEX causal_reviews_situation_order ON niccolo.epistemic_intervention_causal_reviews(workspace_id,situation_id,review_no);
CREATE FUNCTION niccolo.validate_intervention_causal_review() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE original_execution UUID; original_event UUID; current_execution UUID; current_event UUID;
BEGIN
  SELECT execution_record_id,event_observation_record_id INTO original_execution,original_event FROM niccolo.epistemic_intervention_causal_assessments WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND assessment_id=NEW.assessment_id;
  IF NOT FOUND THEN RAISE EXCEPTION 'Unknown causal assessment' USING ERRCODE='23514'; END IF;
  WITH RECURSIVE chain(id) AS (SELECT original_execution UNION SELECT r.record_id FROM niccolo.epistemic_intervention_records r JOIN chain c ON r.revises_record_id=c.id WHERE r.workspace_id=NEW.workspace_id AND r.situation_id=NEW.situation_id)
    SELECT id INTO current_execution FROM chain WHERE NOT EXISTS (SELECT 1 FROM niccolo.epistemic_intervention_records r WHERE r.workspace_id=NEW.workspace_id AND r.situation_id=NEW.situation_id AND r.revises_record_id=chain.id);
  WITH RECURSIVE chain(id) AS (SELECT original_event UNION SELECT r.record_id FROM niccolo.epistemic_intervention_records r JOIN chain c ON r.revises_record_id=c.id WHERE r.workspace_id=NEW.workspace_id AND r.situation_id=NEW.situation_id)
    SELECT id INTO current_event FROM chain WHERE NOT EXISTS (SELECT 1 FROM niccolo.epistemic_intervention_records r WHERE r.workspace_id=NEW.workspace_id AND r.situation_id=NEW.situation_id AND r.revises_record_id=chain.id);
  IF (current_execution=original_execution AND current_event=original_event) OR NEW.reviewed_execution_record_id IS DISTINCT FROM current_execution OR NEW.reviewed_event_record_id IS DISTINCT FROM current_event THEN
    RAISE EXCEPTION 'Review must identify both exact current basis records after a change' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER intervention_causal_review_validate BEFORE INSERT ON niccolo.epistemic_intervention_causal_reviews FOR EACH ROW EXECUTE FUNCTION niccolo.validate_intervention_causal_review();
GRANT SELECT,INSERT ON niccolo.epistemic_intervention_causal_reviews TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_intervention_causal_reviews_review_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_intervention_causal_reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_intervention_causal_reviews FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_intervention_causal_reviews USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_intervention_causal_reviews AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_intervention_causal_reviews AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_intervention_causal_reviews AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false);
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_intervention_causal_reviews AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false);
COMMIT;
