-- Complete the API's four-gate readiness contract without granting mutation.
-- The recommendation and decision ledgers remain append-only.
BEGIN;
CREATE POLICY api_write_update_gate ON niccolo.epistemic_action_recommendations AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false);
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_action_recommendations AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false);
CREATE POLICY api_write_update_gate ON niccolo.epistemic_action_decisions AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false);
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_action_decisions AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false);
COMMIT;
