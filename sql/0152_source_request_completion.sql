-- A human may close an accepted acquisition request without claiming that the result is evidence.
BEGIN;

ALTER TABLE niccolo.situation_analysis_source_request_events
  DROP CONSTRAINT situation_analysis_source_request_events_event_kind_check;

ALTER TABLE niccolo.situation_analysis_source_request_events
  ADD CONSTRAINT situation_analysis_source_request_events_event_kind_check
  CHECK(event_kind IN ('ACCEPTED','DECLINED','DEFERRED','REOPENED','COMPLETED'));

COMMIT;
