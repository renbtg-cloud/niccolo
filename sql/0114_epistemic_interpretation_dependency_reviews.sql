-- Review one directly relying or downstream route against one exact current meaning state.
BEGIN;
CREATE TABLE niccolo.epistemic_interpretation_dependency_reviews (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  review_event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  dependency_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  reviewed_revision_id UUID NOT NULL,
  reviewed_confirmation_event_id UUID,
  disposition TEXT NOT NULL CHECK (disposition IN ('RETAINED','REVISION_REQUIRED','DEFERRED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,review_event_id),
  FOREIGN KEY (workspace_id,situation_id,dependency_id) REFERENCES niccolo.epistemic_interpretation_dependencies(workspace_id,situation_id,dependency_id),
  FOREIGN KEY (workspace_id,situation_id,argument_id) REFERENCES niccolo.epistemic_arguments(workspace_id,situation_id,argument_id),
  FOREIGN KEY (workspace_id,argument_id,grounding_set_id) REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id),
  FOREIGN KEY (workspace_id,interpretation_id,reviewed_revision_id) REFERENCES niccolo.epistemic_interpretation_revisions(workspace_id,interpretation_id,revision_id),
  FOREIGN KEY (workspace_id,interpretation_id,reviewed_revision_id,reviewed_confirmation_event_id) REFERENCES niccolo.epistemic_interpretation_confirmation_events(workspace_id,interpretation_id,revision_id,event_id)
);
CREATE INDEX epistemic_interpretation_dependency_reviews_scope ON niccolo.epistemic_interpretation_dependency_reviews(workspace_id,situation_id,dependency_id,event_no);
GRANT SELECT,INSERT ON niccolo.epistemic_interpretation_dependency_reviews TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_interpretation_dependency_reviews_event_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_interpretation_dependency_reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_interpretation_dependency_reviews FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_interpretation_dependency_reviews USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_interpretation_dependency_reviews AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_interpretation_dependency_reviews AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_interpretation_dependency_reviews AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_interpretation_dependency_reviews AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE FUNCTION niccolo.validate_epistemic_interpretation_review() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM niccolo.epistemic_interpretation_dependencies d WHERE d.workspace_id=NEW.workspace_id AND d.situation_id=NEW.situation_id AND d.dependency_id=NEW.dependency_id AND d.interpretation_id=NEW.interpretation_id) THEN
    RAISE EXCEPTION 'Review must refer to the pinned interpretation identity' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER epistemic_interpretation_review_identity BEFORE INSERT ON niccolo.epistemic_interpretation_dependency_reviews FOR EACH ROW EXECUTE FUNCTION niccolo.validate_epistemic_interpretation_review();
COMMIT;
