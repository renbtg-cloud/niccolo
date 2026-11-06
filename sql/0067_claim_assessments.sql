-- v0.166-v0.170: purpose-bound support assessments; labels do not establish truth.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_assessments (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,analysis_id UUID NOT NULL,claim_id UUID NOT NULL,assessment_id UUID NOT NULL,
 purpose TEXT NOT NULL CHECK(length(purpose) BETWEEN 1 AND 4096 AND btrim(purpose)<>''),assessment TEXT NOT NULL CHECK(assessment IN ('SUFFICIENT_FOR_PURPOSE','INSUFFICIENT','CONTESTED','UNKNOWN')),
 basis_note TEXT NOT NULL CHECK(length(basis_note) BETWEEN 1 AND 4096 AND btrim(basis_note)<>''),recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,analysis_id,claim_id,assessment_id),FOREIGN KEY(workspace_id,situation_id,analysis_id,claim_id) REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_claim_assessments TO niccolo_app;ALTER TABLE niccolo.situation_analysis_claim_assessments ENABLE ROW LEVEL SECURITY;ALTER TABLE niccolo.situation_analysis_claim_assessments FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_assessments USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_assessments AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_assessments AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
