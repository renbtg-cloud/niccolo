BEGIN;

ALTER TABLE niccolo.situation_artifact_representations
  ADD CONSTRAINT artifact_representation_scope_key
  UNIQUE (workspace_id,situation_id,artifact_id,representation_id);

ALTER TABLE niccolo.situation_artifact_relationships
  ADD COLUMN subject_representation_id UUID,
  ADD COLUMN object_representation_id UUID,
  ADD CONSTRAINT artifact_relationship_subject_representation
    FOREIGN KEY (workspace_id,situation_id,subject_artifact_id,subject_representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id,situation_id,artifact_id,representation_id),
  ADD CONSTRAINT artifact_relationship_object_representation
    FOREIGN KEY (workspace_id,situation_id,object_artifact_id,object_representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id,situation_id,artifact_id,representation_id);

COMMIT;
