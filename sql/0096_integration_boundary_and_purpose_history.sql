-- System-integrator operational identity boundary and Situation purpose history.
BEGIN;

CREATE TABLE niccolo.source_operational_events (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  operational_event_id TEXT NOT NULL CHECK (
    length(operational_event_id) BETWEEN 1 AND 512
    AND btrim(operational_event_id) <> ''
  ),
  document_id UUID NOT NULL,
  sha256 CHAR(64) NOT NULL CHECK (sha256 ~ '^[a-f0-9]{64}$'),
  delivery_count BIGINT NOT NULL DEFAULT 1 CHECK (delivery_count >= 1),
  first_seen_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  last_seen_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,operational_event_id),
  FOREIGN KEY (workspace_id,document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id)
);

CREATE INDEX source_operational_events_by_document
  ON niccolo.source_operational_events(workspace_id,document_id);

CREATE TABLE niccolo.situation_purpose_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  purpose_text TEXT NOT NULL CHECK (
    length(purpose_text) BETWEEN 1 AND 4096 AND btrim(purpose_text) <> ''
  ),
  reason TEXT NOT NULL CHECK (
    length(reason) BETWEEN 1 AND 4096 AND btrim(reason) <> ''
  ),
  downstream_use TEXT CHECK (
    downstream_use IS NULL OR
    (length(downstream_use) BETWEEN 1 AND 4096 AND btrim(downstream_use) <> '')
  ),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  PRIMARY KEY (workspace_id,situation_id,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id)
);

CREATE INDEX situation_purpose_events_latest
  ON niccolo.situation_purpose_events(workspace_id,situation_id,event_no DESC);

ALTER TABLE niccolo.situation_analysis_snapshots
  ADD COLUMN purpose_event_no_max BIGINT NOT NULL DEFAULT 0 CHECK (purpose_event_no_max >= 0),
  ADD COLUMN declared_purpose TEXT;

ALTER TABLE niccolo.situation_analysis_snapshots
  ADD CONSTRAINT analysis_declared_purpose_shape CHECK (
    (purpose_event_no_max = 0 AND declared_purpose IS NULL)
    OR
    (purpose_event_no_max > 0
     AND declared_purpose IS NOT NULL
     AND length(declared_purpose) BETWEEN 1 AND 4096
     AND btrim(declared_purpose) <> '')
  );

GRANT SELECT,INSERT,UPDATE ON niccolo.source_operational_events TO niccolo_app;
GRANT SELECT,INSERT ON niccolo.situation_purpose_events TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.situation_purpose_events_event_no_seq TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['source_operational_events','situation_purpose_events'] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
 END LOOP;
END $$;

COMMIT;
