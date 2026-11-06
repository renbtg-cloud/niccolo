-- v0.34: explicit claim-level lineage across a direct reanalysis.
BEGIN;
ALTER TABLE niccolo.situation_analysis_snapshots
  ADD CONSTRAINT situation_analysis_direct_reanalysis_unique
  UNIQUE (workspace_id,situation_id,analysis_id,reanalysis_of);

CREATE TABLE niccolo.situation_analysis_claim_lineage (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  lineage_id UUID NOT NULL,
  prior_analysis_id UUID NOT NULL,
  prior_claim_id UUID NOT NULL,
  new_analysis_id UUID NOT NULL,
  new_claim_id UUID,
  relation TEXT NOT NULL CHECK (relation IN ('RETAINED','REFINED','REVERSED','DROPPED','SPLIT','MERGED')),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,lineage_id),
  FOREIGN KEY (workspace_id,situation_id,new_analysis_id,prior_analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,reanalysis_of),
  FOREIGN KEY (workspace_id,situation_id,prior_analysis_id,prior_claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,new_analysis_id,new_claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id),
  CHECK ((relation='DROPPED' AND new_claim_id IS NULL) OR (relation<>'DROPPED' AND new_claim_id IS NOT NULL))
);
CREATE UNIQUE INDEX situation_analysis_claim_lineage_dedup
  ON niccolo.situation_analysis_claim_lineage(workspace_id,situation_id,prior_analysis_id,prior_claim_id,new_analysis_id,coalesce(new_claim_id,'00000000-0000-0000-0000-000000000000'::uuid),relation);

GRANT SELECT, INSERT ON niccolo.situation_analysis_claim_lineage TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['situation_analysis_claim_lineage'] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)', t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))', t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write'')) WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
  END LOOP;
END $$;
COMMIT;
