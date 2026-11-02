-- Niccolò v0.6: enforce original-source byte boundaries and immutability.
-- Transactional migration, repeatable on the disposable test DB.
BEGIN;
-- Row-locking SELECT FOR UPDATE requires UPDATE privilege. The role can
-- request a lock, but an attempted byte_length change is rejected below.
GRANT UPDATE (byte_length) ON niccolo.source_documents TO niccolo_app;
CREATE OR REPLACE FUNCTION niccolo.validate_source_fragment() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE max_end bigint;
BEGIN
  -- Lock the parent row: concurrent fragment writes for one document cannot
  -- evade the overlap check via racing snapshots.
  SELECT byte_length INTO max_end FROM niccolo.source_documents
    WHERE workspace_id=NEW.workspace_id AND document_id=NEW.document_id FOR UPDATE;
  IF max_end IS NULL OR NEW.byte_end > max_end THEN
    RAISE EXCEPTION 'fragment byte range exceeds source document length' USING ERRCODE='23514';
  END IF;
  IF EXISTS (
    SELECT 1 FROM niccolo.source_fragments f
    WHERE f.workspace_id=NEW.workspace_id AND f.document_id=NEW.document_id
    AND f.ordinal<>NEW.ordinal
    AND f.byte_start<NEW.byte_end AND NEW.byte_start<f.byte_end
  ) THEN
    RAISE EXCEPTION 'source fragment byte ranges overlap' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS check_source_fragment ON niccolo.source_fragments;
CREATE TRIGGER check_source_fragment BEFORE INSERT OR UPDATE ON niccolo.source_fragments
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_source_fragment();

CREATE OR REPLACE FUNCTION niccolo.source_document_metadata_immutable() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
BEGIN
  IF NEW.format IS DISTINCT FROM OLD.format
    OR NEW.locator IS DISTINCT FROM OLD.locator
    OR NEW.sha256 IS DISTINCT FROM OLD.sha256
    OR NEW.byte_length IS DISTINCT FROM OLD.byte_length THEN
    RAISE EXCEPTION 'original source document metadata is immutable' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
DROP TRIGGER IF EXISTS protect_source_document_metadata ON niccolo.source_documents;
CREATE TRIGGER protect_source_document_metadata BEFORE UPDATE ON niccolo.source_documents
  FOR EACH ROW EXECUTE FUNCTION niccolo.source_document_metadata_immutable();
COMMIT;
