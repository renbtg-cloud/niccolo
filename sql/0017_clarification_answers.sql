-- v0.25: human clarification answers become attributed Situation context, never retroactive evidence.
BEGIN;
CREATE TABLE niccolo.situation_analysis_clarification_answers (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  question_id UUID NOT NULL,
  answer_id UUID NOT NULL,
  contribution_id UUID NOT NULL,
  answered_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  answer_revision BIGINT NOT NULL CHECK (answer_revision >= 1),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,question_id,answer_id),
  UNIQUE (workspace_id,situation_id,contribution_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,question_id)
    REFERENCES niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id),
  FOREIGN KEY (workspace_id,situation_id,contribution_id)
    REFERENCES niccolo.situation_contributions(workspace_id,situation_id,contribution_id),
  FOREIGN KEY (workspace_id,situation_id,answer_revision)
    REFERENCES niccolo.situation_revisions(workspace_id,situation_id,revision)
);
CREATE INDEX clarification_answers_by_question
  ON niccolo.situation_analysis_clarification_answers(workspace_id,situation_id,analysis_id,question_id,answer_id);
GRANT SELECT, INSERT ON niccolo.situation_analysis_clarification_answers TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_clarification_answers ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_clarification_answers FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_clarification_answers
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_clarification_answers AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_clarification_answers AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_clarification_answers AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_clarification_answers AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
