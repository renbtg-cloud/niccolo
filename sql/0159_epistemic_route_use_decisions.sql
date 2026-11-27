-- Attributed use/suspension of one authored support route. Keep its evidence intact.
BEGIN;

CREATE TABLE niccolo.epistemic_route_use_decisions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  argument_id UUID NOT NULL,
  grounding_set_id UUID NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('USE', 'SUSPEND')),
  rationale TEXT NOT NULL CHECK (
    octet_length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''
  ),
  basis JSONB NOT NULL CHECK (
    jsonb_typeof(basis) = 'object' AND basis ? 'input' AND basis ? 'result'
  ),
  recorded_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, event_id),
  FOREIGN KEY (workspace_id, situation_id, argument_id)
    REFERENCES niccolo.epistemic_arguments(workspace_id, situation_id, argument_id),
  FOREIGN KEY (workspace_id, argument_id, grounding_set_id)
    REFERENCES niccolo.epistemic_grounding_sets(workspace_id, argument_id, grounding_set_id)
);
CREATE INDEX epistemic_route_use_decisions_situation_order
  ON niccolo.epistemic_route_use_decisions(workspace_id, situation_id, event_no);

GRANT SELECT, INSERT ON niccolo.epistemic_route_use_decisions TO niccolo_app;
GRANT USAGE, SELECT ON SEQUENCE niccolo.epistemic_route_use_decisions_event_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_route_use_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_route_use_decisions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_route_use_decisions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_route_use_decisions AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id, 'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_route_use_decisions AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id, 'write'));

COMMIT;
