-- Durable audit trail for explicitly authorized sensitive presentation exports.
-- Export events record scope and actor, never exported source text or original bytes.
BEGIN;

CREATE TABLE niccolo.analysis_export_events (
  workspace_id UUID NOT NULL,
  export_event_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  export_kind TEXT NOT NULL CHECK (export_kind='EXAMINED_SOURCE_PRESENTATION'),
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  examined_fragment_count BIGINT NOT NULL CHECK (examined_fragment_count >= 0),
  examined_source_text_bytes BIGINT NOT NULL CHECK (examined_source_text_bytes >= 0),
  includes_original_uploads BOOLEAN NOT NULL DEFAULT FALSE CHECK (includes_original_uploads=FALSE),
  establishes_truth BOOLEAN NOT NULL DEFAULT FALSE CHECK (establishes_truth=FALSE),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,export_event_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);

CREATE INDEX analysis_export_events_by_analysis
  ON niccolo.analysis_export_events(workspace_id,situation_id,analysis_id,created_at DESC,export_event_id);

GRANT SELECT,INSERT ON niccolo.analysis_export_events TO niccolo_app;

ALTER TABLE niccolo.analysis_export_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.analysis_export_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.analysis_export_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.analysis_export_events AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.analysis_export_events AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.analysis_export_events AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.analysis_export_events AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
