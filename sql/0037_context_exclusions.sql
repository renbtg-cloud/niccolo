-- v0.73: explain why human context present in the frozen input was not used.
-- This is not a truth judgment and never becomes remembered rejection/deprioritization policy.
BEGIN;
CREATE TABLE niccolo.situation_analysis_context_exclusions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  input_revision BIGINT NOT NULL CHECK (input_revision >= 1),
  contribution_id UUID NOT NULL,
  reason_kind TEXT NOT NULL CHECK (reason_kind IN ('OUT_OF_SCOPE','REDUNDANT_FOR_THIS_ANALYSIS','INSUFFICIENT_RELEVANCE','CONFLICTING_SCOPE','OTHER')),
  reason TEXT NOT NULL CHECK (length(reason) BETWEEN 1 AND 4096 AND btrim(reason) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,contribution_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,input_revision)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision),
  FOREIGN KEY (workspace_id,situation_id,input_revision,contribution_id)
    REFERENCES niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id)
);
CREATE OR REPLACE FUNCTION niccolo.validate_context_exclusion() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (SELECT 1 FROM niccolo.situation_analysis_used_contributions u
    WHERE u.workspace_id=NEW.workspace_id AND u.situation_id=NEW.situation_id
      AND u.analysis_id=NEW.analysis_id AND u.contribution_id=NEW.contribution_id) THEN
    RAISE EXCEPTION 'context already recorded as used by this analysis';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_context_exclusion_before_insert
  BEFORE INSERT ON niccolo.situation_analysis_context_exclusions
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_context_exclusion();
GRANT SELECT, INSERT ON niccolo.situation_analysis_context_exclusions TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_context_exclusions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_context_exclusions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_context_exclusions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_context_exclusions AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_context_exclusions AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
