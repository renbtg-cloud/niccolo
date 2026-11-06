-- v0.42: append-only human/system-visible events about a recorded uncertainty.
-- No event deletes or silently rewrites the original uncertainty.
BEGIN;
CREATE TABLE niccolo.situation_analysis_uncertainty_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  uncertainty_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('CLARIFIED','REMAINS','OUT_OF_SCOPE','REQUIRES_SOURCE','REPHRASED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  target_analysis_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,uncertainty_id,event_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,uncertainty_id)
    REFERENCES niccolo.situation_analysis_uncertainties(workspace_id,situation_id,analysis_id,uncertainty_id),
  FOREIGN KEY (workspace_id,situation_id,target_analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  CHECK ((event_kind='CLARIFIED' AND target_analysis_id IS NOT NULL) OR event_kind<>'CLARIFIED')
);
GRANT SELECT, INSERT ON niccolo.situation_analysis_uncertainty_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_uncertainty_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_uncertainty_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_uncertainty_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_uncertainty_events AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_uncertainty_events AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_uncertainty_events AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_uncertainty_events AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
