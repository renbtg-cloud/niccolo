-- v0.101: explicit human-owned checklist items for a review session.
BEGIN;
CREATE TABLE niccolo.situation_review_checklist_items (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  session_id UUID NOT NULL,
  item_id UUID NOT NULL,
  item_text TEXT NOT NULL CHECK (length(item_text) BETWEEN 1 AND 4096 AND btrim(item_text) <> ''),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,session_id,item_id),
  FOREIGN KEY(workspace_id,situation_id,session_id)
    REFERENCES niccolo.situation_review_sessions(workspace_id,situation_id,session_id)
);
GRANT SELECT,INSERT ON niccolo.situation_review_checklist_items TO niccolo_app;
ALTER TABLE niccolo.situation_review_checklist_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_review_checklist_items FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_review_checklist_items USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_review_checklist_items AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_review_checklist_items AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
