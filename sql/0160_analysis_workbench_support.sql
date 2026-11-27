BEGIN;

ALTER TABLE niccolo.situation_analysis_snapshots
  ADD COLUMN workbench_support_basis JSONB
  CHECK (workbench_support_basis IS NULL OR jsonb_typeof(workbench_support_basis) = 'object');

ALTER TABLE niccolo.situation_analysis_outputs
  ADD COLUMN workbench_support_links JSONB NOT NULL DEFAULT '[]'::jsonb
  CHECK (jsonb_typeof(workbench_support_links) = 'array');

COMMIT;
