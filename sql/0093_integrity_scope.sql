-- Explicit integrity-scope assessment. Byte continuity is distinct from authenticity and completeness.
BEGIN;

CREATE TABLE niccolo.situation_analysis_source_integrity (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  document_id UUID NOT NULL,
  assessment_id UUID NOT NULL,
  integrity_scope TEXT NOT NULL CHECK (integrity_scope IN (
    'STORED_ORIGINAL_BYTES','FROM_DECLARED_COLLECTION_POINT','EXTERNAL_ARTIFACT'
  )),
  byte_continuity TEXT NOT NULL CHECK (byte_continuity IN ('VERIFIED','FAILED','UNKNOWN')),
  document_sha256 TEXT NOT NULL CHECK (document_sha256 ~ '^[a-f0-9]{64}$'),
  computed_sha256 TEXT CHECK (computed_sha256 IS NULL OR computed_sha256 ~ '^[a-f0-9]{64}$'),
  authenticity TEXT NOT NULL CHECK (authenticity IN (
    'NOT_ASSESSED','UNKNOWN','ATTESTED','DISPUTED'
  )),
  completeness TEXT NOT NULL CHECK (completeness IN (
    'UNKNOWN','DECLARED_COMPLETE','KNOWN_INCOMPLETE','VERIFIED_FOR_SCOPE'
  )),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  assertion_kind TEXT NOT NULL CHECK (assertion_kind IN (
    'DOCUMENTARY_OBSERVATION','HUMAN_ASSERTION','MACHINE_HYPOTHESIS'
  )),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,document_id,assessment_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id)
);

CREATE INDEX source_integrity_by_analysis
 ON niccolo.situation_analysis_source_integrity
 (workspace_id,situation_id,analysis_id,document_id,created_at,assessment_id);

GRANT SELECT,INSERT ON niccolo.situation_analysis_source_integrity TO niccolo_app;

ALTER TABLE niccolo.situation_analysis_source_integrity ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_source_integrity FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_source_integrity
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_source_integrity AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_source_integrity AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_source_integrity AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_source_integrity AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
