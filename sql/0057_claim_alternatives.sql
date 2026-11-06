-- v0.115-v0.116: explicit alternative interpretations; no automatic ranking implied.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_alternatives (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  alternative_id UUID NOT NULL,
  alternative_text TEXT NOT NULL CHECK (length(alternative_text) BETWEEN 1 AND 4096 AND btrim(alternative_text) <> ''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,claim_id,alternative_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_claim_alternatives TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_claim_alternatives ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_claim_alternatives FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_alternatives USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_alternatives AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_alternatives AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
