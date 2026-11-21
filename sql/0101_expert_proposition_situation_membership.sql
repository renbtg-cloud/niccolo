-- Situation membership for persistent expert-authored propositions.
-- Proposition semantic identity remains workspace-scoped; use in a Situation is explicit.
BEGIN;

CREATE TABLE niccolo.epistemic_situation_propositions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  proposition_id UUID NOT NULL,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,proposition_id),
  FOREIGN KEY (workspace_id,situation_id)
    REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,proposition_id)
    REFERENCES niccolo.epistemic_propositions(workspace_id,proposition_id)
);
CREATE INDEX epistemic_situation_propositions_by_proposition
  ON niccolo.epistemic_situation_propositions(workspace_id,proposition_id,situation_id);

GRANT SELECT,INSERT ON niccolo.epistemic_situation_propositions TO niccolo_app;

ALTER TABLE niccolo.epistemic_situation_propositions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_situation_propositions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_situation_propositions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_situation_propositions
  AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_situation_propositions
  AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_situation_propositions
  AS RESTRICTIVE FOR UPDATE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_situation_propositions
  AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
