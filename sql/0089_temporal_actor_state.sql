-- Qualified temporal assertions about actors, roles, awareness, authority and influence.
-- These records describe observations/assertions/inferences; they do not establish mental state or truth.
BEGIN;

CREATE TABLE niccolo.situation_analysis_temporal_assertions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  assertion_id UUID NOT NULL,
  subject_label TEXT NOT NULL CHECK (length(subject_label) BETWEEN 1 AND 512 AND btrim(subject_label) <> ''),
  assertion_type TEXT NOT NULL CHECK (assertion_type IN (
    'ROLE','FORMAL_AUTHORITY','ACCESS_TO_INFORMATION','COMMUNICATION',
    'DECLARED_BELIEF','ATTRIBUTED_BELIEF','INFERRED_AWARENESS',
    'RELATIONSHIP','DECISION_PARTICIPATION','PRACTICAL_INFLUENCE'
  )),
  object_text TEXT NOT NULL CHECK (length(object_text) BETWEEN 1 AND 4096 AND btrim(object_text) <> ''),
  valid_from TIMESTAMPTZ,
  valid_to TIMESTAMPTZ,
  epistemic_kind TEXT NOT NULL CHECK (epistemic_kind IN (
    'DOCUMENTARY_OBSERVATION','HUMAN_ASSERTION','MACHINE_HYPOTHESIS'
  )),
  qualification TEXT NOT NULL CHECK (qualification IN (
    'OBSERVED','REPORTED','ATTRIBUTED','INFERRED','UNCERTAIN','CONTRADICTED','UNKNOWN'
  )),
  evidence_id UUID,
  contribution_id UUID,
  rationale TEXT NOT NULL CHECK (length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,assertion_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id),
  FOREIGN KEY (workspace_id,evidence_id)
    REFERENCES niccolo.evidence(workspace_id,evidence_id),
  FOREIGN KEY (workspace_id,situation_id,contribution_id)
    REFERENCES niccolo.situation_contributions(workspace_id,situation_id,contribution_id),
  CHECK (valid_to IS NULL OR valid_from IS NULL OR valid_to >= valid_from),
  CHECK (NOT (evidence_id IS NOT NULL AND contribution_id IS NOT NULL))
);

CREATE INDEX temporal_assertions_by_analysis
 ON niccolo.situation_analysis_temporal_assertions
 (workspace_id,situation_id,analysis_id,assertion_type,subject_label,valid_from,assertion_id);

GRANT SELECT,INSERT ON niccolo.situation_analysis_temporal_assertions TO niccolo_app;

ALTER TABLE niccolo.situation_analysis_temporal_assertions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_temporal_assertions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_temporal_assertions
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_temporal_assertions AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_temporal_assertions AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_update_gate ON niccolo.situation_analysis_temporal_assertions AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'))
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
CREATE POLICY api_write_delete_gate ON niccolo.situation_analysis_temporal_assertions AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'write'));

COMMIT;
