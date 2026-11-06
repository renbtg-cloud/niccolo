-- v0.182-v0.200: durable BFF operation envelope shared by synchronous and asynchronous delivery.
-- The operation is the domain request. Sync/async/pull are interaction modes over the same record.
BEGIN;
CREATE TABLE niccolo.bff_operations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  operation_kind TEXT NOT NULL CHECK (operation_kind IN ('BUILD_BRIEFING','BUILD_HANDOFF','AGENTIC_ANALYSIS','REANALYZE')),
  target_analysis_id UUID,
  parent_operation_id UUID,
  idempotency_key TEXT CHECK (idempotency_key IS NULL OR (length(idempotency_key) BETWEEN 1 AND 256 AND btrim(idempotency_key) <> '')),
  input_json TEXT NOT NULL CHECK (length(input_json) BETWEEN 2 AND 262144 AND btrim(input_json) <> ''),
  input_sha256 CHAR(64) NOT NULL CHECK (input_sha256 ~ '^[a-f0-9]{64}$'),
  state TEXT NOT NULL DEFAULT 'QUEUED' CHECK (state IN ('QUEUED','RUNNING','WAITING_FOR_HUMAN','SUCCEEDED','FAILED','CANCELLED')),
  state_version BIGINT NOT NULL DEFAULT 1 CHECK (state_version >= 1),
  result_json TEXT,
  error_code TEXT,
  error_message TEXT,
  deadline_at TIMESTAMPTZ,
  step_budget INTEGER NOT NULL DEFAULT 32 CHECK (step_budget BETWEEN 1 AND 10000),
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,operation_id),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,situation_id,target_analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,situation_id,parent_operation_id)
    REFERENCES niccolo.bff_operations(workspace_id,situation_id,operation_id),
  CHECK ((state='SUCCEEDED' AND result_json IS NOT NULL AND error_code IS NULL AND error_message IS NULL)
      OR (state='FAILED' AND result_json IS NULL AND error_code IS NOT NULL AND error_message IS NOT NULL)
      OR (state NOT IN ('SUCCEEDED','FAILED') AND result_json IS NULL AND error_code IS NULL AND error_message IS NULL))
);
CREATE UNIQUE INDEX bff_operations_idempotency
  ON niccolo.bff_operations(workspace_id,situation_id,operation_kind,idempotency_key)
  WHERE idempotency_key IS NOT NULL;
CREATE INDEX bff_operations_status
  ON niccolo.bff_operations(workspace_id,situation_id,state,created_at,operation_id);

CREATE TABLE niccolo.bff_operation_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  event_seq BIGINT GENERATED ALWAYS AS IDENTITY,
  event_kind TEXT NOT NULL CHECK (length(event_kind) BETWEEN 1 AND 64 AND event_kind ~ '^[A-Z0-9_]+$'),
  event_json TEXT NOT NULL CHECK (length(event_json) BETWEEN 2 AND 262144 AND btrim(event_json) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,operation_id,event_seq),
  FOREIGN KEY (workspace_id,situation_id,operation_id)
    REFERENCES niccolo.bff_operations(workspace_id,situation_id,operation_id)
);
CREATE INDEX bff_operation_events_cursor
  ON niccolo.bff_operation_events(workspace_id,situation_id,operation_id,event_seq);

GRANT SELECT, INSERT ON niccolo.bff_operations, niccolo.bff_operation_events TO niccolo_app;
GRANT UPDATE (state,state_version,result_json,error_code,error_message,updated_at) ON niccolo.bff_operations TO niccolo_app;
GRANT USAGE, SELECT ON SEQUENCE niccolo.bff_operation_events_event_seq_seq TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['bff_operations','bff_operation_events'] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)', t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))', t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
  END LOOP;
END $$;
COMMIT;
