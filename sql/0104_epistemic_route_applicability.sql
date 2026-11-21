-- Applicability is an attributed assessment of a particular authored route,
-- not a truth value for the condition, premises, or conclusion.
BEGIN;
CREATE TABLE niccolo.epistemic_route_applicability_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  condition_object_id UUID NOT NULL,
  assessment TEXT NOT NULL CHECK (assessment IN ('UNASSESSED','SATISFIED','NOT_SATISFIED','CONTESTED','UNKNOWN')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale)<>''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,situation_id,argument_id)
    REFERENCES niccolo.epistemic_arguments(workspace_id,situation_id,argument_id),
  FOREIGN KEY (workspace_id,argument_id,grounding_set_id)
    REFERENCES niccolo.epistemic_grounding_sets(workspace_id,argument_id,grounding_set_id),
  FOREIGN KEY (workspace_id,condition_object_id)
    REFERENCES niccolo.epistemic_objects(workspace_id,object_id)
);
CREATE INDEX epistemic_route_applicability_by_condition ON niccolo.epistemic_route_applicability_events
  (workspace_id,situation_id,argument_id,grounding_set_id,condition_object_id,event_no DESC);
GRANT SELECT,INSERT ON niccolo.epistemic_route_applicability_events TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_route_applicability_events_event_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_route_applicability_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_route_applicability_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_route_applicability_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_route_applicability_events AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_route_applicability_events AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.epistemic_route_applicability_events AS RESTRICTIVE FOR UPDATE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.epistemic_route_applicability_events AS RESTRICTIVE FOR DELETE TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
