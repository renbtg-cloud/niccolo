-- v0.181: every workspace-scoped table exposed to the API keeps the same
-- four restrictive niccolo_api gates, even when the table is append-only and
-- UPDATE/DELETE are intentionally not granted. Readiness can therefore verify
-- one uniform fail-closed RLS boundary instead of inferring mutability.
BEGIN;
DO $$
DECLARE
  r RECORD;
BEGIN
  FOR r IN
    SELECT c.relname
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    JOIN pg_attribute a ON a.attrelid = c.oid
    WHERE n.nspname = 'niccolo'
      AND c.relkind = 'r'
      AND a.attname = 'workspace_id'
      AND NOT a.attisdropped
      AND c.relrowsecurity
      AND c.relforcerowsecurity
    ORDER BY c.relname
  LOOP
    IF NOT EXISTS (
      SELECT 1 FROM pg_policy p
      WHERE p.polrelid = format('niccolo.%I', r.relname)::regclass
        AND p.polname = 'api_read_gate'
        AND NOT p.polpermissive
        AND 'niccolo_api'::regrole::oid = ANY(p.polroles)
    ) THEN
      EXECUTE format(
        'CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',
        r.relname
      );
    END IF;
    IF NOT EXISTS (
      SELECT 1 FROM pg_policy p
      WHERE p.polrelid = format('niccolo.%I', r.relname)::regclass
        AND p.polname = 'api_write_insert_gate'
        AND NOT p.polpermissive
        AND 'niccolo_api'::regrole::oid = ANY(p.polroles)
    ) THEN
      EXECUTE format(
        'CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',
        r.relname
      );
    END IF;
    IF NOT EXISTS (
      SELECT 1 FROM pg_policy p
      WHERE p.polrelid = format('niccolo.%I', r.relname)::regclass
        AND p.polname = 'api_write_update_gate'
        AND NOT p.polpermissive
        AND 'niccolo_api'::regrole::oid = ANY(p.polroles)
    ) THEN
      EXECUTE format(
        'CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',
        r.relname
      );
    END IF;
    IF NOT EXISTS (
      SELECT 1 FROM pg_policy p
      WHERE p.polrelid = format('niccolo.%I', r.relname)::regclass
        AND p.polname = 'api_write_delete_gate'
        AND NOT p.polpermissive
        AND 'niccolo_api'::regrole::oid = ANY(p.polroles)
    ) THEN
      EXECUTE format(
        'CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',
        r.relname
      );
    END IF;
  END LOOP;
END $$;
COMMIT;
