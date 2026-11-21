-- Keep the uniform fail-closed API RLS contract for the append-only
-- reassessment review ledger. UPDATE/DELETE remain ungranted; the restrictive
-- gates exist so readiness can verify one invariant for every workspace table.
BEGIN;

CREATE POLICY api_write_update_gate ON niccolo.epistemic_reassessment_review_events
  AS RESTRICTIVE FOR UPDATE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_reassessment_review_events
  AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
