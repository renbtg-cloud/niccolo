-- v1.401-v1.500: an orphaned STARTED step can become retry-ready after an explicit stale threshold.
BEGIN;
ALTER TABLE niccolo.agentic_step_events DROP CONSTRAINT agentic_step_events_event_kind_check;
ALTER TABLE niccolo.agentic_step_events ADD CONSTRAINT agentic_step_events_event_kind_check CHECK(event_kind IN('STARTED','SUCCEEDED','FAILED','WAITING_FOR_HUMAN','RESUMED','SKIPPED','CANCELLED','RETRY_READY'));
COMMIT;
