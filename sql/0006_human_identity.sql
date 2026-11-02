-- v0.8: human-confirmed, reversible identity assignments. Never infer identities
-- merely from display names. A binding belongs to one original document.
BEGIN;
CREATE TABLE niccolo.identity_subjects (
 workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
 person_id UUID NOT NULL,
 display_label TEXT NOT NULL CHECK(length(display_label) BETWEEN 1 AND 512),
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(workspace_id,person_id)
);
CREATE TABLE niccolo.identity_events (
 workspace_id UUID NOT NULL,
 event_id UUID NOT NULL,
 event_no BIGINT GENERATED ALWAYS AS IDENTITY,
 document_id UUID NOT NULL,
 speaker_label TEXT NOT NULL CHECK(length(speaker_label) BETWEEN 1 AND 512),
 operation TEXT NOT NULL CHECK(operation IN ('ASSIGN','CLEAR')),
 person_id UUID,
 human_reason TEXT NOT NULL CHECK(length(human_reason) BETWEEN 1 AND 2048),
 actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
 recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(workspace_id,event_id),
 UNIQUE(workspace_id,event_no),
 FOREIGN KEY(workspace_id,document_id) REFERENCES niccolo.source_documents(workspace_id,document_id),
 FOREIGN KEY(workspace_id,person_id) REFERENCES niccolo.identity_subjects(workspace_id,person_id),
 CHECK((operation='ASSIGN' AND person_id IS NOT NULL) OR (operation='CLEAR' AND person_id IS NULL))
);
CREATE INDEX identity_events_by_speaker ON niccolo.identity_events
 (workspace_id,document_id,speaker_label,event_no DESC);
CREATE OR REPLACE FUNCTION niccolo.validate_identity_event() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
BEGIN
 IF NOT EXISTS (SELECT 1 FROM niccolo.source_fragments f WHERE
   f.workspace_id=NEW.workspace_id AND f.document_id=NEW.document_id AND f.speaker_label=NEW.speaker_label) THEN
   RAISE EXCEPTION 'speaker label not present in supplied source document' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER identity_speaker_must_exist BEFORE INSERT ON niccolo.identity_events
 FOR EACH ROW EXECUTE FUNCTION niccolo.validate_identity_event();
GRANT SELECT, INSERT ON niccolo.identity_subjects, niccolo.identity_events TO niccolo_app;
GRANT USAGE, SELECT ON SEQUENCE niccolo.identity_events_event_no_seq TO niccolo_app;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['identity_subjects','identity_events'] LOOP
   EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
   EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
   EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
 END LOOP;
END $$;
-- The application receives the principal identifier only after token, membership,
-- capability and expiration have been checked in one trusted definer function.
CREATE OR REPLACE FUNCTION niccolo_auth.authorized_principal(token_digest TEXT, target_workspace UUID, capability TEXT)
 RETURNS UUID LANGUAGE sql STABLE SECURITY DEFINER SET search_path=pg_catalog,niccolo_auth AS $$
 SELECT p.principal_id FROM niccolo_auth.tokens t
 JOIN niccolo_auth.principals p ON p.principal_id=t.principal_id
 JOIN niccolo_auth.memberships m ON m.principal_id=p.principal_id
 WHERE token_digest ~ '^[a-f0-9]{64}$' AND t.token_sha256=token_digest
 AND t.revoked_at IS NULL AND t.expires_at>now() AND p.active
 AND m.revoked_at IS NULL AND m.workspace_id=target_workspace
 AND ((capability='read' AND (m.can_read OR m.can_write)) OR (capability='write' AND m.can_write))
 LIMIT 1;
$$;
REVOKE ALL ON FUNCTION niccolo_auth.authorized_principal(text,uuid,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION niccolo_auth.authorized_principal(text,uuid,text) TO niccolo_app;
COMMIT;
