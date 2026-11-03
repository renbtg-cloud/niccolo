-- Human-authored, paired evidence review hypotheses. Not model conclusions.
-- Both evidence FKs include workspace_id, preventing cross-workspace links.
BEGIN;
CREATE TABLE niccolo.manual_hypotheses (
 workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
 hypothesis_id UUID NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('APPARENT_CONTRADICTION','ATTRIBUTION_QUESTION','TIMING_QUESTION')),
 left_evidence_id UUID NOT NULL,
 right_evidence_id UUID NOT NULL,
 human_note TEXT NOT NULL CHECK(length(human_note) BETWEEN 1 AND 2048 AND btrim(human_note)<>''),
 actor_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
 created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,hypothesis_id),
 FOREIGN KEY(workspace_id,left_evidence_id) REFERENCES niccolo.evidence(workspace_id,evidence_id),
 FOREIGN KEY(workspace_id,right_evidence_id) REFERENCES niccolo.evidence(workspace_id,evidence_id),
 CHECK(left_evidence_id<>right_evidence_id)
);
CREATE INDEX manual_hypotheses_paged ON niccolo.manual_hypotheses(workspace_id,hypothesis_id);
CREATE TRIGGER manual_hypotheses_immutable BEFORE UPDATE OR DELETE ON niccolo.manual_hypotheses
 FOR EACH ROW EXECUTE FUNCTION niccolo.reject_feedback_rewrite();
GRANT SELECT,INSERT ON niccolo.manual_hypotheses TO niccolo_app;
ALTER TABLE niccolo.manual_hypotheses ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.manual_hypotheses FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.manual_hypotheses
 USING(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
 WITH CHECK(workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
-- Database-internal restrictive second gate for the API role, not GUC-only access.
CREATE POLICY api_read_gate ON niccolo.manual_hypotheses AS RESTRICTIVE
 FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.manual_hypotheses AS RESTRICTIVE
 FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.manual_hypotheses AS RESTRICTIVE
 FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
 WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.manual_hypotheses AS RESTRICTIVE
 FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
