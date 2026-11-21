-- Attributed historical descriptions of epistemic acts, not executable instructions.
BEGIN;
CREATE TABLE niccolo.actor_epistemic_act_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  subject_actor_id UUID NOT NULL,
  target_object_id UUID NOT NULL,
  act_kind TEXT NOT NULL CHECK (act_kind IN ('INVESTIGATING','IGNORING','DEEPENING','DEFERRING','AVOIDING_ACQUISITION','DELEGATING','RECONSIDERING','CHALLENGING','PRESERVING_AMBIGUITY')),
  characterization TEXT NOT NULL CHECK (characterization IN ('REPORTED','OBSERVED','INFERRED','CONTESTED')),
  delegate_actor_id UUID,
  context TEXT NOT NULL CHECK (length(context) BETWEEN 1 AND 2048 AND btrim(context)<>''),
  temporal_scope TEXT NOT NULL CHECK (length(temporal_scope) BETWEEN 1 AND 2048 AND btrim(temporal_scope)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  basis_evidence_id UUID,
  revises_event_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,event_id),
  UNIQUE(workspace_id,situation_id,event_id),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY(workspace_id,subject_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY(workspace_id,delegate_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY(workspace_id,target_object_id) REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY(workspace_id,basis_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY(workspace_id,situation_id,revises_event_id) REFERENCES niccolo.actor_epistemic_act_events(workspace_id,situation_id,event_id),
  CHECK(revises_event_id IS NULL OR revises_event_id<>event_id),
  CHECK(delegate_actor_id IS NULL OR (act_kind='DELEGATING' AND delegate_actor_id<>subject_actor_id)),
  CHECK(characterization<>'OBSERVED' OR basis_evidence_id IS NOT NULL)
);
CREATE UNIQUE INDEX actor_epistemic_act_single_revision ON niccolo.actor_epistemic_act_events(workspace_id,revises_event_id) WHERE revises_event_id IS NOT NULL;
CREATE INDEX actor_epistemic_act_situation_order ON niccolo.actor_epistemic_act_events(workspace_id,situation_id,event_no);
CREATE FUNCTION niccolo.validate_actor_epistemic_act() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM niccolo.epistemic_situation_propositions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND proposition_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_hypotheses WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND hypothesis_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_assumptions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND assumption_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_questions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND question_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_information_gaps WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND gap_id=NEW.target_object_id
    UNION ALL SELECT 1 FROM niccolo.epistemic_findings WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND finding_id=NEW.target_object_id
  ) THEN RAISE EXCEPTION 'Act target must belong to this Situation' USING ERRCODE='23514'; END IF;
  IF NEW.basis_evidence_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.source_fragments f JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id WHERE f.workspace_id=NEW.workspace_id AND s.situation_id=NEW.situation_id AND f.evidence_id=NEW.basis_evidence_id)
  THEN RAISE EXCEPTION 'Act basis must belong to this Situation' USING ERRCODE='23514'; END IF;
  IF NEW.revises_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.actor_epistemic_act_events p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.event_id=NEW.revises_event_id AND p.subject_actor_id=NEW.subject_actor_id AND p.target_object_id=NEW.target_object_id AND p.act_kind=NEW.act_kind AND p.context=NEW.context AND p.temporal_scope=NEW.temporal_scope AND p.delegate_actor_id IS NOT DISTINCT FROM NEW.delegate_actor_id AND p.recorded_by_principal_id=NEW.recorded_by_principal_id)
  THEN RAISE EXCEPTION 'Revision must preserve the attributed act identity' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER actor_epistemic_act_scope BEFORE INSERT ON niccolo.actor_epistemic_act_events FOR EACH ROW EXECUTE FUNCTION niccolo.validate_actor_epistemic_act();
GRANT SELECT,INSERT ON niccolo.actor_epistemic_act_events TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.actor_epistemic_act_events_event_no_seq TO niccolo_app;
ALTER TABLE niccolo.actor_epistemic_act_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.actor_epistemic_act_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.actor_epistemic_act_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.actor_epistemic_act_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.actor_epistemic_act_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.actor_epistemic_act_events AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.actor_epistemic_act_events AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
