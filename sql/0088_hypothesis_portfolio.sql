-- Durable investigation state layered onto model-generated candidate interpretations.
-- Hypotheses remain interpretations, not facts; links record how observations are used.
BEGIN;

CREATE TABLE niccolo.situation_analysis_hypothesis_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('CHALLENGED','WEAKENED','RETAINED','RETIRED','RESTORED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,interpretation_id,event_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id)
);

CREATE TABLE niccolo.situation_analysis_hypothesis_evidence (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  link_id UUID NOT NULL,
  evidence_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('SUPPORTS','CONTRADICTS','CONTEXT')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,interpretation_id,link_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id),
  FOREIGN KEY (workspace_id,evidence_id)
    REFERENCES niccolo.evidence(workspace_id,evidence_id)
);

CREATE TABLE niccolo.situation_analysis_hypothesis_relations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  relation_id UUID NOT NULL,
  left_interpretation_id UUID NOT NULL,
  right_interpretation_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('COMPETES_WITH','CAN_COEXIST','REFINES','OVERLAPS')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,relation_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,left_interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,right_interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id),
  CHECK (left_interpretation_id <> right_interpretation_id)
);

CREATE TABLE niccolo.situation_analysis_hypothesis_predictions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  prediction_id UUID NOT NULL,
  expected_observation TEXT NOT NULL CHECK (length(expected_observation) BETWEEN 1 AND 4096 AND btrim(expected_observation) <> ''),
  direction TEXT NOT NULL CHECK (direction IN ('SUPPORT','COUNTEREVIDENCE','DISCRIMINATE')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,interpretation_id,prediction_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id)
);

CREATE INDEX hypothesis_events_by_analysis
 ON niccolo.situation_analysis_hypothesis_events
 (workspace_id,situation_id,analysis_id,interpretation_id,created_at,event_id);
CREATE INDEX hypothesis_evidence_by_analysis
 ON niccolo.situation_analysis_hypothesis_evidence
 (workspace_id,situation_id,analysis_id,interpretation_id,evidence_id);
CREATE INDEX hypothesis_relations_by_analysis
 ON niccolo.situation_analysis_hypothesis_relations
 (workspace_id,situation_id,analysis_id,left_interpretation_id,right_interpretation_id);
CREATE INDEX hypothesis_predictions_by_analysis
 ON niccolo.situation_analysis_hypothesis_predictions
 (workspace_id,situation_id,analysis_id,interpretation_id,prediction_id);

GRANT SELECT,INSERT ON niccolo.situation_analysis_hypothesis_events,
 niccolo.situation_analysis_hypothesis_evidence,
 niccolo.situation_analysis_hypothesis_relations,
 niccolo.situation_analysis_hypothesis_predictions TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['situation_analysis_hypothesis_events',
                          'situation_analysis_hypothesis_evidence',
                          'situation_analysis_hypothesis_relations',
                          'situation_analysis_hypothesis_predictions'] LOOP
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
