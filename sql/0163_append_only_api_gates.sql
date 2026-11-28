BEGIN;

DO $$
DECLARE
  table_name text;
BEGIN
  FOREACH table_name IN ARRAY ARRAY[
    'workbench_artifact_provenance',
    'public_prominence_records',
    'epistemic_route_use_decisions',
    'artifact_representation_transfers'
  ] LOOP
    EXECUTE format(
      'CREATE POLICY api_write_update_gate ON niccolo.%I AS RESTRICTIVE FOR UPDATE TO niccolo_api USING (false) WITH CHECK (false)',
      table_name
    );
    EXECUTE format(
      'CREATE POLICY api_write_delete_gate ON niccolo.%I AS RESTRICTIVE FOR DELETE TO niccolo_api USING (false)',
      table_name
    );
  END LOOP;
END;
$$;

COMMIT;
