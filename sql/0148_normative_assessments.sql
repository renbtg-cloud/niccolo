-- AT-20: attributed assessments of particular norms, not a permission system.
BEGIN;
CREATE TABLE niccolo.epistemic_normative_assessments (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL CHECK(event_id<>'00000000-0000-0000-0000-000000000000'),
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  subject_actor_id UUID NOT NULL,
  action_object_id UUID NOT NULL,
  norm_object_id UUID NOT NULL,
  issuer_actor_id UUID NOT NULL,
  competing_norm_object_id UUID,
  competing_issuer_actor_id UUID,
  dimension TEXT NOT NULL CHECK(dimension IN ('APPLICABILITY','AUTHORIZATION','OBLIGATION','PROHIBITION','COMPLIANCE','VIOLATION','PRECEDENCE','EXCEPTION')),
  judgment TEXT NOT NULL CHECK(judgment IN ('INDICATED','NOT_INDICATED','UNKNOWN','CONTESTED')),
  context TEXT NOT NULL CHECK(octet_length(context) BETWEEN 1 AND 2048 AND btrim(context)<>''),
  temporal_scope TEXT NOT NULL CHECK(octet_length(temporal_scope) BETWEEN 1 AND 2048 AND btrim(temporal_scope)<>''),
  rationale TEXT NOT NULL CHECK(octet_length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  basis_evidence_id UUID,
  revises_event_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,event_id),
  UNIQUE(workspace_id,situation_id,event_id),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY(workspace_id,subject_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY(workspace_id,issuer_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY(workspace_id,competing_issuer_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY(workspace_id,action_object_id) REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY(workspace_id,norm_object_id) REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY(workspace_id,competing_norm_object_id) REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY(workspace_id,basis_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY(workspace_id,situation_id,revises_event_id) REFERENCES niccolo.epistemic_normative_assessments(workspace_id,situation_id,event_id),
  CHECK(action_object_id<>norm_object_id),
  CHECK(competing_norm_object_id IS NULL OR (competing_norm_object_id<>norm_object_id AND competing_norm_object_id<>action_object_id)),
  CHECK((dimension IN ('PRECEDENCE','EXCEPTION'))=(competing_norm_object_id IS NOT NULL)),
  CHECK((competing_norm_object_id IS NULL)=(competing_issuer_actor_id IS NULL)),
  CHECK(revises_event_id IS NULL OR revises_event_id<>event_id)
);
CREATE UNIQUE INDEX normative_single_revision ON niccolo.epistemic_normative_assessments(workspace_id,revises_event_id) WHERE revises_event_id IS NOT NULL;
CREATE INDEX normative_situation_order ON niccolo.epistemic_normative_assessments(workspace_id,situation_id,event_no);
CREATE FUNCTION niccolo.validate_normative_assessment() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE pinned UUID;
BEGIN
  FOREACH pinned IN ARRAY ARRAY[NEW.action_object_id,NEW.norm_object_id,NEW.competing_norm_object_id] LOOP
    IF pinned IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM niccolo.epistemic_situation_propositions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND proposition_id=pinned
      UNION ALL SELECT 1 FROM niccolo.epistemic_hypotheses WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND hypothesis_id=pinned
      UNION ALL SELECT 1 FROM niccolo.epistemic_assumptions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND assumption_id=pinned
      UNION ALL SELECT 1 FROM niccolo.epistemic_findings WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND finding_id=pinned
    ) THEN RAISE EXCEPTION 'Normative pins must belong to this Situation' USING ERRCODE='23514'; END IF;
  END LOOP;
  IF NEW.basis_evidence_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.source_fragments f JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id WHERE f.workspace_id=NEW.workspace_id AND s.situation_id=NEW.situation_id AND f.evidence_id=NEW.basis_evidence_id) THEN
    RAISE EXCEPTION 'Normative basis must belong to this Situation' USING ERRCODE='23514';
  END IF;
  IF NEW.revises_event_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.epistemic_normative_assessments p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.event_id=NEW.revises_event_id AND p.subject_actor_id=NEW.subject_actor_id AND p.action_object_id=NEW.action_object_id AND p.norm_object_id=NEW.norm_object_id AND p.issuer_actor_id=NEW.issuer_actor_id AND p.competing_norm_object_id IS NOT DISTINCT FROM NEW.competing_norm_object_id AND p.competing_issuer_actor_id IS NOT DISTINCT FROM NEW.competing_issuer_actor_id AND p.dimension=NEW.dimension AND p.context=NEW.context AND p.temporal_scope=NEW.temporal_scope AND p.recorded_by_principal_id=NEW.recorded_by_principal_id) THEN
    RAISE EXCEPTION 'Revision must preserve normative pins, scope and attribution' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER normative_validate BEFORE INSERT ON niccolo.epistemic_normative_assessments FOR EACH ROW EXECUTE FUNCTION niccolo.validate_normative_assessment();
GRANT SELECT,INSERT ON niccolo.epistemic_normative_assessments TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_normative_assessments_event_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_normative_assessments ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_normative_assessments FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_normative_assessments USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_normative_assessments AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_normative_assessments AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_normative_assessments AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false);
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_normative_assessments AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false);
COMMIT;
