BEGIN;

CREATE TABLE niccolo.investigator_input_drafts (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  draft_id UUID NOT NULL,
  situation_revision BIGINT NOT NULL,
  requested_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  input_text TEXT NOT NULL CHECK (octet_length(input_text) BETWEEN 1 AND 4096),
  provider TEXT NOT NULL CHECK (length(provider) BETWEEN 1 AND 512),
  model TEXT NOT NULL CHECK (length(model) BETWEEN 1 AND 512),
  proposal JSONB NOT NULL CHECK (jsonb_typeof(proposal) = 'object'),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, draft_id),
  FOREIGN KEY (workspace_id, situation_id, situation_revision)
    REFERENCES niccolo.situation_revisions(workspace_id, situation_id, revision)
);

CREATE TABLE niccolo.investigator_input_draft_adoptions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  draft_id UUID NOT NULL,
  adopted_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  reviewed_graph JSONB NOT NULL CHECK (jsonb_typeof(reviewed_graph) = 'object'),
  object_ids UUID[] NOT NULL CHECK (cardinality(object_ids) BETWEEN 1 AND 12),
  relation_ids UUID[] NOT NULL CHECK (cardinality(relation_ids) <= 20),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id, situation_id, draft_id),
  FOREIGN KEY (workspace_id, situation_id, draft_id)
    REFERENCES niccolo.investigator_input_drafts(workspace_id, situation_id, draft_id)
);

GRANT SELECT, INSERT ON niccolo.investigator_input_drafts,
  niccolo.investigator_input_draft_adoptions TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
  FOREACH t IN ARRAY ARRAY['investigator_input_drafts', 'investigator_input_draft_adoptions'] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)', t);
    EXECUTE format('CREATE POLICY api_read_gate ON niccolo.%I AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,''read''))', t);
    EXECUTE format('CREATE POLICY api_write_insert_gate ON niccolo.%I AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,''write''))', t);
    EXECUTE format('CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false)', t);
    EXECUTE format('CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false)', t);
  END LOOP;
END $$;

COMMIT;
