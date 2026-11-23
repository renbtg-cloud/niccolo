-- Give append-only source-request decisions a deterministic insertion order.
BEGIN;

ALTER TABLE niccolo.situation_analysis_source_request_events
  ADD COLUMN event_no BIGINT GENERATED ALWAYS AS IDENTITY;

ALTER TABLE niccolo.situation_analysis_source_request_events
  ADD CONSTRAINT situation_analysis_source_request_events_workspace_event_no_key
  UNIQUE (workspace_id,event_no);

CREATE INDEX situation_analysis_source_request_events_latest
  ON niccolo.situation_analysis_source_request_events(
    workspace_id,situation_id,analysis_id,request_id,event_no DESC
  );

GRANT USAGE,SELECT ON SEQUENCE
  niccolo.situation_analysis_source_request_events_event_no_seq
  TO niccolo_app;

COMMIT;
