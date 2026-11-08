-- Bounded model-proposed lexical investigations over a frozen analysis input.
-- A recorded query is a retrieval intent, not evidence; a hit is only source text.
BEGIN;

CREATE TABLE niccolo.situation_analysis_observation_queries (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  query_id UUID NOT NULL,
  query_text TEXT NOT NULL CHECK (octet_length(query_text) BETWEEN 1 AND 160 AND btrim(query_text) <> ''),
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 2048 AND btrim(rationale) <> ''),
  expected_observation TEXT NOT NULL CHECK (length(expected_observation) BETWEEN 1 AND 2048 AND btrim(expected_observation) <> ''),
  direction TEXT NOT NULL CHECK (direction IN ('SUPPORT','COUNTEREVIDENCE','DISCRIMINATE')),
  interpretation_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,query_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,interpretation_id)
    REFERENCES niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id)
);

CREATE TABLE niccolo.situation_analysis_observation_query_hits (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  query_id UUID NOT NULL,
  evidence_id UUID NOT NULL,
  hit_rank SMALLINT NOT NULL CHECK (hit_rank BETWEEN 1 AND 5),
  lexical_relevance REAL NOT NULL CHECK (lexical_relevance >= 0),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,query_id,evidence_id),
  UNIQUE (workspace_id,situation_id,analysis_id,query_id,hit_rank),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,query_id)
    REFERENCES niccolo.situation_analysis_observation_queries(workspace_id,situation_id,analysis_id,query_id),
  FOREIGN KEY (workspace_id,evidence_id)
    REFERENCES niccolo.evidence(workspace_id,evidence_id)
);

CREATE INDEX observation_queries_by_analysis
 ON niccolo.situation_analysis_observation_queries
 (workspace_id,situation_id,analysis_id,created_at,query_id);

CREATE INDEX observation_hits_by_analysis
 ON niccolo.situation_analysis_observation_query_hits
 (workspace_id,situation_id,analysis_id,query_id,hit_rank);

GRANT SELECT,INSERT ON niccolo.situation_analysis_observation_queries,
 niccolo.situation_analysis_observation_query_hits TO niccolo_app;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['situation_analysis_observation_queries',
                          'situation_analysis_observation_query_hits'] LOOP
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
