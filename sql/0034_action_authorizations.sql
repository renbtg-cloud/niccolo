-- v0.67-v0.68: explicit authorization boundary after a human action decision.
-- Authorization is still not execution; this schema contains no executor or external side effect.
BEGIN;
CREATE TABLE niccolo.situation_action_authorization_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  proposal_id UUID NOT NULL,
  authorization_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('AUTHORIZED','REVOKED')),
  accepted_decision_id UUID,
  authorization_scope TEXT NOT NULL CHECK (length(authorization_scope) BETWEEN 1 AND 4096 AND btrim(authorization_scope) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,proposal_id,authorization_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,proposal_id)
    REFERENCES niccolo.situation_analysis_action_proposals(workspace_id,situation_id,analysis_id,proposal_id)
);
CREATE OR REPLACE FUNCTION niccolo.validate_action_authorization() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.event_kind='AUTHORIZED' THEN
    IF NEW.accepted_decision_id IS NULL OR NOT EXISTS (
      SELECT 1 FROM niccolo.situation_analysis_action_decisions d
      WHERE d.workspace_id=NEW.workspace_id AND d.situation_id=NEW.situation_id
        AND d.analysis_id=NEW.analysis_id AND d.proposal_id=NEW.proposal_id
        AND d.decision_id=NEW.accepted_decision_id AND d.decision_kind='ACCEPTED') THEN
      RAISE EXCEPTION 'authorization requires an explicit ACCEPTED human decision';
    END IF;
  ELSIF NEW.accepted_decision_id IS NOT NULL THEN
    RAISE EXCEPTION 'revocation must not invent a new accepted decision';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_action_authorization_before_insert
  BEFORE INSERT ON niccolo.situation_action_authorization_events
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_action_authorization();
GRANT SELECT, INSERT ON niccolo.situation_action_authorization_events TO niccolo_app;
ALTER TABLE niccolo.situation_action_authorization_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_action_authorization_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_action_authorization_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_action_authorization_events AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_action_authorization_events AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
