-- Freeze generic actor-kind and source-binding horizons per analysis.
-- Existing analyses default to zero rather than claiming they saw later actor metadata.
BEGIN;

ALTER TABLE niccolo.situation_analysis_snapshots
  ADD COLUMN actor_kind_event_no_max BIGINT NOT NULL DEFAULT 0
  CHECK(actor_kind_event_no_max >= 0),
  ADD COLUMN actor_binding_event_no_max BIGINT NOT NULL DEFAULT 0
  CHECK(actor_binding_event_no_max >= 0);

COMMIT;
