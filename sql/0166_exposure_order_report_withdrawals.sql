BEGIN;

CREATE TABLE niccolo.exposure_order_report_withdrawals (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  report_id UUID NOT NULL,
  available_on DATE NOT NULL,
  rationale TEXT NOT NULL CHECK (octet_length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale) <> ''),
  recorded_by UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  PRIMARY KEY (workspace_id, situation_id, report_id),
  FOREIGN KEY (workspace_id, situation_id, report_id)
    REFERENCES niccolo.exposure_order_reports(workspace_id, situation_id, report_id)
);

GRANT SELECT, INSERT ON niccolo.exposure_order_report_withdrawals TO niccolo_app;
ALTER TABLE niccolo.exposure_order_report_withdrawals ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.exposure_order_report_withdrawals FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.exposure_order_report_withdrawals
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id', true), '')::uuid);
CREATE POLICY api_read_gate ON niccolo.exposure_order_report_withdrawals AS RESTRICTIVE
  FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id, 'read'));
CREATE POLICY api_write_insert_gate ON niccolo.exposure_order_report_withdrawals AS RESTRICTIVE
  FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id, 'write'));
CREATE POLICY api_write_update_gate ON niccolo.exposure_order_report_withdrawals AS RESTRICTIVE
  FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false);
CREATE POLICY api_write_delete_gate ON niccolo.exposure_order_report_withdrawals AS RESTRICTIVE
  FOR DELETE TO niccolo_api USING (false);

COMMIT;
