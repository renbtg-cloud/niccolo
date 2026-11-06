-- v0.112: assumptions are explicit analysis records, not hidden facts.
BEGIN;
CREATE TABLE niccolo.situation_analysis_assumptions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  assumption_id UUID NOT NULL,
  claim_id UUID,
  statement TEXT NOT NULL CHECK (length(statement) BETWEEN 1 AND 4096 AND btrim(statement) <> ''),
  basis_note TEXT NOT NULL CHECK (length(basis_note) BETWEEN 1 AND 4096 AND btrim(basis_note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,assumption_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id) REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,claim_id) REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_assumptions TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_assumptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_assumptions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_assumptions USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_assumptions AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_assumptions AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
