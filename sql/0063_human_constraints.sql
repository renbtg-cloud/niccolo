-- v0.146-v0.150: human constraints and append-only waiver/reinstatement history.
BEGIN;
CREATE TABLE niccolo.situation_human_constraints (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,constraint_id UUID NOT NULL,
 constraint_kind TEXT NOT NULL CHECK (constraint_kind IN ('MUST','MUST_NOT','PREFERENCE')),
 constraint_text TEXT NOT NULL CHECK (length(constraint_text) BETWEEN 1 AND 4096 AND btrim(constraint_text)<>''),
 declared_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,constraint_id),FOREIGN KEY(workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id)
);
CREATE TABLE niccolo.situation_human_constraint_events (
 workspace_id UUID NOT NULL,situation_id UUID NOT NULL,constraint_id UUID NOT NULL,event_id UUID NOT NULL,
 event_kind TEXT NOT NULL CHECK (event_kind IN ('WAIVED','REINSTATED')),note TEXT NOT NULL CHECK(length(note) BETWEEN 1 AND 4096 AND btrim(note)<>''),
 recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(workspace_id,situation_id,constraint_id,event_id),FOREIGN KEY(workspace_id,situation_id,constraint_id) REFERENCES niccolo.situation_human_constraints(workspace_id,situation_id,constraint_id)
);
GRANT SELECT,INSERT ON niccolo.situation_human_constraints,niccolo.situation_human_constraint_events TO niccolo_app;
ALTER TABLE niccolo.situation_human_constraints ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.situation_human_constraints FORCE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_human_constraint_events ENABLE ROW LEVEL SECURITY; ALTER TABLE niccolo.situation_human_constraint_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_human_constraints USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY workspace_filter ON niccolo.situation_human_constraint_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_human_constraints AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_human_constraints AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_read_gate ON niccolo.situation_human_constraint_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_human_constraint_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
