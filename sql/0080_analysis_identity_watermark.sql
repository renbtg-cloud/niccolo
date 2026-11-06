-- v1.161-v1.200: freeze the identity-event horizon seen by each analysis.
-- Existing historical analyses default to zero rather than retroactively claiming
-- they incorporated identity mappings that were not frozen at creation time.
BEGIN;
ALTER TABLE niccolo.situation_analysis_snapshots
  ADD COLUMN identity_event_no_max BIGINT NOT NULL DEFAULT 0
  CHECK(identity_event_no_max >= 0);
COMMIT;
