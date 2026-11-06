-- v1.001-v1.100: durable human response at agentic boundaries.
BEGIN;
CREATE TABLE niccolo.agentic_human_responses(
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,operation_id UUID NOT NULL,plan_revision BIGINT NOT NULL,step_id UUID NOT NULL,
 response_id UUID NOT NULL,decision TEXT NOT NULL CHECK(decision IN('CONTINUE','STOP','REPLAN')),response_text TEXT NOT NULL CHECK(length(response_text) BETWEEN 1 AND 4096 AND btrim(response_text)<>''),
 actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,operation_id,plan_revision,step_id,response_id),
 FOREIGN KEY(workspace_id,situation_id,operation_id,plan_revision,step_id) REFERENCES niccolo.agentic_steps(workspace_id,situation_id,operation_id,plan_revision,step_id)
);
CREATE UNIQUE INDEX agentic_human_response_once ON niccolo.agentic_human_responses(workspace_id,situation_id,operation_id,plan_revision,step_id);
GRANT SELECT,INSERT ON niccolo.agentic_human_responses TO niccolo_app;
ALTER TABLE niccolo.agentic_human_responses ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.agentic_human_responses FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.agentic_human_responses USING(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.agentic_human_responses AS RESTRICTIVE FOR SELECT TO niccolo_api USING(niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.agentic_human_responses AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK(niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.agentic_human_responses AS RESTRICTIVE FOR UPDATE TO niccolo_api USING(niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK(niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.agentic_human_responses AS RESTRICTIVE FOR DELETE TO niccolo_api USING(niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
