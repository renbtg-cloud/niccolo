-- v0.94: recipient receipt for an exact handoff hash; receipt is not agreement or delegated authority.
BEGIN;
CREATE TABLE niccolo.situation_reasoning_handoff_receipts (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  receipt_id UUID NOT NULL,
  handoff_sha256 CHAR(64) NOT NULL CHECK (handoff_sha256 ~ '^[a-f0-9]{64}$'),
  receipt_kind TEXT NOT NULL CHECK (receipt_kind IN ('RECEIVED','REVIEWED','DECLINED')),
  note TEXT CHECK (note IS NULL OR (length(note) <= 4096 AND btrim(note) <> '')),
  recipient_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY(workspace_id,situation_id,analysis_id,receipt_id),
  FOREIGN KEY(workspace_id,situation_id,analysis_id)
    REFERENCES niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id)
);
GRANT SELECT,INSERT ON niccolo.situation_reasoning_handoff_receipts TO niccolo_app;
ALTER TABLE niccolo.situation_reasoning_handoff_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_reasoning_handoff_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_reasoning_handoff_receipts USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_reasoning_handoff_receipts AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_reasoning_handoff_receipts AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
