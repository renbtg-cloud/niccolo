-- v0.211-v0.225: durable, inspectable agentic plan DAG. The controller may select
-- internal reasoning work, but human-required steps pause the operation rather than
-- manufacturing consent or silently executing an external action.
BEGIN;
CREATE TABLE niccolo.agentic_plans (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  plan_revision BIGINT NOT NULL CHECK (plan_revision >= 1),
  plan_sha256 CHAR(64) NOT NULL CHECK (plan_sha256 ~ '^[a-f0-9]{64}$'),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,operation_id,plan_revision),
  FOREIGN KEY(workspace_id,situation_id,operation_id) REFERENCES niccolo.bff_operations(workspace_id,situation_id,operation_id)
);
CREATE TABLE niccolo.agentic_steps (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  plan_revision BIGINT NOT NULL,
  step_id UUID NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 10000),
  step_kind TEXT NOT NULL CHECK (step_kind IN ('INSPECT_SOURCES','ASSESS_CONTEXT','PROPOSE_CLARIFICATION','SYNTHESIZE','CHECK_ASSUMPTIONS','CHECK_ALTERNATIVES','CHECK_STOP_CONDITIONS','REQUEST_HUMAN_DECISION','PREPARE_HANDOFF')),
  label TEXT NOT NULL CHECK (length(label) BETWEEN 1 AND 1024 AND btrim(label) <> ''),
  requires_human BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,operation_id,plan_revision,step_id),
  UNIQUE(workspace_id,situation_id,operation_id,plan_revision,ordinal),
  FOREIGN KEY(workspace_id,situation_id,operation_id,plan_revision) REFERENCES niccolo.agentic_plans(workspace_id,situation_id,operation_id,plan_revision)
);
CREATE TABLE niccolo.agentic_step_dependencies (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  plan_revision BIGINT NOT NULL,
  step_id UUID NOT NULL,
  depends_on_step_id UUID NOT NULL,
  PRIMARY KEY(workspace_id,situation_id,operation_id,plan_revision,step_id,depends_on_step_id),
  FOREIGN KEY(workspace_id,situation_id,operation_id,plan_revision,step_id) REFERENCES niccolo.agentic_steps(workspace_id,situation_id,operation_id,plan_revision,step_id),
  FOREIGN KEY(workspace_id,situation_id,operation_id,plan_revision,depends_on_step_id) REFERENCES niccolo.agentic_steps(workspace_id,situation_id,operation_id,plan_revision,step_id),
  CHECK(step_id <> depends_on_step_id)
);
CREATE TABLE niccolo.agentic_step_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  operation_id UUID NOT NULL,
  plan_revision BIGINT NOT NULL,
  step_id UUID NOT NULL,
  event_seq BIGINT GENERATED ALWAYS AS IDENTITY,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('STARTED','SUCCEEDED','FAILED','WAITING_FOR_HUMAN','RESUMED','SKIPPED','CANCELLED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,operation_id,plan_revision,step_id,event_seq),
  FOREIGN KEY(workspace_id,situation_id,operation_id,plan_revision,step_id) REFERENCES niccolo.agentic_steps(workspace_id,situation_id,operation_id,plan_revision,step_id)
);
CREATE INDEX agentic_step_events_latest ON niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_seq DESC);
GRANT SELECT, INSERT ON niccolo.agentic_plans,niccolo.agentic_steps,niccolo.agentic_step_dependencies,niccolo.agentic_step_events TO niccolo_app;
GRANT USAGE, SELECT ON SEQUENCE niccolo.agentic_step_events_event_seq_seq TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['agentic_plans','agentic_steps','agentic_step_dependencies','agentic_step_events'] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  END LOOP;
END $$;
COMMIT;
