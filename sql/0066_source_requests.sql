-- v0.161-v0.165: source acquisition requests are proposals; acceptance never fetches anything.
BEGIN;
CREATE TABLE niccolo.situation_analysis_source_requests (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,analysis_id UUID NOT NULL,request_id UUID NOT NULL,
 request_text TEXT NOT NULL CHECK(length(request_text) BETWEEN 1 AND 4096 AND btrim(request_text)<>''),source_kind TEXT NOT NULL CHECK(length(source_kind) BETWEEN 1 AND 256 AND btrim(source_kind)<>''),
 rationale TEXT NOT NULL CHECK(length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),proposed_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,analysis_id,request_id),FOREIGN KEY(workspace_id,situation_id,analysis_id) REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE TABLE niccolo.situation_analysis_source_request_events (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,analysis_id UUID NOT NULL,request_id UUID NOT NULL,event_id UUID NOT NULL,
 event_kind TEXT NOT NULL CHECK(event_kind IN ('ACCEPTED','DECLINED','DEFERRED','REOPENED')),note TEXT NOT NULL CHECK(length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,analysis_id,request_id,event_id),FOREIGN KEY(workspace_id,situation_id,analysis_id,request_id) REFERENCES niccolo.situation_analysis_source_requests(workspace_id,situation_id,analysis_id,request_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_source_requests,niccolo.situation_analysis_source_request_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_source_requests ENABLE ROW LEVEL SECURITY;ALTER TABLE niccolo.situation_analysis_source_requests FORCE ROW LEVEL SECURITY;ALTER TABLE niccolo.situation_analysis_source_request_events ENABLE ROW LEVEL SECURITY;ALTER TABLE niccolo.situation_analysis_source_request_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_source_requests USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);CREATE POLICY workspace_filter ON niccolo.situation_analysis_source_request_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_source_requests AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_source_requests AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));CREATE POLICY api_read_gate ON niccolo.situation_analysis_source_request_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_source_request_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
