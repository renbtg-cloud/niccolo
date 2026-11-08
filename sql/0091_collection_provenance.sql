-- Collection/custody/transformation provenance for source material.
-- A collector, transformer or importer is not thereby the original author or a trusted witness.
BEGIN;

CREATE TABLE niccolo.situation_analysis_collection_events (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  event_id UUID NOT NULL,
  actor_label TEXT NOT NULL CHECK (length(actor_label) BETWEEN 1 AND 512 AND btrim(actor_label) <> ''),
  actor_role TEXT NOT NULL CHECK (actor_role IN (
    'ORIGINAL_AUTHOR','CUSTODIAN','COLLECTOR','INTERVIEWER','TRANSCRIBER',
    'SUMMARIZER','TRANSFORMER','EXPORTER','IMPORTER','ATTESTER','OPERATOR'
  )),
  operation TEXT NOT NULL CHECK (length(operation) BETWEEN 1 AND 512 AND btrim(operation) <> ''),
  input_document_id UUID,
  input_evidence_id UUID,
  output_document_id UUID,
  output_evidence_id UUID,
  occurred_at TIMESTAMPTZ,
  method TEXT NOT NULL CHECK (length(method) BETWEEN 1 AND 4096 AND btrim(method) <> ''),
  assertion_kind TEXT NOT NULL CHECK (assertion_kind IN (
    'DOCUMENTARY_OBSERVATION','HUMAN_ASSERTION','MACHINE_HYPOTHESIS'
  )),
  qualification TEXT NOT NULL CHECK (qualification IN (
    'OBSERVED','REPORTED','ATTRIBUTED','INFERRED','UNCERTAIN','CONTRADICTED','UNKNOWN'
  )),
  note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 4096 AND btrim(note) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,event_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,input_document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id),
  FOREIGN KEY (workspace_id,output_document_id)
    REFERENCES niccolo.source_documents(workspace_id,document_id),
  FOREIGN KEY (workspace_id,input_evidence_id)
    REFERENCES niccolo.evidence(workspace_id,evidence_id),
  FOREIGN KEY (workspace_id,output_evidence_id)
    REFERENCES niccolo.evidence(workspace_id,evidence_id),
  CHECK (input_document_id IS NOT NULL OR input_evidence_id IS NOT NULL OR
         output_document_id IS NOT NULL OR output_evidence_id IS NOT NULL)
);

CREATE INDEX collection_events_by_analysis
 ON niccolo.situation_analysis_collection_events
 (workspace_id,situation_id,analysis_id,created_at,event_id);

CREATE INDEX collection_events_by_actor
 ON niccolo.situation_analysis_collection_events
 (workspace_id,situation_id,analysis_id,actor_label,actor_role);

GRANT SELECT,INSERT ON niccolo.situation_analysis_collection_events TO niccolo_app;

ALTER TABLE niccolo.situation_analysis_collection_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_collection_events FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_collection_events
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_collection_events AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_collection_events AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_collection_events AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_collection_events AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
