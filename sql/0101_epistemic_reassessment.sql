-- Selective downstream reassessment for expert-authored epistemic structure.
-- A change to one ground marks dependent arguments for review; it does not automatically
-- falsify their conclusions or erase alternative grounding routes.
BEGIN;

CREATE TABLE niccolo.epistemic_reassessment_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  trigger_object_id UUID NOT NULL,
  replacement_object_id UUID,
  change_kind TEXT NOT NULL CHECK (change_kind IN ('REVISED','RETRACTED','SUPERSEDED','REINTERPRETED','MANUAL_REVIEW')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,event_id),
  UNIQUE (workspace_id,situation_id,event_id),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,trigger_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY (workspace_id,replacement_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  CHECK (replacement_object_id IS NULL OR replacement_object_id <> trigger_object_id),
  CHECK (
    (change_kind IN ('REVISED','SUPERSEDED','REINTERPRETED') AND replacement_object_id IS NOT NULL) OR
    (change_kind IN ('RETRACTED','MANUAL_REVIEW') AND replacement_object_id IS NULL)
  )
);
CREATE INDEX epistemic_reassessment_events_by_situation
  ON niccolo.epistemic_reassessment_events(workspace_id,situation_id,created_at,event_id);
CREATE INDEX epistemic_reassessment_events_by_trigger
  ON niccolo.epistemic_reassessment_events(workspace_id,trigger_object_id,created_at,event_id);

CREATE TABLE niccolo.epistemic_reassessment_impacts (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  conclusion_object_id UUID NOT NULL,
  minimum_depth SMALLINT NOT NULL CHECK (minimum_depth BETWEEN 1 AND 64),
  impact_scope TEXT NOT NULL CHECK (impact_scope IN ('DIRECT_GROUND','DOWNSTREAM_CONCLUSION')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,event_id,argument_id),
  FOREIGN KEY (workspace_id,situation_id,event_id)
    REFERENCES niccolo.epistemic_reassessment_events(workspace_id,situation_id,event_id),
  FOREIGN KEY (workspace_id,argument_id)
    REFERENCES niccolo.epistemic_arguments(workspace_id,argument_id),
  FOREIGN KEY (workspace_id,conclusion_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id)
);
CREATE INDEX epistemic_reassessment_impacts_by_argument
  ON niccolo.epistemic_reassessment_impacts(workspace_id,argument_id,created_at,event_id);
CREATE INDEX epistemic_reassessment_impacts_by_conclusion
  ON niccolo.epistemic_reassessment_impacts(workspace_id,conclusion_object_id,created_at,event_id);

GRANT SELECT,INSERT ON
  niccolo.epistemic_reassessment_events,
  niccolo.epistemic_reassessment_impacts
TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY[
  'epistemic_reassessment_events',
  'epistemic_reassessment_impacts'
 ] LOOP
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
