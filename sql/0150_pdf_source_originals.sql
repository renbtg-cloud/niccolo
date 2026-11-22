BEGIN;
ALTER TABLE niccolo.source_documents DROP CONSTRAINT source_documents_format_check;
ALTER TABLE niccolo.source_documents ADD CONSTRAINT source_documents_format_check
    CHECK (format IN ('EML', 'WHATSAPP_EXPORT', 'PDF'));
COMMIT;
