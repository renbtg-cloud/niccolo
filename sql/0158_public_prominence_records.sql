-- Source-backed, attributed reports about recognizability within an audience.
BEGIN;

CREATE TABLE niccolo.public_prominence_records (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  record_id UUID NOT NULL,
  subject_actor_id UUID NOT NULL,
  audience_actor_id UUID NOT NULL,
  cue TEXT NOT NULL CHECK (cue IN ('NAME', 'FACE', 'PUBLIC_ROLE')),
  observation TEXT NOT NULL CHECK (observation IN (
    'RECOGNITION_REPORTED', 'NON_RECOGNITION_REPORTED', 'EXPOSURE_REPORTED'
  )),
  audience_scope TEXT NOT NULL CHECK (
    octet_length(audience_scope) BETWEEN 1 AND 2048 AND btrim(audience_scope) <> ''
  ),
  effective_from DATE NOT NULL,
  effective_through DATE NOT NULL,
  available_on DATE NOT NULL,
  claim_object_id UUID NOT NULL,
  representation_id UUID NOT NULL,
  recorded_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, record_id),
  FOREIGN KEY (workspace_id, situation_id)
    REFERENCES niccolo.situations(workspace_id, situation_id),
  FOREIGN KEY (workspace_id, subject_actor_id)
    REFERENCES niccolo.actors(workspace_id, actor_id),
  FOREIGN KEY (workspace_id, audience_actor_id)
    REFERENCES niccolo.actors(workspace_id, actor_id),
  FOREIGN KEY (workspace_id, situation_id, claim_object_id)
    REFERENCES niccolo.epistemic_situation_propositions(workspace_id, situation_id, proposition_id),
  FOREIGN KEY (workspace_id, situation_id, claim_object_id, representation_id)
    REFERENCES niccolo.workbench_artifact_provenance(workspace_id, situation_id, object_id, representation_id),
  CHECK (subject_actor_id <> audience_actor_id),
  CHECK (effective_from <= effective_through)
);

CREATE INDEX public_prominence_scope
  ON niccolo.public_prominence_records (
    workspace_id, situation_id, subject_actor_id, audience_actor_id, effective_from, available_on
  );

GRANT SELECT, INSERT ON niccolo.public_prominence_records TO niccolo_app;
ALTER TABLE niccolo.public_prominence_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.public_prominence_records FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.public_prominence_records
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid);
CREATE POLICY api_read_gate ON niccolo.public_prominence_records AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id, 'read'));
CREATE POLICY api_write_insert_gate ON niccolo.public_prominence_records AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id, 'write'));

COMMIT;
