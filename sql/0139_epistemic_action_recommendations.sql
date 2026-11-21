-- Attributed advisory records; no decision, authorization, planning or execution.
BEGIN;
CREATE TABLE niccolo.epistemic_action_recommendations (
  workspace_id UUID NOT NULL,
  situation_id UUID NOT NULL,
  recommendation_id UUID NOT NULL,
  recommendation_no BIGINT GENERATED ALWAYS AS IDENTITY,
  recipient_actor_id UUID NOT NULL,
  ground_object_ids UUID[] NOT NULL,
  revises_recommendation_id UUID,
  payload JSONB NOT NULL,
  recorded_by_principal_id UUID NOT NULL REFERENCES niccolo_auth.principals(principal_id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (workspace_id,recommendation_id),
  UNIQUE (workspace_id,situation_id,recommendation_id),
  FOREIGN KEY (workspace_id,situation_id) REFERENCES niccolo.situations(workspace_id,situation_id),
  FOREIGN KEY (workspace_id,recipient_actor_id) REFERENCES niccolo.actors(workspace_id,actor_id),
  FOREIGN KEY (workspace_id,situation_id,revises_recommendation_id) REFERENCES niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id),
  CHECK (revises_recommendation_id IS NULL OR revises_recommendation_id<>recommendation_id),
  CHECK (recommendation_id<>'00000000-0000-0000-0000-000000000000' AND recipient_actor_id<>'00000000-0000-0000-0000-000000000000'),
  CHECK (cardinality(ground_object_ids) BETWEEN 1 AND 64 AND array_ndims(ground_object_ids)=1 AND array_position(ground_object_ids,NULL) IS NULL),
  CHECK (array_position(ground_object_ids,'00000000-0000-0000-0000-000000000000'::uuid) IS NULL),
  CHECK (jsonb_typeof(payload)='object')
);
CREATE UNIQUE INDEX action_recommendation_single_revision ON niccolo.epistemic_action_recommendations(workspace_id,revises_recommendation_id) WHERE revises_recommendation_id IS NOT NULL;
CREATE INDEX action_recommendation_situation ON niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_no);

CREATE FUNCTION niccolo.validate_action_recommendation() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE key TEXT; alternative JSONB; prior niccolo.epistemic_action_recommendations%ROWTYPE;
  kinds TEXT[] := ARRAY['PRESERVE_EVIDENCE','REVIEW_PRIVILEGES','SEPARATE_DUTIES','INDEPENDENT_REVIEW','VERIFY_PROVENANCE','GATHER_INFORMATION','DELAY','INACTION'];
  -- Match Rust's Unicode whitespace trimming even under a C database locale.
  whitespace TEXT := U&'\0009\000A\000B\000C\000D\0020\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000';
BEGIN
  IF NOT (NEW.payload ?& ARRAY['kind','recipient_actor_id','ground_object_ids','revises_recommendation_id','purpose','context','rationale','conditions','alternatives','expected_consequences','uncertainty','risk','reversibility'])
    OR (SELECT count(*) FROM jsonb_object_keys(NEW.payload))<>13
    OR NOT COALESCE(NEW.payload->>'kind'=ANY(kinds),false)
    OR NOT COALESCE(NEW.payload->>'reversibility' IN ('REVERSIBLE','PARTIALLY_REVERSIBLE','IRREVERSIBLE','UNKNOWN'),false)
    OR NEW.payload->'recipient_actor_id' IS DISTINCT FROM to_jsonb(NEW.recipient_actor_id)
    OR NEW.payload->'ground_object_ids' IS DISTINCT FROM to_jsonb(NEW.ground_object_ids)
    OR NEW.payload->'revises_recommendation_id' IS DISTINCT FROM COALESCE(to_jsonb(NEW.revises_recommendation_id),'null'::jsonb)
  THEN RAISE EXCEPTION 'Invalid recommendation payload' USING ERRCODE='23514'; END IF;
  FOREACH key IN ARRAY ARRAY['purpose','context','rationale','conditions','expected_consequences','uncertainty','risk'] LOOP
    IF jsonb_typeof(NEW.payload->key) IS DISTINCT FROM 'string' OR octet_length(NEW.payload->>key)>4096 OR btrim(NEW.payload->>key,whitespace)=''
    THEN RAISE EXCEPTION 'Invalid recommendation text' USING ERRCODE='23514'; END IF;
  END LOOP;
  IF jsonb_typeof(NEW.payload->'alternatives') IS DISTINCT FROM 'array' OR jsonb_array_length(NEW.payload->'alternatives') NOT BETWEEN 1 AND 8
  THEN RAISE EXCEPTION 'Recommendation must compare alternatives' USING ERRCODE='23514'; END IF;
  FOR alternative IN SELECT value FROM jsonb_array_elements(NEW.payload->'alternatives') LOOP
    IF jsonb_typeof(alternative) IS DISTINCT FROM 'object' OR NOT (alternative ?& ARRAY['kind','rationale','expected_consequences'])
      OR (SELECT count(*) FROM jsonb_object_keys(alternative))<>3 OR NOT COALESCE(alternative->>'kind'=ANY(kinds),false)
      OR alternative->>'kind'=NEW.payload->>'kind'
    THEN RAISE EXCEPTION 'Invalid recommendation alternative' USING ERRCODE='23514'; END IF;
    FOREACH key IN ARRAY ARRAY['rationale','expected_consequences'] LOOP
      IF jsonb_typeof(alternative->key) IS DISTINCT FROM 'string' OR octet_length(alternative->>key)>4096 OR btrim(alternative->>key,whitespace)=''
      THEN RAISE EXCEPTION 'Invalid alternative text' USING ERRCODE='23514'; END IF;
    END LOOP;
  END LOOP;
  IF (SELECT count(*) FROM (
    SELECT proposition_id AS id FROM niccolo.epistemic_situation_propositions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
    UNION SELECT hypothesis_id FROM niccolo.epistemic_hypotheses WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
    UNION SELECT assumption_id FROM niccolo.epistemic_assumptions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
    UNION SELECT gap_id FROM niccolo.epistemic_information_gaps WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
    UNION SELECT question_id FROM niccolo.epistemic_questions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
    UNION SELECT finding_id FROM niccolo.epistemic_findings WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
    UNION SELECT argument_id FROM niccolo.epistemic_arguments WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id
  ) candidates WHERE id=ANY(NEW.ground_object_ids))<>cardinality(NEW.ground_object_ids)
  THEN RAISE EXCEPTION 'Grounds must be distinct and in the same Situation' USING ERRCODE='23514'; END IF;
  IF NEW.revises_recommendation_id IS NOT NULL THEN
    SELECT * INTO prior FROM niccolo.epistemic_action_recommendations WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND recommendation_id=NEW.revises_recommendation_id;
    IF NOT FOUND OR prior.recipient_actor_id<>NEW.recipient_actor_id
    THEN RAISE EXCEPTION 'Revision must preserve recipient and Situation' USING ERRCODE='23514'; END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER action_recommendation_validate BEFORE INSERT ON niccolo.epistemic_action_recommendations FOR EACH ROW EXECUTE FUNCTION niccolo.validate_action_recommendation();
GRANT SELECT,INSERT ON niccolo.epistemic_action_recommendations TO niccolo_app;
GRANT USAGE,SELECT ON SEQUENCE niccolo.epistemic_action_recommendations_recommendation_no_seq TO niccolo_app;
ALTER TABLE niccolo.epistemic_action_recommendations ENABLE ROW LEVEL SECURITY;
ALTER TABLE niccolo.epistemic_action_recommendations FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_filter ON niccolo.epistemic_action_recommendations USING (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid) WITH CHECK (workspace_id=NULLIF(current_setting('niccolo.workspace_id',true),'')::uuid);
CREATE POLICY api_read_gate ON niccolo.epistemic_action_recommendations AS RESTRICTIVE FOR SELECT TO niccolo_api USING (niccolo_auth.rls_credential_allows(workspace_id,'read'));
CREATE POLICY api_write_insert_gate ON niccolo.epistemic_action_recommendations AS RESTRICTIVE FOR INSERT TO niccolo_api WITH CHECK (niccolo_auth.rls_credential_allows(workspace_id,'write'));
COMMIT;
