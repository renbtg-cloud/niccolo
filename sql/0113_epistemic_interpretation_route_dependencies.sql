-- Explicit reliance on a pinned utterance-interpretation revision and meaning confirmation context.
BEGIN;
CREATE TABLE niccolo.epistemic_interpretation_dependencies (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  dependency_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  revision_id UUID NOT NULL,
  confirmation_event_id UUID,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,dependency_id),
  UNIQUE (workspace_id,situation_id,dependency_id),
  FOREIGN KEY (workspace_id,situation_id,argument_id) REFERENCES niccolo.epistemic_arguments(workspace_id,situation_id,argument_id),
  FOREIGN KEY (workspace_id,argument_id,grounding_set_id) REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id),
  FOREIGN KEY (workspace_id,interpretation_id,revision_id) REFERENCES niccolo.epistemic_interpretation_revisions(workspace_id,interpretation_id,revision_id),
  FOREIGN KEY (workspace_id,interpretation_id,revision_id,confirmation_event_id) REFERENCES niccolo.epistemic_interpretation_confirmation_events(workspace_id,interpretation_id,revision_id,event_id)
);
CREATE INDEX epistemic_interpretation_dependencies_scope ON niccolo.epistemic_interpretation_dependencies(workspace_id,situation_id,argument_id,grounding_set_id);
GRANT SELECT,INSERT ON niccolo.epistemic_interpretation_dependencies TO niccolo_app;
ALTER TABLE niccolo.epistemic_interpretation_dependencies ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_interpretation_dependencies FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_interpretation_dependencies USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_interpretation_dependencies AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_interpretation_dependencies AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_interpretation_dependencies AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_interpretation_dependencies AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE FUNCTION niccolo.validate_epistemic_interpretation_dependency() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM niccolo.epistemic_utterance_interpretations i JOIN niccolo.epistemic_question_responses q ON q.workspace_id=i.workspace_id AND q.utterance_id=i.utterance_id WHERE i.workspace_id=NEW.workspace_id AND i.interpretation_id=NEW.interpretation_id AND q.situation_id=NEW.situation_id) THEN
    RAISE EXCEPTION 'Interpretation dependency is outside this Situation' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER epistemic_interpretation_dependency_scope BEFORE INSERT ON niccolo.epistemic_interpretation_dependencies FOR EACH ROW EXECUTE FUNCTION niccolo.validate_epistemic_interpretation_dependency();
COMMIT;
