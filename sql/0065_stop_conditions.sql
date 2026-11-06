-- v0.156-v0.160: explicit stop/escalation gates; trigger blocks automation and requires human handling.
BEGIN;
CREATE TABLE niccolo.situation_analysis_stop_conditions (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,analysis_id UUID NOT NULL,condition_id UUID NOT NULL,
 condition_text TEXT NOT NULL CHECK(length(condition_text) BETWEEN 1 AND 4096 AND btrim(condition_text)<>''),
 escalation_note TEXT NOT NULL CHECK(length(escalation_note) BETWEEN 1 AND 4096 AND btrim(escalation_note)<>''),
 recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,analysis_id,condition_id),FOREIGN KEY(workspace_id,situation_id,analysis_id) REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE TABLE niccolo.situation_analysis_stop_condition_events (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,analysis_id UUID NOT NULL,condition_id UUID NOT NULL,event_id UUID NOT NULL,
 event_kind TEXT NOT NULL CHECK(event_kind IN ('TRIGGERED','CLEARED')),note TEXT NOT NULL CHECK(length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
 recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,analysis_id,condition_id,event_id),FOREIGN KEY(workspace_id,situation_id,analysis_id,condition_id) REFERENCES niccolo.situation_analysis_stop_conditions(workspace_id,situation_id,analysis_id,condition_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_stop_conditions,niccolo.situation_analysis_stop_condition_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_stop_conditions ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.situation_analysis_stop_conditions FORCE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_stop_condition_events ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.situation_analysis_stop_condition_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_stop_conditions USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY workspace_filter ON niccolo.situation_analysis_stop_condition_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_stop_conditions AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_stop_conditions AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_read_gate ON niccolo.situation_analysis_stop_condition_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_stop_condition_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
