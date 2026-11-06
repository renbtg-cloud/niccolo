-- v0.141-v0.145: explicit human priorities; no automatic reordering.
BEGIN;
CREATE TABLE niccolo.situation_human_priorities (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  priority_id UUID NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 1),
  priority_text TEXT NOT NULL CHECK (length(priority_text) BETWEEN 1 AND 4096 AND btrim(priority_text) <> ''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  declared_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,priority_id),
  UNIQUE(workspace_id,situation_id,ordinal),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id)
);
GRANT SELECT,INSERT ON niccolo.situation_human_priorities TO niccolo_app;
ALTER TABLE niccolo.situation_human_priorities ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.situation_human_priorities FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_human_priorities USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_human_priorities AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_human_priorities AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
