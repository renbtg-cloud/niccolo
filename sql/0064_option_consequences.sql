-- v0.151-v0.155: consequences are explicit projections, never facts or automatic recommendations.
BEGIN;
CREATE TABLE niccolo.situation_analysis_option_consequences (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,analysis_id UUID NOT NULL,decision_id UUID NOT NULL,option_id UUID NOT NULL,consequence_id UUID NOT NULL,
 consequence_text TEXT NOT NULL CHECK(length(consequence_text) BETWEEN 1 AND 4096 AND btrim(consequence_text)<>''),
 uncertainty_note TEXT NOT NULL CHECK(length(uncertainty_note) BETWEEN 1 AND 4096 AND btrim(uncertainty_note)<>''),
 recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,analysis_id,decision_id,option_id,consequence_id),
 FOREIGN KEY(workspace_id,situation_id,analysis_id,decision_id,option_id) REFERENCES niccolo.situation_analysis_decision_options(workspace_id,situation_id,analysis_id,decision_id,option_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_option_consequences TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_option_consequences ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.situation_analysis_option_consequences FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_option_consequences USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_option_consequences AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_option_consequences AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
