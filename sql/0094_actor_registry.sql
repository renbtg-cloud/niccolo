-- Generic observable actors. Human/software/mixed/organization is descriptive metadata,
-- not a gate for investigation and not an assertion of mind or consciousness.
BEGIN;

CREATE TABLE niccolo.actors (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  actor_id UUID NOT NULL,
  display_label TEXT NOT NULL CHECK (length(display_label) BETWEEN 1 AND 512 AND btrim(display_label) <> ''),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,actor_id)
);

CREATE TABLE niccolo.actor_kind_events (
  workspace_id UUID NOT NULL,
  actor_id UUID NOT NULL,
  event_id UUID NOT NULL,
  operation TEXT NOT NULL CHECK (operation IN ('SET','CLEAR')),
  actor_kind TEXT CHECK (actor_kind IS NULL OR actor_kind IN ('UNKNOWN','HUMAN','SOFTWARE','MIXED','ORGANIZATION')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  PRIMARY KEY (workspace_id,actor_id,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  CHECK ((operation='SET' AND actor_kind IS NOT NULL) OR (operation='CLEAR' AND actor_kind IS NULL))
);

CREATE TABLE niccolo.actor_binding_events (
  workspace_id UUID NOT NULL,
  document_id UUID NOT NULL,
  speaker_label TEXT NOT NULL CHECK (length(speaker_label) BETWEEN 1 AND 512 AND btrim(speaker_label) <> ''),
  event_id UUID NOT NULL,
  operation TEXT NOT NULL CHECK (operation IN ('ASSIGN','CLEAR')),
  actor_id UUID,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  PRIMARY KEY (workspace_id,document_id,speaker_label,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,document_id) REFERENCES niccolo.source_documents(workspace_id,document_id),
  FOREIGN KEY (workspace_id,actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  CHECK ((operation='ASSIGN' AND actor_id IS NOT NULL) OR (operation='CLEAR' AND actor_id IS NULL))
);

CREATE OR REPLACE FUNCTION niccolo.validate_actor_binding() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
BEGIN
 IF NOT EXISTS (
   SELECT 1 FROM niccolo.source_fragments f
    WHERE f.workspace_id=NEW.workspace_id
      AND f.document_id=NEW.document_id
      AND f.speaker_label=NEW.speaker_label
 ) THEN
   RAISE EXCEPTION 'speaker label not present in supplied source document' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;

CREATE TRIGGER actor_binding_speaker_must_exist
 BEFORE INSERT ON niccolo.actor_binding_events
 FOR EACH ROW EXECUTE FUNCTION niccolo.validate_actor_binding();

CREATE INDEX actor_kind_events_latest
 ON niccolo.actor_kind_events(workspace_id,actor_id,event_no DESC);
CREATE INDEX actor_binding_events_latest
 ON niccolo.actor_binding_events(workspace_id,document_id,speaker_label,event_no DESC);

GRANT SELECT,INSERT ON niccolo.actors,niccolo.actor_kind_events,niccolo.actor_binding_events TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.actor_kind_events_event_no_seq,
 niccolo.actor_binding_events_event_no_seq TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['actors','actor_kind_events','actor_binding_events'] LOOP
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
