-- Untrusted forensic re-import quarantine.
-- Imported historical state remains physically separate from native evidence,
-- configuration and identity tables. Structural validity is not epistemic trust.
BEGIN;

CREATE TABLE niccolo.forensic_import_batches (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  import_batch_id UUID NOT NULL,
  source_package_sha256 CHAR(64) NOT NULL CHECK (source_package_sha256 ~ '^[a-f0-9]{64}$'),
  source_manifest_sha256 CHAR(64) NOT NULL CHECK (source_manifest_sha256 ~ '^[a-f0-9]{64}$'),
  source_format TEXT NOT NULL CHECK (length(source_format) BETWEEN 1 AND 128),
  source_format_version INTEGER NOT NULL CHECK (source_format_version > 0),
  source_workspace_id UUID NOT NULL,
  source_situation_id UUID NOT NULL,
  declared_origin TEXT NOT NULL CHECK (
    length(declared_origin) BETWEEN 1 AND 2048 AND btrim(declared_origin) <> ''
  ),
  verification_status TEXT NOT NULL CHECK (verification_status = 'STRUCTURALLY_VERIFIED'),
  import_mode TEXT NOT NULL CHECK (import_mode = 'UNTRUSTED'),
  imported_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  imported_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,import_batch_id),
  UNIQUE (workspace_id,source_package_sha256)
);

CREATE INDEX forensic_import_batches_recent
  ON niccolo.forensic_import_batches(workspace_id,imported_at DESC,import_batch_id);

CREATE TABLE niccolo.forensic_import_items (
  workspace_id UUID NOT NULL,
  import_batch_id UUID NOT NULL,
  item_id UUID NOT NULL,
  source_path TEXT NOT NULL CHECK (
    length(source_path) BETWEEN 1 AND 2048 AND btrim(source_path) <> ''
  ),
  source_kind TEXT NOT NULL CHECK (
    source_kind IN ('LOGICAL_ROW','ORIGINAL_SOURCE_BYTES')
  ),
  source_sha256 CHAR(64) NOT NULL CHECK (source_sha256 ~ '^[a-f0-9]{64}$'),
  source_byte_length BIGINT NOT NULL CHECK (source_byte_length >= 0),
  origin_class TEXT NOT NULL CHECK (
    origin_class IN (
      'IMPORTED_HUMAN_ASSERTION',
      'IMPORTED_MACHINE_OUTPUT',
      'IMPORTED_CONFIGURATION_ASSERTION',
      'IMPORTED_SOURCE_ARTIFACT',
      'IMPORTED_IDENTITY_ASSERTION',
      'IMPORTED_HISTORICAL_STATE'
    )
  ),
  source_table TEXT,
  source_row_index BIGINT CHECK (source_row_index IS NULL OR source_row_index >= 0),
  original_document_id UUID,
  payload_json JSONB,
  payload_bytes BYTEA,
  current_credentials_establish_truth BOOLEAN NOT NULL DEFAULT FALSE
    CHECK (current_credentials_establish_truth = FALSE),
  documentary_evidence BOOLEAN NOT NULL DEFAULT FALSE
    CHECK (documentary_evidence = FALSE),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,import_batch_id,item_id),
  UNIQUE (workspace_id,import_batch_id,source_path),
  FOREIGN KEY (workspace_id,import_batch_id)
    REFERENCES niccolo.forensic_import_batches(workspace_id,import_batch_id),
  CHECK (
    (source_kind='LOGICAL_ROW' AND payload_json IS NOT NULL AND payload_bytes IS NULL
      AND source_table IS NOT NULL AND source_row_index IS NOT NULL
      AND original_document_id IS NULL)
    OR
    (source_kind='ORIGINAL_SOURCE_BYTES' AND payload_json IS NULL AND payload_bytes IS NOT NULL
      AND source_table IS NULL AND source_row_index IS NULL
      AND original_document_id IS NOT NULL)
  )
);

CREATE INDEX forensic_import_items_by_class
  ON niccolo.forensic_import_items(workspace_id,import_batch_id,origin_class,source_path);

GRANT SELECT,INSERT ON niccolo.forensic_import_batches TO niccolo_app;
GRANT SELECT,INSERT ON niccolo.forensic_import_items TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['forensic_import_batches','forensic_import_items'] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
 END LOOP;
END $$;

COMMIT;
