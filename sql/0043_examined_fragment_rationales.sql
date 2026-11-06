-- v0.85: why a source fragment was examined; examination is not verification or truth.
BEGIN;
CREATE TABLE niccolo.situation_analysis_examined_fragment_rationales (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  evidence_id UUID NOT NULL,
  rationale_id UUID NOT NULL,
  reason_kind TEXT NOT NULL CHECK (reason_kind IN ('DIRECT_SUPPORT','CROSS_CHECK','CONTEXT','CONTRADICTION','OTHER')),
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 2048 AND btrim(reason) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,evidence_id,rationale_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,evidence_id)
    REFERENCES niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_examined_fragment_rationales TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_examined_fragment_rationales ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_examined_fragment_rationales FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_examined_fragment_rationales USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_examined_fragment_rationales AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_examined_fragment_rationales AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
