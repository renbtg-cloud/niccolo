-- Attributed, dimension-specific judgments, never a universal score or truth value.
BEGIN;
CREATE TABLE niccolo.epistemic_assessment_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  revises_event_id UUID,
  target_object_id UUID,
  target_evidence_id UUID,
  target_relation_id UUID,
  target_argument_id UUID,
  target_grounding_set_id UUID,
  dimension TEXT NOT NULL,
  result TEXT NOT NULL,
  confidence TEXT NOT NULL CHECK (confidence IN ('LOW','MODERATE','HIGH','UNKNOWN')),
  criterion TEXT NOT NULL CHECK (length(criterion) BETWEEN 1 AND 2048 AND btrim(criterion)<>''),
  context TEXT NOT NULL CHECK (length(context) BETWEEN 1 AND 2048 AND btrim(context)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,event_id),
  UNIQUE (workspace_id,situation_id,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,situation_id,revises_event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id),
  FOREIGN KEY (workspace_id,target_object_id) REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY (workspace_id,target_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY (workspace_id,situation_id,target_relation_id) REFERENCES niccolo.epistemic_relations(workspace_id,situation_id,relation_id),
  FOREIGN KEY (workspace_id,situation_id,target_argument_id) REFERENCES niccolo.epistemic_arguments(workspace_id,situation_id,argument_id),
  FOREIGN KEY (workspace_id,target_argument_id,target_grounding_set_id) REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id),
  CHECK (num_nonnulls(target_object_id,target_evidence_id,target_relation_id,target_argument_id)=1),
  CHECK ((target_argument_id IS NULL) = (target_grounding_set_id IS NULL)),
  CHECK (revises_event_id IS NULL OR revises_event_id<>event_id),
  CHECK (dimension IN ('CREDIBILITY','RELEVANCE','RELIABILITY','INDEPENDENCE','DISCRIMINATORY_POWER')),
  CHECK (result IN ('UNKNOWN','CONTESTED') OR
    (dimension='CREDIBILITY' AND result IN ('CREDIBLE','QUESTIONABLE','NOT_CREDIBLE')) OR
    (dimension='RELEVANCE' AND result IN ('RELEVANT','LIMITED_RELEVANCE','IRRELEVANT')) OR
    (dimension='RELIABILITY' AND result IN ('RELIABLE','LIMITED_RELIABILITY','UNRELIABLE')) OR
    (dimension='INDEPENDENCE' AND result IN ('INDEPENDENCE_SUPPORTED','DEPENDENCE_INDICATED','DEPENDENT')) OR
    (dimension='DISCRIMINATORY_POWER' AND result IN ('HIGH_POWER','LIMITED_POWER','NO_POWER')))
);
CREATE UNIQUE INDEX epistemic_assessment_single_revision ON niccolo.epistemic_assessment_events(workspace_id,revises_event_id) WHERE revises_event_id IS NOT NULL;
CREATE INDEX epistemic_assessment_situation_order ON niccolo.epistemic_assessment_events(workspace_id,situation_id,event_no);
CREATE TABLE niccolo.epistemic_assessment_basis (
  workspace_id UUID NOT NULL,
  event_id UUID NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 63),
  ground_object_id UUID,
  ground_evidence_id UUID,
  PRIMARY KEY (workspace_id,event_id,ordinal),
  FOREIGN KEY (workspace_id,event_id) REFERENCES niccolo.epistemic_assessment_events(workspace_id,event_id),
  FOREIGN KEY (workspace_id,ground_object_id) REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY (workspace_id,ground_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  CHECK (num_nonnulls(ground_object_id,ground_evidence_id)=1)
);
GRANT SELECT,INSERT ON niccolo.epistemic_assessment_events,niccolo.epistemic_assessment_basis TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_assessment_events_event_no_seq TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['epistemic_assessment_events','epistemic_assessment_basis'] LOOP
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
