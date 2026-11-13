-- v1.x: database-level guards for bounded temporal actor-state semantics.
-- NOT VALID keeps upgrades possible when a pre-guard local database contains
-- historical rows; PostgreSQL still enforces both checks for every new write.
BEGIN;

ALTER TABLE niccolo.situation_analysis_temporal_assertions
  ADD CONSTRAINT temporal_assertion_inferred_awareness_stays_inferential
  CHECK (
    assertion_type <> 'INFERRED_AWARENESS'
    OR qualification IN ('INFERRED','UNCERTAIN','CONTRADICTED','UNKNOWN')
  ) NOT VALID;

ALTER TABLE niccolo.situation_analysis_temporal_assertions
  ADD CONSTRAINT temporal_assertion_interval_not_reversed
  CHECK (
    valid_from IS NULL
    OR valid_to IS NULL
    OR valid_from <= valid_to
  ) NOT VALID;

COMMIT;
