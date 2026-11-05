-- v0.24: clarification questions tied to the frozen analysis input that motivated them.
BEGIN;
CREATE TABLE niccolo.situation_analysis_clarification_questions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  question_id UUID NOT NULL,
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 2048 AND btrim(reason) <> ''),
  proposed_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, analysis_id, question_id),
  FOREIGN KEY (workspace_id, situation_id, analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id, situation_id, analysis_id),
  FOREIGN KEY (workspace_id, question_id)
    REFERENCES niccolo.investigative_questions(workspace_id, question_id)
);
CREATE INDEX situation_analysis_clarifications_by_question
  ON niccolo.situation_analysis_clarification_questions(workspace_id, question_id);

GRANT SELECT, INSERT ON niccolo.situation_analysis_clarification_questions TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_clarification_questions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_clarification_questions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_clarification_questions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_clarification_questions AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_clarification_questions AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_clarification_questions AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_clarification_questions AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
