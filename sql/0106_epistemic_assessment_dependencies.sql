-- An assessment matters to a route only by explicit attributed reliance.
BEGIN;
CREATE TABLE niccolo.epistemic_assessment_dependencies (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  dependency_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  assessment_event_id UUID NOT NULL,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,dependency_id),
  UNIQUE (workspace_id,situation_id,dependency_id),
  FOREIGN KEY (workspace_id,situation_id,argument_id) REFERENCES niccolo.epistemic_arguments(workspace_id,situation_id,argument_id),
  FOREIGN KEY (workspace_id,argument_id,grounding_set_id) REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id),
  FOREIGN KEY (workspace_id,situation_id,assessment_event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id)
);
CREATE INDEX epistemic_assessment_dependencies_situation ON niccolo.epistemic_assessment_dependencies(workspace_id,situation_id,argument_id,grounding_set_id);
CREATE TABLE niccolo.epistemic_assessment_dependency_reviews (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  review_event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  dependency_id UUID NOT NULL,
  reviewed_assessment_event_id UUID NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('RETAINED','REVISION_REQUIRED','DEFERRED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,review_event_id),
  FOREIGN KEY (workspace_id,situation_id,dependency_id) REFERENCES niccolo.epistemic_assessment_dependencies(workspace_id,situation_id,dependency_id),
  FOREIGN KEY (workspace_id,situation_id,reviewed_assessment_event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id)
);
CREATE INDEX epistemic_assessment_dependency_reviews_situation ON niccolo.epistemic_assessment_dependency_reviews(workspace_id,situation_id,dependency_id,event_no);
GRANT SELECT,INSERT ON niccolo.epistemic_assessment_dependencies,niccolo.epistemic_assessment_dependency_reviews TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_assessment_dependency_reviews_event_no_seq TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['epistemic_assessment_dependencies','epistemic_assessment_dependency_reviews'] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
 END LOOP;
END $$;
COMMIT;
