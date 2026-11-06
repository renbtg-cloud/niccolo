-- v0.125-v0.129: explicit evidence gaps and append-only human disposition.
BEGIN;
CREATE TABLE niccolo.situation_analysis_evidence_gaps (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  gap_id UUID NOT NULL,
  claim_id UUID,
  description TEXT NOT NULL CHECK (length(description) BETWEEN 1 AND 4096 AND btrim(description) <> ''),
  requested_source_kind TEXT CHECK (requested_source_kind IS NULL OR (length(requested_source_kind) <= 256 AND btrim(requested_source_kind) <> '')),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,gap_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id) REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,claim_id) REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
CREATE TABLE niccolo.situation_analysis_evidence_gap_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  gap_id UUID NOT NULL,
  event_id UUID NOT NULL,
  event_kind TEXT NOT NULL CHECK (event_kind IN ('SATISFIED','WAIVED','REOPENED')),
  evidence_id UUID,
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,gap_id,event_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id,gap_id) REFERENCES niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id),
  FOREIGN KEY(workspace_id,evidence_id) REFERENCES niccolo.source_fragments(workspace_id,evidence_id),
  CHECK ((event_kind='SATISFIED' AND evidence_id IS NOT NULL) OR event_kind<>'SATISFIED')
);
GRANT SELECT,INSERT ON niccolo.situation_analysis_evidence_gaps,niccolo.situation_analysis_evidence_gap_events TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_evidence_gaps ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_evidence_gaps FORCE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_evidence_gap_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_evidence_gap_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_evidence_gaps USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY workspace_filter ON niccolo.situation_analysis_evidence_gap_events USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_evidence_gaps AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_evidence_gaps AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_read_gate ON niccolo.situation_analysis_evidence_gap_events AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_evidence_gap_events AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
