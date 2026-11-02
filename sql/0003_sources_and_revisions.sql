-- Niccolò v0.6: source provenance, idempotent fragment imports, graph revisions.
-- Apply with schema-owner privileges after 0001 and 0002.  No source bytes or credentials
-- are stored by this migration. The source artifact itself must be retained in an
-- appropriately access-controlled content store by a separate, trusted layer.
BEGIN;
CREATE TABLE IF NOT EXISTS niccolo.source_documents (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  document_id UUID NOT NULL,
  format TEXT NOT NULL CHECK (format IN ('EML','WHATSAPP_EXPORT')),
  locator TEXT NOT NULL CHECK (length(locator) BETWEEN 1 AND 2048),
  sha256 TEXT NOT NULL CHECK (sha256 ~ '^[a-f0-9]{64}$'),
  byte_length BIGINT NOT NULL CHECK (byte_length BETWEEN 1 AND 4194304),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (workspace_id, document_id)
);
CREATE TABLE IF NOT EXISTS niccolo.source_fragments (
  workspace_id UUID NOT NULL,
  document_id UUID NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
  evidence_id UUID NOT NULL,
  byte_start INTEGER NOT NULL CHECK (byte_start >= 0),
  byte_end INTEGER NOT NULL CHECK (byte_end > byte_start),
  speaker_label TEXT NOT NULL CHECK (length(speaker_label) BETWEEN 1 AND 512),
  PRIMARY KEY (workspace_id, document_id, ordinal),
  UNIQUE (workspace_id, evidence_id),
  FOREIGN KEY (workspace_id, document_id) REFERENCES niccolo.source_documents(workspace_id, document_id),
  FOREIGN KEY (workspace_id, evidence_id) REFERENCES niccolo.evidence(workspace_id, evidence_id),
  CHECK (byte_end <= 4194304)
);
CREATE INDEX IF NOT EXISTS source_fragments_by_document
    ON niccolo.source_fragments(workspace_id, document_id, ordinal);
-- One monotonically increasing revision per workspace, advanced by every evidence
-- and derivation mutation, including manual revocations through SQL.
CREATE TABLE IF NOT EXISTS niccolo.graph_revisions (
  workspace_id UUID PRIMARY KEY REFERENCES niccolo.workspaces(workspace_id),
  revision BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
CREATE OR REPLACE FUNCTION niccolo.advance_graph_revision() RETURNS TRIGGER
LANGUAGE plpgsql SECURITY INVOKER SET search_path = pg_catalog, niccolo AS $$
DECLARE ws UUID;
BEGIN
  ws := CASE WHEN TG_OP = 'DELETE' THEN OLD.workspace_id ELSE NEW.workspace_id END;
  IF TG_OP = 'UPDATE' AND NEW.workspace_id <> OLD.workspace_id THEN
    RAISE EXCEPTION 'workspace change forbidden for tracked evidence';
  END IF;
  INSERT INTO niccolo.graph_revisions(workspace_id,revision) VALUES (ws,1)
  ON CONFLICT(workspace_id) DO UPDATE SET revision = niccolo.graph_revisions.revision + 1;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END $$;
DROP TRIGGER IF EXISTS evidence_graph_revision ON niccolo.evidence;
CREATE TRIGGER evidence_graph_revision AFTER INSERT OR UPDATE OR DELETE ON niccolo.evidence
FOR EACH ROW EXECUTE FUNCTION niccolo.advance_graph_revision();
DROP TRIGGER IF EXISTS derivation_graph_revision ON niccolo.evidence_derivation;
CREATE TRIGGER derivation_graph_revision AFTER INSERT OR UPDATE OR DELETE ON niccolo.evidence_derivation
FOR EACH ROW EXECUTE FUNCTION niccolo.advance_graph_revision();
GRANT SELECT, INSERT, UPDATE ON niccolo.graph_revisions TO niccolo_app;
GRANT SELECT, INSERT ON niccolo.source_documents, niccolo.source_fragments TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['source_documents','source_fragments','graph_revisions'] LOOP
   EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
   EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
   EXECUTE format('DROP POLICY IF EXISTS workspace_filter ON niccolo.%I',t);
   EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
 END LOOP;
END $$;
COMMIT;
