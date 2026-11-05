-- v0.16: bounded persistent Situations and immutable reasoning snapshots.
-- Human contributions remain attributed context/hypotheses/preferences; they are not evidence.
BEGIN;
CREATE TABLE niccolo.situations (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  situation_id UUID NOT NULL,
  focus TEXT NOT NULL CHECK (length(focus) BETWEEN 1 AND 4096 AND btrim(focus) <> ''),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  current_revision BIGINT NOT NULL DEFAULT 0 CHECK (current_revision >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id)
);

CREATE TABLE niccolo.situation_sources (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  document_id UUID NOT NULL,
  added_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  added_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, document_id),
  FOREIGN KEY (workspace_id, situation_id)
    REFERENCES niccolo.situations(workspace_id, situation_id),
  FOREIGN KEY (workspace_id, document_id)
    REFERENCES niccolo.source_documents(workspace_id, document_id)
);

CREATE TABLE niccolo.situation_contributions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  contribution_id UUID NOT NULL,
  author_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  kind TEXT NOT NULL CHECK (kind IN ('USER_REPORTED','HYPOTHETICAL','INVESTIGATIVE_PREFERENCE')),
  contribution_text TEXT NOT NULL CHECK (length(contribution_text) BETWEEN 1 AND 4096 AND btrim(contribution_text) <> ''),
  scope TEXT NOT NULL DEFAULT 'SITUATION' CHECK (scope = 'SITUATION'),
  related_evidence_id UUID,
  status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE','RETRACTED','CORRECTED')),
  corrected_by UUID,
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  changed_at TIMESTAMPTZ,
  PRIMARY KEY (workspace_id, situation_id, contribution_id),
  FOREIGN KEY (workspace_id, situation_id)
    REFERENCES niccolo.situations(workspace_id, situation_id),
  FOREIGN KEY (workspace_id, related_evidence_id)
    REFERENCES niccolo.evidence(workspace_id, evidence_id),
  FOREIGN KEY (workspace_id, situation_id, corrected_by)
    REFERENCES niccolo.situation_contributions(workspace_id, situation_id, contribution_id)
    DEFERRABLE INITIALLY DEFERRED,
  CHECK ((status = 'CORRECTED' AND corrected_by IS NOT NULL)
      OR (status <> 'CORRECTED' AND corrected_by IS NULL))
);
CREATE INDEX situation_contributions_by_status
  ON niccolo.situation_contributions(workspace_id, situation_id, status, contribution_id);

CREATE TABLE niccolo.situation_revisions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL CHECK (revision >= 1),
  focus TEXT NOT NULL CHECK (length(focus) BETWEEN 1 AND 4096 AND btrim(focus) <> ''),
  fingerprint CHAR(64) NOT NULL CHECK (fingerprint ~ '^[a-f0-9]{64}$'),
  actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  cause TEXT NOT NULL CHECK (cause IN ('CREATED','FOCUS_CHANGED','SOURCE_ADDED','CONTRIBUTION_ADDED','CORRECTED','RETRACTED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, revision),
  FOREIGN KEY (workspace_id, situation_id)
    REFERENCES niccolo.situations(workspace_id, situation_id)
);

CREATE TABLE niccolo.situation_revision_sources (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL,
  document_id UUID NOT NULL,
  PRIMARY KEY (workspace_id, situation_id, revision, document_id),
  FOREIGN KEY (workspace_id, situation_id, revision)
    REFERENCES niccolo.situation_revisions(workspace_id, situation_id, revision),
  FOREIGN KEY (workspace_id, document_id)
    REFERENCES niccolo.source_documents(workspace_id, document_id)
);

CREATE TABLE niccolo.situation_revision_contributions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  revision BIGINT NOT NULL,
  contribution_id UUID NOT NULL,
  PRIMARY KEY (workspace_id, situation_id, revision, contribution_id),
  FOREIGN KEY (workspace_id, situation_id, revision)
    REFERENCES niccolo.situation_revisions(workspace_id, situation_id, revision),
  FOREIGN KEY (workspace_id, situation_id, contribution_id)
    REFERENCES niccolo.situation_contributions(workspace_id, situation_id, contribution_id)
);

GRANT SELECT, INSERT ON niccolo.situations TO niccolo_app;
GRANT UPDATE (focus, current_revision, updated_at) ON niccolo.situations TO niccolo_app;
GRANT SELECT, INSERT ON niccolo.situation_sources TO niccolo_app;
GRANT SELECT, INSERT ON niccolo.situation_contributions TO niccolo_app;
GRANT UPDATE (status, corrected_by, changed_at) ON niccolo.situation_contributions TO niccolo_app;
GRANT SELECT, INSERT ON niccolo.situation_revisions,
  niccolo.situation_revision_sources, niccolo.situation_revision_contributions TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY[
    'situations','situation_sources','situation_contributions','situation_revisions',
    'situation_revision_sources','situation_revision_contributions'
  ] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)', t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))', t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
  END LOOP;
END $$;
COMMIT;
