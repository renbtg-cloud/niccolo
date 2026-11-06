-- v0.35: recorded use of a human clarification answer in a later analysis claim.
BEGIN;
ALTER TABLE niccolo.situation_analysis_clarification_answers
  ADD CONSTRAINT clarification_answer_with_contribution_unique
  UNIQUE (workspace_id,situation_id,analysis_id,question_id,answer_id,contribution_id);

CREATE TABLE niccolo.situation_analysis_clarification_claim_impacts (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  impact_id UUID NOT NULL,
  source_analysis_id UUID NOT NULL,
  question_id UUID NOT NULL,
  answer_id UUID NOT NULL,
  contribution_id UUID NOT NULL,
  target_analysis_id UUID NOT NULL,
  target_claim_id UUID NOT NULL,
  recorded_effect TEXT NOT NULL CHECK (recorded_effect IN ('INFORMS','QUALIFIES','RESOLVES_FOR_THIS_ANALYSIS','INTRODUCES_UNCERTAINTY')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,impact_id),
  FOREIGN KEY (workspace_id,situation_id,source_analysis_id,question_id,answer_id,contribution_id)
    REFERENCES niccolo.situation_analysis_clarification_answers(workspace_id,situation_id,analysis_id,question_id,answer_id,contribution_id),
  FOREIGN KEY (workspace_id,situation_id,target_analysis_id,target_claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,target_analysis_id,contribution_id)
    REFERENCES niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,contribution_id),
  CHECK (source_analysis_id <> target_analysis_id)
);

GRANT SELECT, INSERT ON niccolo.situation_analysis_clarification_claim_impacts TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['situation_analysis_clarification_claim_impacts'] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)', t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))', t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
  END LOOP;
END $$;
COMMIT;
