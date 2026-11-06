-- v0.74: bind structured claims to byte spans in the human-visible answer text.
BEGIN;
CREATE TABLE niccolo.situation_analysis_claim_output_spans (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  analysis_id UUID NOT NULL,
  claim_id UUID NOT NULL,
  byte_start BIGINT NOT NULL CHECK (byte_start >= 0),
  byte_end BIGINT NOT NULL CHECK (byte_end > byte_start),
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,situation_id,analysis_id,claim_id),
  FOREIGN KEY (workspace_id,situation_id,analysis_id,claim_id)
    REFERENCES niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id)
);
CREATE OR REPLACE FUNCTION niccolo.validate_claim_output_span() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE n bigint;
BEGIN
  SELECT octet_length(answer_text) INTO n FROM niccolo.situation_analysis_outputs o
    WHERE o.workspace_id=NEW.workspace_id AND o.situation_id=NEW.situation_id AND o.analysis_id=NEW.analysis_id;
  IF n IS NULL OR NEW.byte_end > n THEN RAISE EXCEPTION 'claim output span outside answer bytes'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER validate_claim_output_span_before_insert
  BEFORE INSERT ON niccolo.situation_analysis_claim_output_spans
  FOR EACH ROW EXECUTE FUNCTION niccolo.validate_claim_output_span();
GRANT SELECT, INSERT ON niccolo.situation_analysis_claim_output_spans TO niccolo_app;
ALTER TABLE niccolo.situation_analysis_claim_output_spans ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.situation_analysis_claim_output_spans FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.situation_analysis_claim_output_spans
  USING (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid)
  WITH CHECK (workspace_id = NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.situation_analysis_claim_output_spans AS RESTRICTIVE FOR SELECT TO niccolo_api
  USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.situation_analysis_claim_output_spans AS RESTRICTIVE FOR INSERT TO niccolo_api
  WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
