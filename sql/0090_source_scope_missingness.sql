-- Explicit collection-scope and missingness assertions for one frozen analysis.
-- Absence, non-collection and unavailability remain distinct states.
BEGIN;

CREATE TABLE niccolo.situation_analysis_source_scope (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  scope_id UUID NOT NULL,
  source_family TEXT NOT NULL CHECK (length(source_family) BETWEEN 1 AND 512 AND btrim(source_family) <> ''),
  collection_status TEXT NOT NULL CHECK (collection_status IN (
    'EXPECTED_NOT_REQUESTED','REQUESTED_PENDING','COLLECTED',
    'KNOWN_UNAVAILABLE','DECLARED_OUT_OF_SCOPE','UNKNOWN_EXISTENCE'
  )),
  scope_description TEXT NOT NULL CHECK (length(scope_description) BETWEEN 1 AND 4096 AND btrim(scope_description) <> ''),
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 4096 AND btrim(reason) <> ''),
  collector_label TEXT CHECK (collector_label IS NULL OR (length(collector_label) BETWEEN 1 AND 512 AND btrim(collector_label) <> '')),
  date_scope TEXT CHECK (date_scope IS NULL OR (length(date_scope) BETWEEN 1 AND 1024 AND btrim(date_scope) <> '')),
  assertion_kind TEXT NOT NULL CHECK (assertion_kind IN (
    'DOCUMENTARY_OBSERVATION','HUMAN_ASSERTION','MACHINE_HYPOTHESIS'
  )),
  document_id UUID,
  supersedes_scope_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,scope_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,supersedes_scope_id)
    REFERENCES niccolo.situation_analysis_source_scope(workspace_id,situation_id,analysis_id,scope_id),
  CHECK (supersedes_scope_id IS NULL OR supersedes_scope_id <> scope_id)
);

CREATE INDEX source_scope_by_analysis
 ON niccolo.situation_analysis_source_scope
 (workspace_id,situation_id,analysis_id,source_family,created_at,scope_id);

GRANT SELECT,INSERT ON niccolo.situation_analysis_source_scope TO niccolo_app;

ALTER TABLE niccolo.situation_analysis_source_scope ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_source_scope FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_source_scope
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_source_scope AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_source_scope AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_source_scope AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_source_scope AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
