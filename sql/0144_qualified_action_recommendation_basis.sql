-- Optional qualified grounding of an advisory recommendation; never infer roles for old rows.
BEGIN;
ALTER TABLE niccolo.epistemic_action_recommendations ADD COLUMN qualified_basis JSONB NOT NULL DEFAULT '[]'::jsonb;
CREATE FUNCTION niccolo.validate_recommendation_qualified_basis() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE basis JSONB; member JSONB; covered JSONB := '[]'::jsonb;
  whitespace TEXT := U&'\0009\000A\000B\000C\000D\0020\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000';
BEGIN
  IF jsonb_typeof(NEW.qualified_basis) IS DISTINCT FROM 'array' OR jsonb_array_length(NEW.qualified_basis)>16 THEN
    RAISE EXCEPTION 'Invalid qualified basis' USING ERRCODE='23514';
  END IF;
  FOR basis IN SELECT value FROM jsonb_array_elements(NEW.qualified_basis) LOOP
    IF jsonb_typeof(basis) IS DISTINCT FROM 'object' OR NOT (basis ?& ARRAY['kind','ground_object_ids','rationale']) OR (SELECT count(*) FROM jsonb_object_keys(basis))<>3
      OR NOT COALESCE(basis->>'kind' IN ('JOINTLY_NECESSARY','ALTERNATIVE_SUFFICIENT','COMPLEMENTARY','CONTEXT_ONLY'),false)
      OR jsonb_typeof(basis->'rationale') IS DISTINCT FROM 'string' OR octet_length(basis->>'rationale')>4096 OR btrim(basis->>'rationale',whitespace)=''
      OR jsonb_typeof(basis->'ground_object_ids') IS DISTINCT FROM 'array' OR jsonb_array_length(basis->'ground_object_ids') NOT BETWEEN 1 AND 64
    THEN RAISE EXCEPTION 'Invalid qualified basis group' USING ERRCODE='23514'; END IF;
    IF (SELECT count(DISTINCT value) FROM jsonb_array_elements(basis->'ground_object_ids'))<>jsonb_array_length(basis->'ground_object_ids') THEN
      RAISE EXCEPTION 'Duplicate basis member' USING ERRCODE='23514';
    END IF;
    FOR member IN SELECT value FROM jsonb_array_elements(basis->'ground_object_ids') LOOP
      IF jsonb_typeof(member) IS DISTINCT FROM 'string' OR NOT (to_jsonb(NEW.ground_object_ids) @> jsonb_build_array(member)) THEN
        RAISE EXCEPTION 'Qualified basis must use the exact listed grounds' USING ERRCODE='23514';
      END IF;
      covered := covered || jsonb_build_array(member);
    END LOOP;
  END LOOP;
  IF jsonb_array_length(NEW.qualified_basis)>0 AND NOT (covered @> to_jsonb(NEW.ground_object_ids)) THEN
    RAISE EXCEPTION 'Every listed ground must be qualified when groups are supplied' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER action_recommendation_qualified_basis_validate BEFORE INSERT ON niccolo.epistemic_action_recommendations FOR EACH ROW EXECUTE FUNCTION niccolo.validate_recommendation_qualified_basis();
COMMIT;
