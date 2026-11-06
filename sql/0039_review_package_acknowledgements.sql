-- v0.77-v0.78: acknowledge an exact review-package hash without implying agreement or approval.
BEGIN;
CREATE TABLE niccolo.situation_review_package_acknowledgements (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  acknowledgement_id UUID NOT NULL,
  package_sha256 CHAR(64) NOT NULL CHECK (package_sha256 ~ '^[a-f0-9]{64}$'),
  acknowledgement_kind TEXT NOT NULL CHECK (acknowledgement_kind IN ('VIEWED','EXPORTED','DISCUSSED')),
  note TEXT CHECK (note IS NULL OR (length(note) <= 4096 AND btrim(note) <> '')),
  acknowledged_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,acknowledgement_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
GRANT SELECT, INSERT ON niccolo.situation_review_package_acknowledgements TO niccolo_app;
ALTER TABLE niccolo.situation_review_package_acknowledgements ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_review_package_acknowledgements FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_review_package_acknowledgements
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_review_package_acknowledgements AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_review_package_acknowledgements AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
