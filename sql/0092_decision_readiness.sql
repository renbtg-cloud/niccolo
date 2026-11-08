-- Decision-readiness assessments and material reversal conditions.
-- Readiness is purpose-bound support for human decision-making, not omniscience or authority.
BEGIN;

CREATE TABLE niccolo.situation_analysis_decision_readiness (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  assessment_id UUID NOT NULL,
  subject_label TEXT NOT NULL CHECK (length(subject_label) BETWEEN 1 AND 512 AND btrim(subject_label) <> ''),
  readiness TEXT NOT NULL CHECK (readiness IN ('READY','NOT_READY','ESCALATE')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  unresolved_unknowns TEXT NOT NULL CHECK (length(unresolved_unknowns) BETWEEN 1 AND 4096 AND btrim(unresolved_unknowns) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,decision_id,assessment_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,decision_id)
    REFERENCES niccolo.situation_analysis_decision_points(workspace_id,situation_id,analysis_id,decision_id)
);

CREATE TABLE niccolo.situation_analysis_decision_reversal_conditions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  assessment_id UUID NOT NULL,
  condition_id UUID NOT NULL,
  condition_text TEXT NOT NULL CHECK (length(condition_text) BETWEEN 1 AND 4096 AND btrim(condition_text) <> ''),
  impact TEXT NOT NULL CHECK (impact IN ('MAY_REVERSE','REQUIRES_REVIEW','LOW_IMPACT')),
  acquisition_note TEXT CHECK (acquisition_note IS NULL OR (length(acquisition_note) BETWEEN 1 AND 4096 AND btrim(acquisition_note) <> '')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,decision_id,assessment_id,condition_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,decision_id,assessment_id)
    REFERENCES niccolo.situation_analysis_decision_readiness(workspace_id,situation_id,analysis_id,decision_id,assessment_id)
);

CREATE INDEX decision_readiness_by_analysis
 ON niccolo.situation_analysis_decision_readiness
 (workspace_id,situation_id,analysis_id,decision_id,created_at,assessment_id);
CREATE INDEX decision_reversal_by_analysis
 ON niccolo.situation_analysis_decision_reversal_conditions
 (workspace_id,situation_id,analysis_id,decision_id,assessment_id,impact,condition_id);

GRANT SELECT,INSERT ON niccolo.situation_analysis_decision_readiness,
 niccolo.situation_analysis_decision_reversal_conditions TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['situation_analysis_decision_readiness',
                          'situation_analysis_decision_reversal_conditions'] LOOP
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
