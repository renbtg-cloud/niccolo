-- Niccolò MVP: retain byte-exact source originals in authoritative PostgreSQL state.
-- Existing pre-migration documents have no payload until the same validated source
-- is re-uploaded. Never fabricate missing original bytes from parsed fragments.
BEGIN;
CREATE TABLE niccolo.source_payloads (
  workspace_id UUID NOT NULL,
  document_id UUID NOT NULL,
  original_bytes BYTEA NOT NULL,
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, document_id),
  FOREIGN KEY (workspace_id, document_id)
    REFERENCES niccolo.source_documents(workspace_id, document_id),
  CHECK (octet_length(original_bytes) BETWEEN 1 AND 4194304)
);

CREATE OR REPLACE FUNCTION niccolo.validate_source_payload() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE original_length BIGINT;
BEGIN
  SELECT byte_length INTO original_length
    FROM niccolo.source_documents
   WHERE workspace_id=NEW.workspace_id AND document_id=NEW.document_id
   FOR UPDATE;
  IF original_length IS NULL OR octet_length(NEW.original_bytes) <> original_length THEN
    RAISE EXCEPTION 'source payload does not match original declared length'
      USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_source_payload
  BEFORE INSERT ON niccolo.source_payloads
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_source_payload();

CREATE OR REPLACE FUNCTION niccolo.block_source_payload_mutation() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
BEGIN
  RAISE EXCEPTION 'original source payloads are immutable' USING ERRCODE='23514';
END $$;
CREATE TRIGGER block_source_payload_mutation
  BEFORE UPDATE OR DELETE ON niccolo.source_payloads
  FOR EACH ROW EXECUTE FUNCTION niccolo.block_source_payload_mutation();

GRANT SELECT, INSERT ON niccolo.source_payloads TO niccolo_app;
ALTER TABLE niccolo.source_payloads ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.source_payloads FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.source_payloads
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.source_payloads
  AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.source_payloads
  AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.source_payloads
  AS RESTRICTIVE FOR UPDATE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.source_payloads
  AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
