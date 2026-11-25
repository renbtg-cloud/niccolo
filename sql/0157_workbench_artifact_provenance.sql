BEGIN;

CREATE TABLE niccolo.workbench_artifact_provenance (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  object_id UUID NOT NULL,
  representation_id UUID NOT NULL,
  rationale TEXT NOT NULL CHECK(octet_length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,object_id,representation_id),
  FOREIGN KEY(workspace_id,object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id),
  FOREIGN KEY(workspace_id,situation_id,representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id,situation_id,representation_id)
);

GRANT SELECT, INSERT ON niccolo.workbench_artifact_provenance TO niccolo_app;
ALTER TABLE niccolo.workbench_artifact_provenance ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.workbench_artifact_provenance FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.workbench_artifact_provenance
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.workbench_artifact_provenance AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.workbench_artifact_provenance AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMENT ON TABLE niccolo.workbench_artifact_provenance IS 'Investigator-declared links to explicitly supplied interpretations of external artifacts.';

COMMIT;
