-- A response links an actual utterance to an investigative question, never to its truth.
BEGIN;
ALTER TABLE niccolo.epistemic_questions ADD CONSTRAINT epistemic_question_situation_identity UNIQUE (workspace_id,situation_id,question_id);
ALTER TABLE niccolo.epistemic_human_utterances ADD CONSTRAINT epistemic_utterance_situation_identity UNIQUE (workspace_id,situation_id,utterance_id);
CREATE TABLE niccolo.epistemic_question_responses (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  question_id UUID NOT NULL,
  response_id UUID NOT NULL,
  utterance_id UUID NOT NULL,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,response_id),
  UNIQUE (workspace_id,question_id,utterance_id),
  FOREIGN KEY (workspace_id,situation_id,question_id) REFERENCES niccolo.epistemic_questions(workspace_id,situation_id,question_id),
  FOREIGN KEY (workspace_id,situation_id,utterance_id) REFERENCES niccolo.epistemic_human_utterances(workspace_id,situation_id,utterance_id)
);
CREATE INDEX epistemic_responses_situation ON niccolo.epistemic_question_responses(workspace_id,situation_id,created_at,response_id);
GRANT SELECT,INSERT ON niccolo.epistemic_question_responses TO niccolo_app;
ALTER TABLE niccolo.epistemic_question_responses ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_question_responses FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_question_responses
 USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_question_responses AS RESTRICTIVE FOR SELECT TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_question_responses AS RESTRICTIVE FOR INSERT TO niccolo_api
 WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_question_responses AS RESTRICTIVE FOR UPDATE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_question_responses AS RESTRICTIVE FOR DELETE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
