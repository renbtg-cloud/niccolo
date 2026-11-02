-- Human-directed investigative questions and durable, distinguishable feedback.
-- No automated truth/confidence updates follow from a response.
BEGIN;
CREATE TABLE niccolo.question_feedback_events (
 workspace_id UUID NOT NULL,
 event_id UUID NOT NULL,
 event_no BIGINT GENERATED ALWAYS AS IDENTITY,
 question_id UUID NOT NULL,
 response TEXT NOT NULL CHECK(response IN ('REJECTED','INSUFFICIENT_EVIDENCE','UNKNOWN')),
 human_reason TEXT NOT NULL CHECK(length(human_reason) BETWEEN 1 AND 2048),
 actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
 recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,event_id),
 UNIQUE(workspace_id,event_no),
 FOREIGN KEY(workspace_id,question_id) REFERENCES niccolo.investigative_questions(workspace_id,question_id)
);
CREATE INDEX question_feedback_by_question ON niccolo.question_feedback_events
 (workspace_id,question_id,event_no);
-- Deliberately immutable, including attempts from principals with table privileges.
CREATE OR REPLACE FUNCTION niccolo.reject_feedback_rewrite() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog AS $$
BEGIN
 RAISE EXCEPTION 'question feedback is append-only' USING ERRCODE='23514';
END $$;
CREATE TRIGGER feedback_immutable BEFORE UPDATE OR DELETE ON niccolo.question_feedback_events
 FOR EACH ROW EXECUTE FUNCTION niccolo.reject_feedback_rewrite();
REVOKE UPDATE, DELETE ON niccolo.investigative_questions FROM niccolo_app;
GRANT SELECT,INSERT ON niccolo.question_feedback_events TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.question_feedback_events_event_no_seq TO niccolo_app;
ALTER TABLE niccolo.question_feedback_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.question_feedback_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.question_feedback_events
 USING(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
COMMIT;
