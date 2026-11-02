-- Evidence references on human investigatory questions: source pointers,
-- NOT endorsements, truth assertions or autonomous conclusions.
BEGIN;
CREATE TABLE niccolo.question_evidence_references (
 workspace_id UUID NOT NULL,
 reference_id UUID NOT NULL,
 question_id UUID NOT NULL,
 evidence_id UUID NOT NULL,
 relation TEXT NOT NULL CHECK(relation IN ('REFERENCES','CHALLENGES','FOLLOW_UP_ON')),
 human_reason TEXT NOT NULL CHECK(length(human_reason) BETWEEN 1 AND 2048),
 actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
 recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,reference_id),
 FOREIGN KEY(workspace_id,question_id) REFERENCES niccolo.investigative_questions(workspace_id,question_id),
 FOREIGN KEY(workspace_id,evidence_id) REFERENCES niccolo.evidence(workspace_id,evidence_id)
);
CREATE INDEX question_refs_by_question ON niccolo.question_evidence_references (workspace_id,question_id,reference_id);
CREATE TRIGGER question_refs_immutable BEFORE UPDATE OR DELETE ON niccolo.question_evidence_references
 FOR EACH ROW EXECUTE FUNCTION niccolo.reject_feedback_rewrite();
GRANT SELECT,INSERT ON niccolo.question_evidence_references TO niccolo_app;
ALTER TABLE niccolo.question_evidence_references ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.question_evidence_references FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.question_evidence_references
 USING(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
COMMIT;
