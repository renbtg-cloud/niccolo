-- v0.7: server-verified bearer-token authorization; no client-asserted workspace identity.
-- Apply as a privileged schema owner, NEVER with a request-handling database role.
BEGIN;
CREATE SCHEMA IF NOT EXISTS niccolo_auth;
REVOKE ALL ON SCHEMA niccolo_auth FROM PUBLIC;
CREATE TABLE IF NOT EXISTS niccolo_auth.principals (
  principal_id UUID PRIMARY KEY,
  active BOOLEAN NOT NULL DEFAULT TRUE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS niccolo_auth.tokens (
  token_sha256 CHAR(64) PRIMARY KEY CHECK (token_sha256 ~ '^[a-f0-9]{64}$'),
  principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK (expires_at > created_at)
);
CREATE INDEX IF NOT EXISTS token_by_principal ON niccolo_auth.tokens(principal_id);
CREATE TABLE IF NOT EXISTS niccolo_auth.memberships (
  principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  can_read BOOLEAN NOT NULL DEFAULT FALSE,
  can_write BOOLEAN NOT NULL DEFAULT FALSE,
  revoked_at TIMESTAMPTZ,
  PRIMARY KEY (principal_id, workspace_id),
  CHECK (can_read OR can_write)
);
REVOKE ALL ON ALL TABLES IN SCHEMA niccolo_auth FROM PUBLIC;
-- A request-handling role receives EXECUTE ONLY, never SELECT on token hashes or memberships.
CREATE OR REPLACE FUNCTION niccolo_auth.authorize(token_digest text, target_workspace uuid, capability text)
RETURNS BOOLEAN LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, niccolo_auth AS $$
  SELECT EXISTS (
    SELECT 1 FROM niccolo_auth.tokens t
    JOIN niccolo_auth.principals p ON p.principal_id=t.principal_id
    JOIN niccolo_auth.memberships m ON m.principal_id=p.principal_id
    WHERE t.token_sha256=token_digest
      AND token_digest ~ '^[a-f0-9]{64}$'
      AND p.active AND t.revoked_at IS NULL AND t.expires_at > now()
      AND m.revoked_at IS NULL AND m.workspace_id=target_workspace
      AND ((capability='read' AND (m.can_read OR m.can_write))
           OR (capability='write' AND m.can_write))
  );
$$;
REVOKE ALL ON FUNCTION niccolo_auth.authorize(text,uuid,text) FROM PUBLIC;
GRANT USAGE ON SCHEMA niccolo_auth TO niccolo_app;
GRANT EXECUTE ON FUNCTION niccolo_auth.authorize(text,uuid,text) TO niccolo_app;
COMMIT;
