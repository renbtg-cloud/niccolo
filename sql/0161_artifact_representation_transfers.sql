-- An explicit handoff copies one immutable representation into another Situation.
BEGIN;

CREATE TABLE niccolo.artifact_representation_transfers (
  workspace_id UUID NOT NULL,
  target_situation_id UUID NOT NULL,
  transfer_id UUID NOT NULL,
  source_situation_id UUID NOT NULL,
  representation_id UUID NOT NULL,
  artifact_id UUID NOT NULL,
  note TEXT NOT NULL CHECK (octet_length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, target_situation_id, transfer_id),
  UNIQUE (workspace_id, target_situation_id, representation_id),
  FOREIGN KEY (workspace_id, source_situation_id, representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id, situation_id, representation_id),
  FOREIGN KEY (workspace_id, target_situation_id, representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id, situation_id, representation_id),
  FOREIGN KEY (workspace_id, target_situation_id, artifact_id)
    REFERENCES niccolo.situation_artifacts(workspace_id, situation_id, artifact_id),
  CHECK (source_situation_id <> target_situation_id)
);

GRANT SELECT, INSERT ON niccolo.artifact_representation_transfers TO niccolo_app;
ALTER TABLE niccolo.artifact_representation_transfers ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.artifact_representation_transfers FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.artifact_representation_transfers
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid);
CREATE POLICY api_read_gate ON niccolo.artifact_representation_transfers AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id, 'read'));
CREATE POLICY api_write_insert_gate ON niccolo.artifact_representation_transfers AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id, 'write'));

COMMIT;
