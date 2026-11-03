-- v0.10: a restrictive *second* RLS gate for dedicated API roles.
-- A mutable workspace GUC alone is no longer sufficient for API DB access.
-- This role should be used for network-facing application queries. Legacy
-- test/service role is deliberately NOT secured by this extra policy.
BEGIN;
DO $$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='niccolo_api') THEN
   CREATE ROLE niccolo_api NOLOGIN INHERIT;
 END IF;
END $$;
-- Inheritance is allowed, SET ROLE to the unrestricted legacy app role is not.
GRANT niccolo_app TO niccolo_api WITH INHERIT TRUE, SET FALSE;
GRANT USAGE ON SCHEMA niccolo_auth TO niccolo_api;
CREATE OR REPLACE FUNCTION niccolo_auth.rls_credential_allows(target_workspace UUID, required_capability TEXT)
RETURNS BOOLEAN LANGUAGE sql STABLE SECURITY DEFINER
SET search_path=pg_catalog,niccolo_auth AS $$
 SELECT EXISTS (
   SELECT 1 FROM niccolo_auth.tokens t
   JOIN niccolo_auth.principals p ON p.principal_id=t.principal_id
   JOIN niccolo_auth.memberships m ON m.principal_id=p.principal_id
   WHERE t.token_sha256=current_setting('niccolo.auth_digest',true)
     AND current_setting('niccolo.auth_digest',true) ~ '^[a-f0-9]{64}$'
     AND t.revoked_at IS NULL AND t.expires_at>now() AND p.active
     AND m.revoked_at IS NULL AND m.workspace_id=target_workspace
     AND target_workspace::text=current_setting('niccolo.workspace_id',true)
     AND ((required_capability='read' AND (m.can_read OR m.can_write)) OR
          (required_capability='write' AND m.can_write))
 )
$$;
REVOKE ALL ON FUNCTION niccolo_auth.rls_credential_allows(uuid,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION niccolo_auth.rls_credential_allows(uuid,text) TO niccolo_api;
DO $$ DECLARE t RECORD; BEGIN
 FOR t IN SELECT cls.relname
   FROM pg_class cls JOIN pg_namespace ns ON ns.oid=cls.relnamespace
   JOIN pg_attribute a ON a.attrelid=cls.oid
   WHERE ns.nspname='niccolo' AND cls.relkind='r'
   AND a.attname='workspace_id' AND NOT a.attisdropped
 LOOP
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t.relname);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t.relname);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t.relname);
  EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t.relname);
 END LOOP;
END $$;
COMMIT;
