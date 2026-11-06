-- v0.107: explicit human-facing options with display order, not automatic ranking.
BEGIN;
CREATE TABLE niccolo.situation_analysis_decision_options (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  option_id UUID NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal >= 1),
  option_text TEXT NOT NULL CHECK (length(option_text) BETWEEN 1 AND 4096 AND btrim(option_text) <> ''),
  tradeoff_note TEXT CHECK (tradeoff_note IS NULL OR (length(tradeoff_note) <= 4096 AND btrim(tradeoff_note) <> '')),
  created_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,decision_id,option_id),
  UNIQUE(workspace_id,situation_id,analysis_id,decision_id,ordinal),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,decision_id)
    REFERENCES niccolo.situation_analysis_decision_points(workspace_id,situation_id,analysis_id,decision_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_decision_options TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_decision_options ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_decision_options FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_decision_options USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_decision_options AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_decision_options AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
