-- Parser-free MVP substrate: external artifact identity, location, declared access,
-- human-supplied representations, and qualified artifact relationships.
BEGIN;

CREATE TABLE niccolo.artifacts (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  artifact_id UUID NOT NULL,
  artifact_type TEXT NOT NULL CHECK(octet_length(artifact_type) BETWEEN 1 AND 128 AND btrim(artifact_type)<>'' AND artifact_type !~ '[[:cntrl:]]'),
  label TEXT NOT NULL CHECK(octet_length(label) BETWEEN 1 AND 1024 AND btrim(label)<>'' AND label !~ '[[:cntrl:]]'),
  declared_mime_type TEXT CHECK(declared_mime_type IS NULL OR (octet_length(declared_mime_type) BETWEEN 1 AND 255 AND btrim(declared_mime_type)<>'' AND declared_mime_type !~ '[[:cntrl:]]')),
  declared_byte_length BIGINT CHECK(declared_byte_length IS NULL OR declared_byte_length>=0),
  declared_sha256 TEXT CHECK(declared_sha256 IS NULL OR declared_sha256 ~ '^[a-f0-9]{64}$'),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,artifact_id)
);

CREATE TABLE niccolo.artifact_storage_references (
  workspace_id UUID NOT NULL,
  artifact_id UUID NOT NULL,
  storage_reference_id UUID NOT NULL,
  storage_type TEXT NOT NULL CHECK(octet_length(storage_type) BETWEEN 1 AND 64 AND btrim(storage_type)<>'' AND storage_type !~ '[[:cntrl:]]'),
  locator TEXT NOT NULL CHECK(octet_length(locator) BETWEEN 1 AND 4096 AND btrim(locator)<>'' AND locator !~ '[[:cntrl:]]'),
  provider TEXT CHECK(provider IS NULL OR (octet_length(provider) BETWEEN 1 AND 256 AND btrim(provider)<>'' AND provider !~ '[[:cntrl:]]')),
  environment_label TEXT CHECK(environment_label IS NULL OR (octet_length(environment_label) BETWEEN 1 AND 256 AND btrim(environment_label)<>'' AND environment_label !~ '[[:cntrl:]]')),
  resolver_profile TEXT CHECK(resolver_profile IS NULL OR (octet_length(resolver_profile) BETWEEN 1 AND 256 AND btrim(resolver_profile)<>'' AND resolver_profile !~ '[[:cntrl:]]')),
  notes TEXT CHECK(notes IS NULL OR (octet_length(notes) BETWEEN 1 AND 4096 AND btrim(notes)<>'')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,artifact_id,storage_reference_id),
  UNIQUE(workspace_id,storage_reference_id),
  FOREIGN KEY(workspace_id,artifact_id) REFERENCES niccolo.artifacts(workspace_id,artifact_id)
);

CREATE TABLE niccolo.artifact_storage_access_events (
  workspace_id UUID NOT NULL,
  artifact_id UUID NOT NULL,
  storage_reference_id UUID NOT NULL,
  access_event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  access_status TEXT NOT NULL CHECK(access_status IN ('UNKNOWN','NONE','AVAILABLE','TEMPORARILY_UNAVAILABLE','ACCESS_DENIED','NOT_FOUND','UNVERIFIED')),
  status_basis TEXT NOT NULL DEFAULT 'HUMAN_DECLARATION' CHECK(status_basis='HUMAN_DECLARATION'),
  note TEXT CHECK(note IS NULL OR (octet_length(note) BETWEEN 1 AND 4096 AND btrim(note)<>'')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,artifact_id,storage_reference_id,access_event_id),
  UNIQUE(workspace_id,event_no),
  FOREIGN KEY(workspace_id,artifact_id,storage_reference_id)
    REFERENCES niccolo.artifact_storage_references(workspace_id,artifact_id,storage_reference_id)
);

CREATE TABLE niccolo.situation_artifacts (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  artifact_id UUID NOT NULL,
  added_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  added_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,artifact_id),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY(workspace_id,artifact_id) REFERENCES niccolo.artifacts(workspace_id,artifact_id)
);

CREATE TABLE niccolo.situation_artifact_representations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  artifact_id UUID NOT NULL,
  representation_id UUID NOT NULL,
  representation_type TEXT NOT NULL CHECK(octet_length(representation_type) BETWEEN 1 AND 128 AND btrim(representation_type)<>'' AND representation_type !~ '[[:cntrl:]]'),
  producer_type TEXT NOT NULL CHECK(producer_type IN ('USER','THIRD_PARTY_HUMAN','EXTERNAL_SYSTEM','UNKNOWN')),
  producer_label TEXT CHECK(producer_label IS NULL OR (octet_length(producer_label) BETWEEN 1 AND 512 AND btrim(producer_label)<>'' AND producer_label !~ '[[:cntrl:]]')),
  production_method TEXT CHECK(production_method IS NULL OR (octet_length(production_method) BETWEEN 1 AND 1024 AND btrim(production_method)<>'' AND production_method !~ '[[:cntrl:]]')),
  scope_locator TEXT NOT NULL CHECK(octet_length(scope_locator) BETWEEN 1 AND 2048 AND btrim(scope_locator)<>'' AND scope_locator !~ '[[:cntrl:]]'),
  content TEXT NOT NULL CHECK(octet_length(content) BETWEEN 1 AND 65536 AND btrim(content)<>''),
  completeness_status TEXT NOT NULL CHECK(completeness_status IN ('COMPLETE','PARTIAL','UNKNOWN')),
  epistemic_qualification TEXT CHECK(epistemic_qualification IS NULL OR (octet_length(epistemic_qualification) BETWEEN 1 AND 4096 AND btrim(epistemic_qualification)<>'')),
  limitations TEXT CHECK(limitations IS NULL OR (octet_length(limitations) BETWEEN 1 AND 4096 AND btrim(limitations)<>'')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,representation_id),
  FOREIGN KEY(workspace_id,situation_id,artifact_id)
    REFERENCES niccolo.situation_artifacts(workspace_id,situation_id,artifact_id)
);

CREATE TABLE niccolo.situation_artifact_relationships (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  relationship_id UUID NOT NULL,
  subject_artifact_id UUID NOT NULL,
  object_artifact_id UUID NOT NULL,
  relationship_type TEXT NOT NULL CHECK(octet_length(relationship_type) BETWEEN 1 AND 128 AND btrim(relationship_type)<>'' AND relationship_type !~ '[[:cntrl:]]'),
  qualification TEXT CHECK(qualification IS NULL OR (octet_length(qualification) BETWEEN 1 AND 4096 AND btrim(qualification)<>'')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,relationship_id),
  FOREIGN KEY(workspace_id,situation_id,subject_artifact_id)
    REFERENCES niccolo.situation_artifacts(workspace_id,situation_id,artifact_id),
  FOREIGN KEY(workspace_id,situation_id,object_artifact_id)
    REFERENCES niccolo.situation_artifacts(workspace_id,situation_id,artifact_id),
  CHECK(subject_artifact_id<>object_artifact_id)
);

CREATE INDEX artifact_storage_reference_order
  ON niccolo.artifact_storage_references(workspace_id,artifact_id,recorded_at,storage_reference_id);
CREATE INDEX artifact_storage_access_latest
  ON niccolo.artifact_storage_access_events(workspace_id,artifact_id,storage_reference_id,event_no DESC);
CREATE INDEX situation_artifact_representation_order
  ON niccolo.situation_artifact_representations(workspace_id,situation_id,artifact_id,recorded_at,representation_id);
CREATE INDEX situation_artifact_relationship_subject
  ON niccolo.situation_artifact_relationships(workspace_id,situation_id,subject_artifact_id,recorded_at);

GRANT SELECT,INSERT ON niccolo.artifacts,niccolo.artifact_storage_references,
  niccolo.artifact_storage_access_events,niccolo.situation_artifacts,
  niccolo.situation_artifact_representations,niccolo.situation_artifact_relationships TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.artifact_storage_access_events_event_no_seq TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY[
    'artifacts','artifact_storage_references','artifact_storage_access_events',
    'situation_artifacts','situation_artifact_representations','situation_artifact_relationships'
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

COMMENT ON TABLE niccolo.artifacts IS 'External or alleged artifact identity; no source bytes are stored here.';
COMMENT ON TABLE niccolo.situation_artifact_representations IS 'Attributed epistemic text explicitly supplied by a human; never an automatically parsed artifact fragment in the MVP.';
COMMIT;
