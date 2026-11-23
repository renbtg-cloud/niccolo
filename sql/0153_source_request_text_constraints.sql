-- Keep source-request audit text single-paragraph even when written outside the HTTP boundary.
BEGIN;

ALTER TABLE niccolo.situation_analysis_source_requests
  ADD CONSTRAINT situation_analysis_source_requests_no_control_text
  CHECK(
    request_text !~ '[[:cntrl:]]'
    AND source_kind !~ '[[:cntrl:]]'
    AND rationale !~ '[[:cntrl:]]'
  );

ALTER TABLE niccolo.situation_analysis_source_request_events
  ADD CONSTRAINT situation_analysis_source_request_events_no_control_text
  CHECK(note !~ '[[:cntrl:]]');

COMMIT;
