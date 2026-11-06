-- v0.61: explicit human reanalysis requests tied to one exact steering event.
-- A request records intent; it does not itself create or run a new analysis.
BEGIN;
CREATE TABLE niccolo.situation_reanalysis_requests (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  request_id UUID NOT NULL,
  prior_analysis_id UUID NOT NULL,
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 4096 AND btrim(reason) <> ''),
  steering_event_type TEXT NOT NULL CHECK (steering_event_type IN
    ('CLAIM_REVIEW','CLARIFICATION_ANSWER','UNCERTAINTY_EVENT','ACTION_DECISION','ATTENTION_PIN_EVENT')),
  steering_event_id UUID NOT NULL,
  requested_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,request_id),
  FOREIGN KEY (workspace_id,situation_id,prior_analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
CREATE OR REPLACE FUNCTION niccolo.validate_reanalysis_steering_event() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE ok boolean := false;
BEGIN
  CASE NEW.steering_event_type
    WHEN 'CLAIM_REVIEW' THEN
      SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claim_reviews r
        WHERE r.workspace_id=NEW.workspace_id AND r.situation_id=NEW.situation_id
          AND r.analysis_id=NEW.prior_analysis_id AND r.review_id=NEW.steering_event_id) INTO ok;
    WHEN 'CLARIFICATION_ANSWER' THEN
      SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_clarification_answers a
        WHERE a.workspace_id=NEW.workspace_id AND a.situation_id=NEW.situation_id
          AND a.analysis_id=NEW.prior_analysis_id AND a.answer_id=NEW.steering_event_id) INTO ok;
    WHEN 'UNCERTAINTY_EVENT' THEN
      SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainty_events e
        WHERE e.workspace_id=NEW.workspace_id AND e.situation_id=NEW.situation_id
          AND e.analysis_id=NEW.prior_analysis_id AND e.event_id=NEW.steering_event_id) INTO ok;
    WHEN 'ACTION_DECISION' THEN
      SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_decisions d
        WHERE d.workspace_id=NEW.workspace_id AND d.situation_id=NEW.situation_id
          AND d.analysis_id=NEW.prior_analysis_id AND d.decision_id=NEW.steering_event_id) INTO ok;
    WHEN 'ATTENTION_PIN_EVENT' THEN
      SELECT EXISTS(SELECT 1 FROM niccolo.situation_attention_pin_events p
        WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id
          AND p.analysis_id=NEW.prior_analysis_id AND p.event_id=NEW.steering_event_id) INTO ok;
  END CASE;
  IF NOT ok THEN RAISE EXCEPTION 'reanalysis request steering event is not visible in prior analysis'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_reanalysis_steering_event_before_insert
  BEFORE INSERT ON niccolo.situation_reanalysis_requests
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_reanalysis_steering_event();
GRANT SELECT, INSERT ON niccolo.situation_reanalysis_requests TO niccolo_app;
ALTER TABLE niccolo.situation_reanalysis_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_reanalysis_requests FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_reanalysis_requests
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_reanalysis_requests AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_reanalysis_requests AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
