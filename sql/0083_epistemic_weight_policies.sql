-- EPI-008: append-only workspace defaults and per-Situation weight-policy overrides.
-- Policies are heuristics, not posterior truth probabilities.
BEGIN;
CREATE TABLE niccolo.epistemic_weight_policies (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  policy_id UUID NOT NULL,
  situation_id UUID,
  support_multiplier_pct SMALLINT NOT NULL CHECK(support_multiplier_pct BETWEEN 0 AND 200),
  counter_multiplier_pct SMALLINT NOT NULL CHECK(counter_multiplier_pct BETWEEN 0 AND 200),
  per_origin_cap SMALLINT NOT NULL CHECK(per_origin_cap BETWEEN 0 AND 100),
  rationale TEXT NOT NULL CHECK(length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,policy_id),
  FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE INDEX epistemic_weight_policies_by_scope
  ON niccolo.epistemic_weight_policies(workspace_id,situation_id,created_at DESC,policy_id DESC);
GRANT SELECT,INSERT ON niccolo.epistemic_weight_policies TO niccolo_app;
ALTER TABLE niccolo.epistemic_weight_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_weight_policies FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_weight_policies
 USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_weight_policies AS RESTRICTIVE FOR SELECT TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_weight_policies AS RESTRICTIVE FOR INSERT TO niccolo_api
 WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_weight_policies AS RESTRICTIVE FOR UPDATE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_weight_policies AS RESTRICTIVE FOR DELETE TO niccolo_api
 USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
