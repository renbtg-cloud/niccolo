-- v0.69: bounded reanalysis suggestions. A suggestion never starts an analysis.
BEGIN;
CREATE TABLE niccolo.situation_reanalysis_suggestions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  suggestion_id UUID NOT NULL,
  reason_kind TEXT NOT NULL CHECK (reason_kind IN ('UNCERTAINTY','CONTEXT_DELTA','CLAIM_REVIEW','CLARIFICATION','OTHER')),
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 4096 AND btrim(reason) <> ''),
  proposed_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,suggestion_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
GRANT SELECT, INSERT ON niccolo.situation_reanalysis_suggestions TO niccolo_app;
ALTER TABLE niccolo.situation_reanalysis_suggestions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_reanalysis_suggestions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_reanalysis_suggestions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_reanalysis_suggestions AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_reanalysis_suggestions AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
