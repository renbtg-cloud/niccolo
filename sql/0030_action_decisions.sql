-- v0.44: append-only human decisions on proposals. ACCEPTED still means only accepted,
-- never that Niccolo executed the action.
BEGIN;
CREATE TABLE niccolo.situation_analysis_action_decisions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  proposal_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  decision_kind TEXT NOT NULL CHECK (decision_kind IN ('ACCEPTED','REJECTED','DEFERRED','MODIFIED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  decided_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,proposal_id,decision_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,proposal_id)
    REFERENCES niccolo.situation_analysis_action_proposals(workspace_id,situation_id,analysis_id,proposal_id)
);
GRANT SELECT, INSERT ON niccolo.situation_analysis_action_decisions TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_action_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_action_decisions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_action_decisions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_action_decisions AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_action_decisions AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_action_decisions AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_action_decisions AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
