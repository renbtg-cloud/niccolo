-- Freeze parser-free artifact identities and explicitly supplied epistemic material
-- into the same immutable Situation revision boundary used by analysis snapshots.
BEGIN;

ALTER TABLE niccolo.situation_revisions
  DROP CONSTRAINT situation_revisions_cause_check;
ALTER TABLE niccolo.situation_revisions
  ADD CONSTRAINT situation_revisions_cause_check CHECK(cause IN (
    'CREATED','FOCUS_CHANGED','SOURCE_ADDED','CONTRIBUTION_ADDED','CORRECTED','RETRACTED',
    'ARTIFACT_ADDED','ARTIFACT_STORAGE_RECORDED','ARTIFACT_REPRESENTATION_ADDED',
    'ARTIFACT_RELATIONSHIP_ADDED'
  ));

CREATE TABLE niccolo.situation_revision_artifacts (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL,
  artifact_id UUID NOT NULL,
  PRIMARY KEY(workspace_id,situation_id,revision,artifact_id),
  FOREIGN KEY(workspace_id,situation_id,revision)
    REFERENCES niccolo.situation_revisions(workspace_id,situation_id,revision),
  FOREIGN KEY(workspace_id,situation_id,artifact_id)
    REFERENCES niccolo.situation_artifacts(workspace_id,situation_id,artifact_id)
);

CREATE TABLE niccolo.situation_revision_artifact_storage_references (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL,
  artifact_id UUID NOT NULL,
  storage_reference_id UUID NOT NULL,
  access_event_id UUID NOT NULL,
  PRIMARY KEY(workspace_id,situation_id,revision,artifact_id,storage_reference_id),
  FOREIGN KEY(workspace_id,situation_id,revision,artifact_id)
    REFERENCES niccolo.situation_revision_artifacts(workspace_id,situation_id,revision,artifact_id),
  FOREIGN KEY(workspace_id,artifact_id,storage_reference_id)
    REFERENCES niccolo.artifact_storage_references(workspace_id,artifact_id,storage_reference_id),
  FOREIGN KEY(workspace_id,artifact_id,storage_reference_id,access_event_id)
    REFERENCES niccolo.artifact_storage_access_events(workspace_id,artifact_id,storage_reference_id,access_event_id)
);

CREATE TABLE niccolo.situation_revision_artifact_representations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL,
  artifact_id UUID NOT NULL,
  representation_id UUID NOT NULL,
  PRIMARY KEY(workspace_id,situation_id,revision,representation_id),
  FOREIGN KEY(workspace_id,situation_id,revision,artifact_id)
    REFERENCES niccolo.situation_revision_artifacts(workspace_id,situation_id,revision,artifact_id),
  FOREIGN KEY(workspace_id,situation_id,representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id,situation_id,representation_id)
);

CREATE TABLE niccolo.situation_revision_artifact_relationships (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL,
  relationship_id UUID NOT NULL,
  PRIMARY KEY(workspace_id,situation_id,revision,relationship_id),
  FOREIGN KEY(workspace_id,situation_id,revision)
    REFERENCES niccolo.situation_revisions(workspace_id,situation_id,revision),
  FOREIGN KEY(workspace_id,situation_id,relationship_id)
    REFERENCES niccolo.situation_artifact_relationships(workspace_id,situation_id,relationship_id)
);

GRANT SELECT,INSERT ON niccolo.situation_revision_artifacts,
  niccolo.situation_revision_artifact_storage_references,
  niccolo.situation_revision_artifact_representations,
  niccolo.situation_revision_artifact_relationships TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY[
    'situation_revision_artifacts','situation_revision_artifact_storage_references',
    'situation_revision_artifact_representations','situation_revision_artifact_relationships'
  ] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false)',t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false)',t);
  END LOOP;
END $$;
COMMIT;
