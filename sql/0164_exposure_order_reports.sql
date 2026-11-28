BEGIN;

CREATE TABLE niccolo.exposure_order_reports (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  report_id UUID NOT NULL,
  exposure_record_id UUID NOT NULL,
  testimony_representation_id UUID NOT NULL,
  candidate_representation_id UUID NOT NULL,
  direction TEXT NOT NULL CHECK (direction IN ('EXPOSURE_BEFORE_TESTIMONY', 'EXPOSURE_AFTER_TESTIMONY')),
  source_object_id UUID NOT NULL,
  source_representation_id UUID NOT NULL,
  available_on DATE NOT NULL,
  recorded_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  PRIMARY KEY (workspace_id, situation_id, report_id),
  FOREIGN KEY (workspace_id, situation_id, exposure_record_id)
    REFERENCES niccolo.public_prominence_records(workspace_id, situation_id, record_id),
  FOREIGN KEY (workspace_id, situation_id, testimony_representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id, situation_id, representation_id),
  FOREIGN KEY (workspace_id, situation_id, candidate_representation_id)
    REFERENCES niccolo.situation_artifact_representations(workspace_id, situation_id, representation_id),
  FOREIGN KEY (workspace_id, situation_id, source_object_id, source_representation_id)
    REFERENCES niccolo.workbench_artifact_provenance(workspace_id, situation_id, object_id, representation_id)
);

CREATE INDEX exposure_order_lookup ON niccolo.exposure_order_reports (
  workspace_id, situation_id, exposure_record_id, testimony_representation_id,
  candidate_representation_id, direction, available_on, report_id
);

GRANT SELECT, INSERT ON niccolo.exposure_order_reports TO niccolo_app;
ALTER TABLE niccolo.exposure_order_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.exposure_order_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.exposure_order_reports
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid);
CREATE POLICY api_read_gate ON niccolo.exposure_order_reports AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id, 'read'));
CREATE POLICY api_write_insert_gate ON niccolo.exposure_order_reports AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id, 'write'));
CREATE POLICY api_write_update_gate ON niccolo.exposure_order_reports AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false);
CREATE POLICY api_write_delete_gate ON niccolo.exposure_order_reports AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (false);

COMMIT;
