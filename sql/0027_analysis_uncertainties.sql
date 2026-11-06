-- v0.41: explicit uncertainty register. Uncertainty records are observations about limits,
-- not hidden model state and not truth labels on people or evidence.
BEGIN;
CREATE TABLE niccolo.situation_analysis_uncertainties (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  uncertainty_id UUID NOT NULL,
  claim_id UUID,
  uncertainty_kind TEXT NOT NULL CHECK (uncertainty_kind IN ('MISSING_EVIDENCE','AMBIGUITY','CONFLICT','ASSUMPTION','UNKNOWN')),
  statement TEXT NOT NULL CHECK (length(statement) BETWEEN 1 AND 4096 AND btrim(statement) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,uncertainty_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
GRANT SELECT, INSERT ON niccolo.situation_analysis_uncertainties TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_uncertainties ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_uncertainties FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_uncertainties
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_uncertainties AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_uncertainties AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_uncertainties AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_uncertainties AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
