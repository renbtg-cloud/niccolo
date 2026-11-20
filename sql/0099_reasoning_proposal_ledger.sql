-- Durable boundary between model-generated proposals and promoted reasoning objects.
-- This migration does not yet change promotion policy: it records proposal identity,
-- item identity and explicit promotion decisions so parsimony can be added without
-- losing auditability or silently conflating model output with canonical reasoning.
BEGIN;

CREATE TABLE niccolo.situation_analysis_reasoning_proposals (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  proposal_id UUID NOT NULL,
  step_kind TEXT NOT NULL CHECK (step_kind IN ('ASSESS_CONTEXT','CHECK_ASSUMPTIONS','PROPOSE_CLARIFICATION','CHECK_ALTERNATIVES')),
  proposal_payload JSONB NOT NULL CHECK (jsonb_typeof(proposal_payload)='object'),
  proposed_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,proposal_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);

CREATE TABLE niccolo.situation_analysis_reasoning_proposal_items (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  proposal_id UUID NOT NULL,
  item_kind TEXT NOT NULL CHECK (item_kind IN ('HYPOTHESIS','ASSUMPTION','EVIDENCE_GAP','CLARIFICATION','SOURCE_QUERY')),
  item_id UUID NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 1024),
  item_payload JSONB NOT NULL CHECK (jsonb_typeof(item_payload)='object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id),
  UNIQUE (workspace_id,situation_id,analysis_id,proposal_id,item_kind,ordinal),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,proposal_id)
    REFERENCES niccolo.situation_analysis_reasoning_proposals(workspace_id,situation_id,analysis_id,proposal_id)
);
CREATE INDEX reasoning_proposal_items_by_analysis
  ON niccolo.situation_analysis_reasoning_proposal_items(workspace_id,situation_id,analysis_id,item_kind,item_id);

CREATE TABLE niccolo.situation_analysis_reasoning_proposal_dispositions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  proposal_id UUID NOT NULL,
  item_kind TEXT NOT NULL,
  item_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_no BIGINT GENERATED ALWAYS AS IDENTITY,
  disposition TEXT NOT NULL CHECK (disposition IN ('PROMOTED','DEFERRED','REJECTED')),
  decision_source TEXT NOT NULL CHECK (decision_source IN ('DETERMINISTIC','HUMAN')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale)<>''),
  decided_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id,event_id),
  UNIQUE (workspace_id,event_no),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id)
    REFERENCES niccolo.situation_analysis_reasoning_proposal_items(workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id)
);
CREATE INDEX reasoning_proposal_dispositions_latest
  ON niccolo.situation_analysis_reasoning_proposal_dispositions(
    workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id,event_no DESC
  );

GRANT SELECT,INSERT ON
  niccolo.situation_analysis_reasoning_proposals,
  niccolo.situation_analysis_reasoning_proposal_items,
  niccolo.situation_analysis_reasoning_proposal_dispositions
TO niccolo_app;

GRANT USAGE,SELECT ON SEQUENCE
  niccolo.situation_analysis_reasoning_proposal_dispositions_event_no_seq
TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY[
  'situation_analysis_reasoning_proposals',
  'situation_analysis_reasoning_proposal_items',
  'situation_analysis_reasoning_proposal_dispositions'
 ] LOOP
  EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY',t);
  EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY',t);
  EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)',t);
  EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))',t);
  EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
  EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))',t);
 END LOOP;
END $$;

COMMIT;
