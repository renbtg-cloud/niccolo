-- v0.98: session-local human dispositions. They never become remembered future policy.
BEGIN;
CREATE TABLE niccolo.situation_review_session_dispositions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  session_id UUID NOT NULL,
  disposition_id UUID NOT NULL,
  target_type TEXT NOT NULL CHECK (target_type IN ('CLAIM','QUESTION','CONDITION','ACTION_PROPOSAL','UNCERTAINTY')),
  target_id UUID NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('KEEP_OPEN','DEFERRED','RESOLVED_FOR_THIS_SESSION','OUT_OF_SCOPE_FOR_THIS_SESSION')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,session_id,disposition_id),
  FOREIGN KEY(workspace_id,situation_id,session_id)
    REFERENCES niccolo.situation_review_sessions(workspace_id,situation_id,session_id)
);
CREATE FUNCTION niccolo.validate_review_session_disposition() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE a UUID; ok BOOLEAN;
BEGIN
  SELECT analysis_id INTO a FROM niccolo.situation_review_sessions
   WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND session_id=NEW.session_id;
  IF a IS NULL THEN RAISE EXCEPTION 'session disposition target requires analysis-scoped session'; END IF;
  IF NEW.target_type='CLAIM' THEN
    SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claims WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND analysis_id=a AND claim_id=NEW.target_id) INTO ok;
  ELSIF NEW.target_type='QUESTION' THEN
    SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_clarification_questions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND analysis_id=a AND question_id=NEW.target_id) INTO ok;
  ELSIF NEW.target_type='CONDITION' THEN
    SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_claim_revision_conditions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND analysis_id=a AND condition_id=NEW.target_id) INTO ok;
  ELSIF NEW.target_type='ACTION_PROPOSAL' THEN
    SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_action_proposals WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND analysis_id=a AND proposal_id=NEW.target_id) INTO ok;
  ELSE
    SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_uncertainties WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND analysis_id=a AND uncertainty_id=NEW.target_id) INTO ok;
  END IF;
  IF NOT coalesce(ok,false) THEN RAISE EXCEPTION 'review disposition target not in session analysis'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_review_session_disposition_before_insert
BEFORE INSERT ON niccolo.situation_review_session_dispositions
FOR EACH ROW EXECUTE FUNCTION niccolo.validate_review_session_disposition();
GRANT SELECT,INSERT ON niccolo.situation_review_session_dispositions TO niccolo_app;
ALTER TABLE niccolo.situation_review_session_dispositions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_review_session_dispositions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_review_session_dispositions USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_review_session_dispositions AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_review_session_dispositions AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
