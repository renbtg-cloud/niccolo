-- Keep the API readiness invariant uniform for the integration-boundary tables.
-- DELETE remains ungranted; the restrictive gate exists so every workspace-scoped
-- table has the same fail-closed RLS policy shape.
BEGIN;

CREATE POLICY api_write_delete_gate
  ON niccolo.source_operational_events
  AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

CREATE POLICY api_write_delete_gate
  ON niccolo.situation_purpose_events
  AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
