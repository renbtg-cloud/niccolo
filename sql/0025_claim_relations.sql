-- v0.39: immutable explicit relations among claims from the same analysis.
-- A relation records the analysis structure; it does not establish truth.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_relations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  relation_id UUID NOT NULL,
  source_claim_id UUID NOT NULL,
  target_claim_id UUID NOT NULL,
  relation TEXT NOT NULL CHECK (relation IN ('SUPPORTS','QUALIFIES','TENSIONS_WITH','ALTERNATIVE_TO','DEPENDS_ON')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,relation_id),
  UNIQUE (workspace_id,situation_id,analysis_id,source_claim_id,target_claim_id,relation),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,source_claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,target_claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  CHECK (source_claim_id <> target_claim_id)
);
GRANT SELECT, INSERT ON niccolo.situation_analysis_claim_relations TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_claim_relations ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_claim_relations FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_relations
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_relations AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_relations AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_claim_relations AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_claim_relations AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
