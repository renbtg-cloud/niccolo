-- Explicit investigator-authored question links, not truth or resolution edges.
BEGIN;
ALTER TABLE niccolo.epistemic_relations DROP CONSTRAINT epistemic_relations_check1;
ALTER TABLE niccolo.epistemic_relations ADD CONSTRAINT epistemic_relations_family_kind_check CHECK (
    (relation_family='SEMANTIC' AND relation_kind IN ('EQUIVALENT_TO','MORE_SPECIFIC_THAN','ENTAILS','CONTRADICTS','PARTIAL_OVERLAP')) OR
    (relation_family='GENEALOGY' AND relation_kind IN ('REFINES','SUPERSEDES','RETRACTS','CONSOLIDATES','SEPARATES','REINTERPRETS')) OR
    (relation_family='CONTEXTUAL' AND relation_kind IN ('RELEVANT_TO','APPLICABLE_UNDER','ADDRESSES_GAP','TESTS_HYPOTHESIS','PRESUPPOSES')) OR
    (relation_family='CAUSAL' AND relation_kind IN ('CAUSES','CONTRIBUTES_TO','ENABLES','PREVENTS','INTENSIFIES','ATTENUATES'))
);

-- No privileged function: caller's workspace/RLS scope remains in force.
CREATE FUNCTION niccolo.check_investigative_relation() RETURNS trigger
LANGUAGE plpgsql SECURITY INVOKER SET search_path=pg_catalog,niccolo AS $$
DECLARE left_kind TEXT; right_kind TEXT; right_visible BOOLEAN;
BEGIN
  IF NEW.relation_kind NOT IN ('ADDRESSES_GAP','TESTS_HYPOTHESIS','PRESUPPOSES') THEN
    RETURN NEW;
  END IF;
  SELECT object_kind INTO left_kind FROM niccolo.epistemic_objects
    WHERE workspace_id=NEW.workspace_id AND object_id=NEW.left_object_id;
  SELECT object_kind INTO right_kind FROM niccolo.epistemic_objects
    WHERE workspace_id=NEW.workspace_id AND object_id=NEW.right_object_id;
  IF left_kind IS DISTINCT FROM 'QUESTION' OR NOT EXISTS (
    SELECT 1 FROM niccolo.epistemic_questions WHERE workspace_id=NEW.workspace_id
      AND situation_id=NEW.situation_id AND question_id=NEW.left_object_id
  ) THEN
    RAISE EXCEPTION 'investigative relation requires a Situation-visible question' USING ERRCODE='23514';
  END IF;
  IF (NEW.relation_kind='ADDRESSES_GAP' AND right_kind IS DISTINCT FROM 'INFORMATION_GAP') OR
     (NEW.relation_kind='TESTS_HYPOTHESIS' AND right_kind IS DISTINCT FROM 'HYPOTHESIS') OR
     (NEW.relation_kind='PRESUPPOSES' AND (right_kind IS NULL OR right_kind NOT IN ('PROPOSITION','ASSUMPTION','HYPOTHESIS'))) THEN
    RAISE EXCEPTION 'invalid investigative relation endpoint' USING ERRCODE='23514';
  END IF;
  right_visible := CASE right_kind
    WHEN 'INFORMATION_GAP' THEN EXISTS(SELECT 1 FROM niccolo.epistemic_information_gaps WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND gap_id=NEW.right_object_id)
    WHEN 'HYPOTHESIS' THEN EXISTS(SELECT 1 FROM niccolo.epistemic_hypotheses WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND hypothesis_id=NEW.right_object_id)
    WHEN 'ASSUMPTION' THEN EXISTS(SELECT 1 FROM niccolo.epistemic_assumptions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND assumption_id=NEW.right_object_id)
    WHEN 'PROPOSITION' THEN EXISTS(SELECT 1 FROM niccolo.epistemic_situation_propositions WHERE workspace_id=NEW.workspace_id AND situation_id=NEW.situation_id AND proposition_id=NEW.right_object_id)
    ELSE FALSE END;
  IF NOT right_visible THEN
    RAISE EXCEPTION 'investigative target not visible in Situation' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER epistemic_investigative_relation_guard BEFORE INSERT OR UPDATE ON niccolo.epistemic_relations
 FOR EACH ROW EXECUTE FUNCTION niccolo.check_investigative_relation();
COMMIT;
