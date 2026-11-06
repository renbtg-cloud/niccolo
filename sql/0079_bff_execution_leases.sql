-- v0.311-v0.320: durable execution lease shared by sync and async BFF delivery.
-- A lease prevents two delivery surfaces from executing the same durable task concurrently.
BEGIN;
ALTER TABLE niccolo.bff_operations
  ADD COLUMN execution_owner TEXT,
  ADD COLUMN execution_lease_until TIMESTAMPTZ,
  ADD COLUMN execution_attempts INTEGER NOT NULL DEFAULT 0 CHECK (execution_attempts >= 0),
  ADD CONSTRAINT bff_execution_lease_pair CHECK (
    (execution_owner IS NULL AND execution_lease_until IS NULL)
    OR
    (execution_owner IS NOT NULL AND execution_lease_until IS NOT NULL
      AND length(execution_owner) BETWEEN 1 AND 128 AND btrim(execution_owner) <> '')
  );

CREATE INDEX bff_operations_execution_lease
  ON niccolo.bff_operations(workspace_id,state,execution_lease_until,created_at,operation_id)
  WHERE state IN ('QUEUED','RUNNING');

GRANT UPDATE (execution_owner,execution_lease_until,execution_attempts,updated_at)
  ON niccolo.bff_operations TO niccolo_app;
COMMIT;
