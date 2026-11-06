-- v0.401-v0.450: bounded model invocation governance and telemetry.
BEGIN;
ALTER TABLE niccolo.model_invocations
  ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
  ADD COLUMN timeout_ms INTEGER CHECK (timeout_ms BETWEEN 1 AND 120000),
  ADD COLUMN latency_ms BIGINT CHECK (latency_ms >= 0),
  ADD COLUMN max_total_tokens BIGINT CHECK (max_total_tokens IS NULL OR max_total_tokens >= 0),
  ADD COLUMN budget_exceeded BOOLEAN NOT NULL DEFAULT false;
GRANT UPDATE (attempt_count, timeout_ms, latency_ms, max_total_tokens, budget_exceeded)
  ON niccolo.model_invocations TO niccolo_app;
COMMIT;
