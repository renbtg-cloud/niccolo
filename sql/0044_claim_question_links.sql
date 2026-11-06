-- v0.86: open/clarifying questions linked explicitly to structured claims.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_question_links (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  question_id UUID NOT NULL,
  relation_kind TEXT NOT NULL CHECK (relation_kind IN ('OPEN_QUESTION','WOULD_CLARIFY','CHALLENGES')),
  linked_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,question_id,relation_kind),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,question_id)
    REFERENCES niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_claim_question_links TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_claim_question_links ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_claim_question_links FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_question_links USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_question_links AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_question_links AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
