-- Expert-authored investigative questions are not assertions or information gaps.
BEGIN;

ALTER TABLE niccolo.epistemic_objects DROP CONSTRAINT epistemic_objects_object_kind_check;
ALTER TABLE niccolo.epistemic_objects ADD CONSTRAINT epistemic_objects_object_kind_check CHECK (
    object_kind IN ('PROPOSITION','HYPOTHESIS','ASSUMPTION','INFORMATION_GAP','FINDING','ARGUMENT',
                    'UTTERANCE','UTTERANCE_INTERPRETATION','QUESTION')
);

CREATE TABLE niccolo.epistemic_questions (
  workspace_id UUID NOT NULL,
  question_id UUID NOT NULL,
  object_kind TEXT NOT NULL DEFAULT 'QUESTION' CHECK (object_kind='QUESTION'),
  situation_id UUID NOT NULL,
  question_text TEXT NOT NULL CHECK (length(question_text) BETWEEN 1 AND 8192 AND btrim(question_text)<>''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,question_id),
  FOREIGN KEY (workspace_id,question_id,object_kind)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id,object_kind),
  FOREIGN KEY (workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE INDEX epistemic_questions_by_situation ON niccolo.epistemic_questions(workspace_id,situation_id,created_at,question_id);
GRANT SELECT,INSERT ON niccolo.epistemic_questions TO niccolo_app;
ALTER TABLE niccolo.epistemic_questions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_questions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_questions
 USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_questions AS RESTRICTIVE FOR SELECT TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_questions AS RESTRICTIVE FOR INSERT TO niccolo_api
 WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_questions AS RESTRICTIVE FOR UPDATE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_questions AS RESTRICTIVE FOR DELETE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
