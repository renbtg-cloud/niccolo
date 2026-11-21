-- A human's selected intended course is not a permission grant or execution.
BEGIN;
CREATE TABLE niccolo.epistemic_action_decisions (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  decision_id UUID NOT NULL,
  decision_no BIGINT GENERATED ALWAYS AS IDENTITY,
  recommendation_id UUID NOT NULL,
  disposition TEXT NOT NULL CHECK (disposition IN ('SELECT_PRIMARY','SELECT_ALTERNATIVE','DEFER','DECLINE')),
  alternative_index INTEGER,
  rationale TEXT NOT NULL CHECK (octet_length(rationale) BETWEEN 1 AND 4096 AND btrim(rationale,U&'\0009\000A\000B\000C\000D\0020\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000')<>''),
  revises_decision_id UUID,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,decision_id),
  UNIQUE (workspace_id,situation_id,decision_id),
  FOREIGN KEY (workspace_id,situation_id,recommendation_id) REFERENCES niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id),
  FOREIGN KEY (workspace_id,situation_id,revises_decision_id) REFERENCES niccolo.epistemic_action_decisions(workspace_id,situation_id,decision_id),
  CHECK ((disposition='SELECT_ALTERNATIVE')=(alternative_index IS NOT NULL)),
  CHECK (alternative_index BETWEEN 0 AND 7),
  CHECK (revises_decision_id IS NULL OR revises_decision_id<>decision_id),
  CHECK (decision_id<>'00000000-0000-0000-0000-000000000000')
);
CREATE UNIQUE INDEX action_decision_single_revision ON niccolo.epistemic_action_decisions(workspace_id,revises_decision_id) WHERE revises_decision_id IS NOT NULL;
CREATE INDEX action_decisions_situation ON niccolo.epistemic_action_decisions(workspace_id,situation_id,decision_no);
CREATE FUNCTION niccolo.validate_action_decision() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE alternative_count INTEGER;
BEGIN
  SELECT jsonb_array_length(payload->'alternatives') INTO alternative_count FROM niccolo.epistemic_action_recommendations
    WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND recommendation_id=NEW.recommendation_id;
  IF NOT FOUND OR (NEW.alternative_index IS NOT NULL AND NEW.alternative_index>=alternative_count)
  THEN RAISE EXCEPTION 'Decision must pin an exact visible recommendation and its actual option' USING ERRCODE='23514'; END IF;
  IF NEW.revises_decision_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM niccolo.epistemic_action_decisions prior WHERE prior.workspace_id=NEW.workspace_id AND prior.situation_id=NEW.situation_id
      AND prior.decision_id=NEW.revises_decision_id AND prior.recommendation_id=NEW.recommendation_id AND prior.recorded_by_principal_id=NEW.recorded_by_principal_id
  ) THEN RAISE EXCEPTION 'Decision revision must preserve attribution and pinned recommendation' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER action_decision_validate BEFORE INSERT ON niccolo.epistemic_action_decisions FOR EACH ROW EXECUTE FUNCTION niccolo.validate_action_decision();
GRANT SELECT,INSERT ON niccolo.epistemic_action_decisions TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_action_decisions_decision_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_action_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_action_decisions FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_action_decisions USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_action_decisions AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_action_decisions AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
