-- v0.70: append-only human decisions on reanalysis suggestions.
-- REQUESTED means the human asked for reanalysis; it still does not run one.
BEGIN;
CREATE TABLE niccolo.situation_reanalysis_suggestion_decisions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  suggestion_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  decision_kind TEXT NOT NULL CHECK (decision_kind IN ('REQUESTED','DISMISSED','DEFERRED')),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  decided_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,suggestion_id,decision_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,suggestion_id)
    REFERENCES niccolo.situation_reanalysis_suggestions(workspace_id,situation_id,analysis_id,suggestion_id)
);
GRANT SELECT, INSERT ON niccolo.situation_reanalysis_suggestion_decisions TO niccolo_app;
ALTER TABLE niccolo.situation_reanalysis_suggestion_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_reanalysis_suggestion_decisions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_reanalysis_suggestion_decisions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_reanalysis_suggestion_decisions AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_reanalysis_suggestion_decisions AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
