-- Expert-authored review of selective reassessment obligations.
-- A review event records what the investigator did with an impact; it does not rewrite history.
BEGIN;

CREATE UNIQUE INDEX epistemic_relations_workspace_situation_relation_unique
  ON niccolo.epistemic_relations(workspace_id,situation_id,relation_id);
CREATE UNIQUE INDEX epistemic_arguments_workspace_situation_argument_unique
  ON niccolo.epistemic_arguments(workspace_id,situation_id,argument_id);

CREATE TABLE niccolo.epistemic_reassessment_review_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  review_event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  trigger_relation_id UUID NOT NULL,
  argument_id UUID NOT NULL,
  grounding_set_id UUID,
  disposition TEXT NOT NULL CHECK (disposition IN ('RETAINED','REVISION_REQUIRED','DEFERRED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,review_event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,situation_id,trigger_relation_id)
    REFERENCES niccolo.epistemic_relations(workspace_id,situation_id,relation_id),
  FOREIGN KEY (workspace_id,situation_id,argument_id)
    REFERENCES niccolo.epistemic_arguments(workspace_id,situation_id,argument_id),
  FOREIGN KEY (workspace_id,argument_id,grounding_set_id)
    REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id)
);
CREATE INDEX epistemic_reassessment_reviews_by_impact
  ON niccolo.epistemic_reassessment_review_events(
    workspace_id,situation_id,trigger_relation_id,argument_id,grounding_set_id,event_no DESC
  );

GRANT SELECT,INSERT ON niccolo.epistemic_reassessment_review_events TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE
  niccolo.epistemic_reassessment_review_events_event_no_seq
TO niccolo_app;

ALTER TABLE niccolo.epistemic_reassessment_review_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_reassessment_review_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_reassessment_review_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_reassessment_review_events
  AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_reassessment_review_events
  AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
