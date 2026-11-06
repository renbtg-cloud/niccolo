-- v0.121-v0.124: append-only human rationale for decision selections.
BEGIN;
CREATE TABLE niccolo.situation_analysis_decision_rationales (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  rationale_id UUID NOT NULL,
  option_id UUID,
  rationale_text TEXT NOT NULL CHECK (length(rationale_text) BETWEEN 1 AND 4096 AND btrim(rationale_text) <> ''),
  tradeoff_ack TEXT CHECK (tradeoff_ack IS NULL OR (length(tradeoff_ack) <= 4096 AND btrim(tradeoff_ack) <> '')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,decision_id,rationale_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,decision_id)
    REFERENCES niccolo.situation_analysis_decision_points(workspace_id,situation_id,analysis_id,decision_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,decision_id,option_id)
    REFERENCES niccolo.situation_analysis_decision_options(workspace_id,situation_id,analysis_id,decision_id,option_id)
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_decision_rationales TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_decision_rationales ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_decision_rationales FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_decision_rationales USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_decision_rationales AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_decision_rationales AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
