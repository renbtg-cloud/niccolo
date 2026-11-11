-- Explicit, quarantined forensic material selected as current analysis context.
-- Selection makes imported historical material visible to reasoning; it does not
-- turn the imported item into evidence, truth, trusted configuration, or current identity.
BEGIN;

CREATE TABLE niccolo.analysis_forensic_context (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  binding_id UUID NOT NULL,
  import_batch_id UUID NOT NULL,
  item_id UUID NOT NULL,
  note TEXT CHECK (note IS NULL OR (length(note) BETWEEN 1 AND 2048 AND btrim(note) <> '')),
  bound_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,binding_id),
  UNIQUE (workspace_id,situation_id,analysis_id,import_batch_id,item_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,import_batch_id,item_id)
    REFERENCES niccolo.forensic_import_items(workspace_id,import_batch_id,item_id)
);

CREATE INDEX analysis_forensic_context_by_item
  ON niccolo.analysis_forensic_context(workspace_id,import_batch_id,item_id,analysis_id);

GRANT SELECT,INSERT ON niccolo.analysis_forensic_context TO niccolo_app;

ALTER TABLE niccolo.analysis_forensic_context ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.analysis_forensic_context FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.analysis_forensic_context
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.analysis_forensic_context AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.analysis_forensic_context AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.analysis_forensic_context AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.analysis_forensic_context AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
