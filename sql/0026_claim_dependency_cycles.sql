-- v0.40: DEPENDS_ON edges must remain acyclic within one analysis.
BEGIN;
CREATE OR REPLACE FUNCTION niccolo.prevent_claim_dependency_cycle()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.relation <> 'DEPENDS_ON' THEN
    RETURN NEW;
  END IF;
  IF EXISTS (
    WITH RECURSIVE walk(claim_id) AS (
      SELECT NEW.target_claim_id
      UNION
      SELECT r.target_claim_id
      FROM niccolo.situation_analysis_claim_relations r
      JOIN walk w ON r.source_claim_id = w.claim_id
      WHERE r.workspace_id = NEW.workspace_id
        AND r.situation_id = NEW.situation_id
        AND r.analysis_id = NEW.analysis_id
        AND r.relation = 'DEPENDS_ON'
    )
    SELECT 1 FROM walk WHERE claim_id = NEW.source_claim_id
  ) THEN
    RAISE EXCEPTION 'claim dependency cycle' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER claim_dependency_cycle_guard
BEFORE INSERT ON niccolo.situation_analysis_claim_relations
FOR EACH ROW EXECUTE FUNCTION niccolo.prevent_claim_dependency_cycle();
COMMIT;
