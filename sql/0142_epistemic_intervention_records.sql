-- Independent descriptions and causal assessments; never authorization or execution.
BEGIN;
CREATE TABLE niccolo.epistemic_intervention_records (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  record_id UUID NOT NULL CHECK(record_id<>'00000000-0000-0000-0000-000000000000'),
  record_no BIGINT GENERATED ALWAYS AS IDENTITY,
  kind TEXT NOT NULL CHECK(kind IN ('AUTHORIZATION_REPORT','EXISTING_PLAN_REPORT','EXECUTION_OCCURRENCE','EVENT_OBSERVATION')),
  linked_decision_id UUID,
  characterization TEXT NOT NULL CHECK(characterization IN ('REPORTED','OBSERVED','INFERRED','CONTESTED')),
  description TEXT NOT NULL CHECK(octet_length(description) BETWEEN 1 AND 4096 AND btrim(description)<>''),
  event_temporal_scope TEXT NOT NULL CHECK(octet_length(event_temporal_scope) BETWEEN 1 AND 2048 AND btrim(event_temporal_scope)<>''),
  acquisition_temporal_scope TEXT NOT NULL CHECK(octet_length(acquisition_temporal_scope) BETWEEN 1 AND 2048 AND btrim(acquisition_temporal_scope)<>''),
  basis_evidence_id UUID,
  revises_record_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,record_id),
  UNIQUE(workspace_id,situation_id,record_id),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY(workspace_id,situation_id,linked_decision_id) REFERENCES niccolo.epistemic_action_decisions(workspace_id,situation_id,decision_id),
  FOREIGN KEY(workspace_id,basis_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY(workspace_id,situation_id,revises_record_id) REFERENCES niccolo.epistemic_intervention_records(workspace_id,situation_id,record_id),
  CHECK(characterization<>'OBSERVED' OR basis_evidence_id IS NOT NULL),
  CHECK(revises_record_id IS NULL OR revises_record_id<>record_id)
);
CREATE UNIQUE INDEX intervention_single_revision ON niccolo.epistemic_intervention_records(workspace_id,revises_record_id) WHERE revises_record_id IS NOT NULL;
CREATE INDEX intervention_situation_order ON niccolo.epistemic_intervention_records(workspace_id,situation_id,record_no);
CREATE TABLE niccolo.epistemic_intervention_causal_assessments (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  assessment_id UUID NOT NULL CHECK(assessment_id<>'00000000-0000-0000-0000-000000000000'),
  assessment_no BIGINT GENERATED ALWAYS AS IDENTITY,
  execution_record_id UUID NOT NULL,
  event_observation_record_id UUID NOT NULL,
  assessment TEXT NOT NULL CHECK(assessment IN ('PREDATED_INTERVENTION','POSSIBLY_INDUCED','POSSIBLY_INTENSIFIED','POSSIBLY_INDEPENDENT','UNDETERMINED')),
  rationale TEXT NOT NULL CHECK(octet_length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  alternative_explanations TEXT NOT NULL CHECK(octet_length(alternative_explanations) BETWEEN 1 AND 4096 AND btrim(alternative_explanations)<>''),
  uncertainty TEXT NOT NULL CHECK(octet_length(uncertainty) BETWEEN 1 AND 4096 AND btrim(uncertainty)<>''),
  basis_evidence_id UUID,
  revises_assessment_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,assessment_id),
  UNIQUE(workspace_id,situation_id,assessment_id),
  FOREIGN KEY(workspace_id,situation_id,execution_record_id) REFERENCES niccolo.epistemic_intervention_records(workspace_id,situation_id,record_id),
  FOREIGN KEY(workspace_id,situation_id,event_observation_record_id) REFERENCES niccolo.epistemic_intervention_records(workspace_id,situation_id,record_id),
  FOREIGN KEY(workspace_id,basis_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY(workspace_id,situation_id,revises_assessment_id) REFERENCES niccolo.epistemic_intervention_causal_assessments(workspace_id,situation_id,assessment_id),
  CHECK(execution_record_id<>event_observation_record_id),
  CHECK(revises_assessment_id IS NULL OR revises_assessment_id<>assessment_id)
);
CREATE UNIQUE INDEX intervention_causal_single_revision ON niccolo.epistemic_intervention_causal_assessments(workspace_id,revises_assessment_id) WHERE revises_assessment_id IS NOT NULL;
CREATE INDEX intervention_causal_situation_order ON niccolo.epistemic_intervention_causal_assessments(workspace_id,situation_id,assessment_no);
CREATE FUNCTION niccolo.validate_intervention_record() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NEW.basis_evidence_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.source_fragments f JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id WHERE f.workspace_id=NEW.workspace_id AND s.situation_id=NEW.situation_id AND f.evidence_id=NEW.basis_evidence_id) THEN
    RAISE EXCEPTION 'Intervention basis must belong to this Situation' USING ERRCODE='23514';
  END IF;
  IF NEW.revises_record_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.epistemic_intervention_records p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.record_id=NEW.revises_record_id AND p.kind=NEW.kind AND p.linked_decision_id IS NOT DISTINCT FROM NEW.linked_decision_id AND p.recorded_by_principal_id=NEW.recorded_by_principal_id) THEN
    RAISE EXCEPTION 'Revision must preserve stage, attribution and decision pin' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER intervention_validate BEFORE INSERT ON niccolo.epistemic_intervention_records FOR EACH ROW EXECUTE FUNCTION niccolo.validate_intervention_record();
CREATE FUNCTION niccolo.validate_intervention_causal_assessment() RETURNS trigger LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM niccolo.epistemic_intervention_records WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND record_id=NEW.execution_record_id AND kind='EXECUTION_OCCURRENCE') OR NOT EXISTS (SELECT 1 FROM niccolo.epistemic_intervention_records WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND record_id=NEW.event_observation_record_id AND kind='EVENT_OBSERVATION') THEN
    RAISE EXCEPTION 'Causal assessment requires exact execution and event records' USING ERRCODE='23514';
  END IF;
  IF NEW.basis_evidence_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.source_fragments f JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id WHERE f.workspace_id=NEW.workspace_id AND s.situation_id=NEW.situation_id AND f.evidence_id=NEW.basis_evidence_id) THEN
    RAISE EXCEPTION 'Causal assessment basis must belong to this Situation' USING ERRCODE='23514';
  END IF;
  IF NEW.revises_assessment_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM niccolo.epistemic_intervention_causal_assessments p WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id AND p.assessment_id=NEW.revises_assessment_id AND p.execution_record_id=NEW.execution_record_id AND p.event_observation_record_id=NEW.event_observation_record_id AND p.recorded_by_principal_id=NEW.recorded_by_principal_id) THEN
    RAISE EXCEPTION 'Causal revision must preserve both record pins and attribution' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER intervention_causal_validate BEFORE INSERT ON niccolo.epistemic_intervention_causal_assessments FOR EACH ROW EXECUTE FUNCTION niccolo.validate_intervention_causal_assessment();
DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['epistemic_intervention_records','epistemic_intervention_causal_assessments'] LOOP
    EXECUTE format('GRANT SELECT,INSERT ON niccolo.%I TO niccolo_app',t);
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false)',t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false)',t);
  END LOOP;
END $$;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_intervention_records_record_no_seq,niccolo.epistemic_intervention_causal_assessments_assessment_no_seq TO niccolo_app;
COMMIT;
