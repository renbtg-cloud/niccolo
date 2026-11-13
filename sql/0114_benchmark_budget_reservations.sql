-- Transactional budget reservations for optional comparative/frontier probes.
-- A reservation is created before any paid provider side effect and atomically
-- contributes to the workspace/account monthly_reserved counter.
BEGIN;

CREATE TABLE niccolo.benchmark_budget_reservations (
  workspace_id UUID NOT NULL,
  reservation_id UUID NOT NULL,
  run_id UUID NOT NULL,
  account_subject TEXT NOT NULL,
  estimated_micro_usd BIGINT NOT NULL CHECK(estimated_micro_usd > 0),
  status TEXT NOT NULL DEFAULT 'RESERVED'
    CHECK(status IN ('RESERVED','SETTLED','RELEASED')),
  actual_micro_usd BIGINT CHECK(actual_micro_usd >= 0),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  closed_by_principal_id UUID REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  closed_at TIMESTAMPTZ,
  PRIMARY KEY(workspace_id,reservation_id),
  UNIQUE(workspace_id,run_id),
  FOREIGN KEY(workspace_id,run_id)
    REFERENCES niccolo.benchmark_runs(workspace_id,run_id),
  FOREIGN KEY(workspace_id,account_subject)
    REFERENCES niccolo.evaluation_policies(workspace_id,account_subject),
  CHECK(
    (status='RESERVED' AND actual_micro_usd IS NULL AND closed_by_principal_id IS NULL AND closed_at IS NULL)
    OR
    (status='SETTLED' AND actual_micro_usd IS NOT NULL AND closed_by_principal_id IS NOT NULL AND closed_at IS NOT NULL)
    OR
    (status='RELEASED' AND actual_micro_usd IS NULL AND closed_by_principal_id IS NOT NULL AND closed_at IS NOT NULL)
  )
);

CREATE INDEX benchmark_budget_reservations_open
  ON niccolo.benchmark_budget_reservations(workspace_id,account_subject,created_at)
  WHERE status='RESERVED';

GRANT SELECT,INSERT,UPDATE ON niccolo.benchmark_budget_reservations TO niccolo_app;

ALTER TABLE niccolo.benchmark_budget_reservations ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.benchmark_budget_reservations FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.benchmark_budget_reservations
  USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.benchmark_budget_reservations AS RESTRICTIVE
  FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.benchmark_budget_reservations AS RESTRICTIVE
  FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.benchmark_budget_reservations AS RESTRICTIVE
  FOR UPDATE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.benchmark_budget_reservations AS RESTRICTIVE
  FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
