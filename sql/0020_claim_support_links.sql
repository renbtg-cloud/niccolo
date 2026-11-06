-- v0.32: explicit claim support/context links. A link records analysis use; it does not establish truth.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_fragments (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  evidence_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('SUPPORTS_AS_USED','CONTRADICTS_AS_USED','CONTEXT_ONLY')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,evidence_id)
    REFERENCES niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id)
);
CREATE TABLE niccolo.situation_analysis_claim_contributions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  contribution_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('INFORMS','QUALIFIES','CONFLICTS_WITH_REPORTED_CONTEXT')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,contribution_id,relation),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,contribution_id)
    REFERENCES niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,contribution_id)
);

GRANT SELECT, INSERT ON niccolo.situation_analysis_claim_fragments TO niccolo_app;
GRANT SELECT, INSERT ON niccolo.situation_analysis_claim_contributions TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['situation_analysis_claim_fragments','situation_analysis_claim_contributions'] LOOP
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
