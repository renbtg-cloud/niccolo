-- v0.276-v0.290: auditable model invocation ledger bound to an agentic operation/step.
-- The ledger records structured request/response envelopes and hashes; it does not assert that model output is true.
BEGIN;
CREATE TABLE niccolo.model_invocations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  plan_revision BIGINT NOT NULL,
  step_id UUID NOT NULL,
  invocation_id UUID NOT NULL,
  provider TEXT NOT NULL CHECK (length(provider) BETWEEN 1 AND 128 AND btrim(provider) <> ''),
  model TEXT NOT NULL CHECK (length(model) BETWEEN 1 AND 256 AND btrim(model) <> ''),
  request_json TEXT NOT NULL CHECK (length(request_json) BETWEEN 2 AND 1048576 AND btrim(request_json) <> ''),
  request_sha256 CHAR(64) NOT NULL CHECK (request_sha256 ~ '^[a-f0-9]{64}$'),
  response_json TEXT,
  response_sha256 CHAR(64),
  status TEXT NOT NULL CHECK (status IN ('STARTED','SUCCEEDED','FAILED')),
  error_code TEXT,
  error_message TEXT,
  prompt_tokens BIGINT CHECK (prompt_tokens IS NULL OR prompt_tokens >= 0),
  completion_tokens BIGINT CHECK (completion_tokens IS NULL OR completion_tokens >= 0),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  completed_at TIMESTAMPTZ,
  PRIMARY KEY(workspace_id,situation_id,operation_id,plan_revision,step_id,invocation_id),
  FOREIGN KEY(workspace_id,situation_id,operation_id,plan_revision,step_id)
    REFERENCES niccolo.agentic_steps(workspace_id,situation_id,operation_id,plan_revision,step_id),
  CHECK ((status='STARTED' AND response_json IS NULL AND response_sha256 IS NULL AND error_code IS NULL AND error_message IS NULL AND completed_at IS NULL)
      OR (status='SUCCEEDED' AND response_json IS NOT NULL AND response_sha256 IS NOT NULL AND error_code IS NULL AND error_message IS NULL AND completed_at IS NOT NULL)
      OR (status='FAILED' AND response_json IS NULL AND response_sha256 IS NULL AND error_code IS NOT NULL AND error_message IS NOT NULL AND completed_at IS NOT NULL))
);
CREATE INDEX model_invocations_by_operation
  ON niccolo.model_invocations(workspace_id,situation_id,operation_id,plan_revision,step_id,created_at,invocation_id);
GRANT SELECT, INSERT ON niccolo.model_invocations TO niccolo_app;
GRANT UPDATE (response_json,response_sha256,status,error_code,error_message,prompt_tokens,completion_tokens,completed_at)
  ON niccolo.model_invocations TO niccolo_app;
ALTER TABLE niccolo.model_invocations ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.model_invocations FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.model_invocations
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.model_invocations AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.model_invocations AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.model_invocations AS RESTRICTIVE FOR UPDATE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.model_invocations AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
