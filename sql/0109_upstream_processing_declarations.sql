-- Attributed, append-only declarations about processing performed upstream of Niccolò.
-- A declaration records what an integration says it did. It is not verification
-- that the processing was correct, complete, unbiased, or faithfully executed.
BEGIN;

CREATE TABLE niccolo.upstream_processing_declarations (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  declaration_id UUID NOT NULL,
  processing_kind TEXT NOT NULL CHECK (
    processing_kind IN (
      'FILTER',
      'DEDUPLICATION',
      'SAMPLING',
      'SOURCE_SELECTION',
      'EXCLUSION',
      'TRANSFORMATION',
      'AGGREGATION'
    )
  ),
  scope_locator TEXT NOT NULL CHECK (
    length(scope_locator) BETWEEN 1 AND 1024
    AND btrim(scope_locator) <> ''
    AND scope_locator !~ '[[:cntrl:]]'
  ),
  declaration_text TEXT NOT NULL CHECK (
    length(declaration_text) BETWEEN 1 AND 4096
    AND btrim(declaration_text) <> ''
  ),
  source_document_id UUID,
  recorded_by_principal_id UUID NOT NULL
    REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  PRIMARY KEY (workspace_id,declaration_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,source_document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id)
);

CREATE INDEX upstream_processing_declarations_by_scope
  ON niccolo.upstream_processing_declarations(workspace_id,event_no);

CREATE INDEX upstream_processing_declarations_by_document
  ON niccolo.upstream_processing_declarations(workspace_id,source_document_id,event_no)
  WHERE source_document_id IS NOT NULL;

GRANT SELECT,INSERT ON niccolo.upstream_processing_declarations TO niccolo_app;
GRANT SELECT,INSERT ON niccolo.upstream_processing_declarations TO niccolo_api;
GRANT USAGE,SELECT ON SEQUENCE niccolo.upstream_processing_declarations_event_no_seq TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.upstream_processing_declarations_event_no_seq TO niccolo_api;

ALTER TABLE niccolo.upstream_processing_declarations ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.upstream_processing_declarations FORCE ROW LEVEL SECURITY;

CREATE POLICY workspace_filter ON niccolo.upstream_processing_declarations
  USING (
    workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid
  )
  WITH CHECK (
    workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid
  );

CREATE POLICY api_read_gate ON niccolo.upstream_processing_declarations
  AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));

CREATE POLICY api_write_insert_gate ON niccolo.upstream_processing_declarations
  AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
