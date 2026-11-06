-- v0.88: append-only attributed events about whether a revision condition has been met.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_condition_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  condition_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('MET','NOT_MET','REOPENED')),
  resolution_note TEXT NOT NULL CHECK (length(resolution_note) BETWEEN 1 AND 4096 AND btrim(resolution_note) <> ''),
  related_evidence_id UUID,
  related_contribution_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id,condition_id,event_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id,condition_id)
    REFERENCES niccolo.situation_analysis_claim_revision_conditions(workspace_id,situation_id,analysis_id,claim_id,condition_id),
  FOREIGN KEY (workspace_id,related_evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  FOREIGN KEY (workspace_id,situation_id,related_contribution_id)
    REFERENCES niccolo.situation_contributions(workspace_id,situation_id,contribution_id),
  CHECK (NOT (related_evidence_id IS NOT NULL AND related_contribution_id IS NOT NULL))
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_claim_condition_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_claim_condition_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_claim_condition_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_condition_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_condition_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_condition_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
