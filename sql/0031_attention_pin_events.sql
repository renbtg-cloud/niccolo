-- v0.59-v0.60: human-selected attention pins and immutable pin history.
-- Pins are user steering only; they are not model priority, truth, or future policy.
BEGIN;
CREATE TABLE niccolo.situation_attention_pin_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  event_id UUID NOT NULL,
  target_type TEXT NOT NULL CHECK (target_type IN ('CLAIM','UNCERTAINTY','ACTION_PROPOSAL')),
  target_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('PINNED','UNPINNED')),
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 4096 AND btrim(reason) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,event_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);

CREATE OR REPLACE FUNCTION niccolo.validate_attention_pin_target() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.target_type='CLAIM' THEN
    IF NOT EXISTS (SELECT 1 FROM niccolo.situation_analysis_claims c
      WHERE c.workspace_id=NEW.workspace_id AND c.situation_id=NEW.situation_id
        AND c.analysis_id=NEW.analysis_id AND c.claim_id=NEW.target_id) THEN
      RAISE EXCEPTION 'attention pin claim target is not visible in analysis';
    END IF;
  ELSIF NEW.target_type='UNCERTAINTY' THEN
    IF NOT EXISTS (SELECT 1 FROM niccolo.situation_analysis_uncertainties u
      WHERE u.workspace_id=NEW.workspace_id AND u.situation_id=NEW.situation_id
        AND u.analysis_id=NEW.analysis_id AND u.uncertainty_id=NEW.target_id) THEN
      RAISE EXCEPTION 'attention pin uncertainty target is not visible in analysis';
    END IF;
  ELSIF NEW.target_type='ACTION_PROPOSAL' THEN
    IF NOT EXISTS (SELECT 1 FROM niccolo.situation_analysis_action_proposals p
      WHERE p.workspace_id=NEW.workspace_id AND p.situation_id=NEW.situation_id
        AND p.analysis_id=NEW.analysis_id AND p.proposal_id=NEW.target_id) THEN
      RAISE EXCEPTION 'attention pin proposal target is not visible in analysis';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_attention_pin_target_before_insert
  BEFORE INSERT ON niccolo.situation_attention_pin_events
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_attention_pin_target();
CREATE INDEX attention_pin_history_by_target
  ON niccolo.situation_attention_pin_events(workspace_id,situation_id,analysis_id,target_type,target_id,created_at,event_id);
GRANT SELECT, INSERT ON niccolo.situation_attention_pin_events TO niccolo_app;
ALTER TABLE niccolo.situation_attention_pin_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_attention_pin_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_attention_pin_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_attention_pin_events AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_attention_pin_events AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
