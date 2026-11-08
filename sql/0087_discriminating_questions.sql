-- Structured question-discrimination metadata for investigation scheduling.
-- Scores are transparent scheduling heuristics, never truth probabilities.
BEGIN;

CREATE TABLE niccolo.situation_analysis_clarification_values (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  question_id UUID NOT NULL,
  decision_relevance SMALLINT CHECK (decision_relevance BETWEEN 0 AND 100),
  acquisition_cost SMALLINT CHECK (acquisition_cost BETWEEN 0 AND 100),
  cheaper_alternative TEXT CHECK (cheaper_alternative IS NULL OR (length(cheaper_alternative) BETWEEN 1 AND 2048 AND btrim(cheaper_alternative) <> '')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,question_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,question_id)
    REFERENCES niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id)
);

CREATE TABLE niccolo.situation_analysis_clarification_interpretations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  question_id UUID NOT NULL,
  interpretation_id UUID NOT NULL,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,question_id,interpretation_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,question_id)
    REFERENCES niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id)
);

CREATE TABLE niccolo.situation_analysis_clarification_outcomes (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  question_id UUID NOT NULL,
  outcome_id UUID NOT NULL,
  outcome_label TEXT NOT NULL CHECK (length(outcome_label) BETWEEN 1 AND 512 AND btrim(outcome_label) <> ''),
  effect_note TEXT NOT NULL CHECK (length(effect_note) BETWEEN 1 AND 2048 AND btrim(effect_note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,question_id,outcome_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,question_id)
    REFERENCES niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id)
);

CREATE INDEX clarification_values_by_analysis
 ON niccolo.situation_analysis_clarification_values
 (workspace_id,situation_id,analysis_id,question_id);

CREATE INDEX clarification_interpretations_by_analysis
 ON niccolo.situation_analysis_clarification_interpretations
 (workspace_id,situation_id,analysis_id,question_id,interpretation_id);

CREATE INDEX clarification_outcomes_by_analysis
 ON niccolo.situation_analysis_clarification_outcomes
 (workspace_id,situation_id,analysis_id,question_id,outcome_id);

GRANT SELECT,INSERT ON niccolo.situation_analysis_clarification_values,
 niccolo.situation_analysis_clarification_interpretations,
 niccolo.situation_analysis_clarification_outcomes TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['situation_analysis_clarification_values',
                          'situation_analysis_clarification_interpretations',
                          'situation_analysis_clarification_outcomes'] LOOP
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
