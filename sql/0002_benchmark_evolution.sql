-- Niccolò POC migration 0002: comparative evaluation and improvement tickets.
-- These tables store evaluation metadata, references and independent judgments.
-- Do not store passwords, provider API keys, or copies of original personal data.
-- Raw baseline inputs must be prepared by an isolated authorized input assembler;
-- database metadata alone cannot prove a blind run was truly independent.
BEGIN;
CREATE TABLE IF NOT EXISTS niccolo.evaluation_policies (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  account_subject TEXT NOT NULL,
  background_evaluations_enabled BOOLEAN NOT NULL DEFAULT FALSE,
  extra_model_probes_allowed BOOLEAN NOT NULL DEFAULT FALSE,
  permitted_providers TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
  prohibited_models TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
  monthly_cap_micro_usd BIGINT CHECK (monthly_cap_micro_usd > 0),
  monthly_spent_micro_usd BIGINT NOT NULL DEFAULT 0 CHECK (monthly_spent_micro_usd >= 0),
  monthly_reserved_micro_usd BIGINT NOT NULL DEFAULT 0 CHECK (monthly_reserved_micro_usd >= 0),
  maximum_background_utilization_permille SMALLINT NOT NULL DEFAULT 300 CHECK (maximum_background_utilization_permille BETWEEN 1 AND 1000),
  billing_cycle_start DATE NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY(workspace_id, account_subject)
);
CREATE TABLE IF NOT EXISTS niccolo.benchmark_runs (
  workspace_id UUID NOT NULL REFERENCES niccolo.workspaces(workspace_id),
  run_id UUID NOT NULL,
  account_subject TEXT NOT NULL,
  case_label TEXT NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('CHEAP_VS_FRONTIER', 'NICCOLO_VS_BLIND_FRONTIER_AGENT')),
  status TEXT NOT NULL DEFAULT 'CREATED' CHECK (status IN ('CREATED','RUNNING','AWAITING_INDEPENDENT_REVIEW','REVIEWED','FAILED','CANCELLED')),
  frontier_provider TEXT NOT NULL,
  frontier_model TEXT NOT NULL,
  raw_original_source_ids TEXT[] NOT NULL,
  baseline_recipe JSONB NOT NULL DEFAULT '{}'::jsonb,
  baseline_input_manifest JSONB NOT NULL DEFAULT '{}'::jsonb,
  niccolo_build_id TEXT,
  blind_input_assembler_version TEXT,
  external_egress_authorized BOOLEAN NOT NULL DEFAULT FALSE,
  extra_model_probe_authorized BOOLEAN NOT NULL DEFAULT FALSE,
  independent_adjudication BOOLEAN NOT NULL DEFAULT FALSE,
  projected_cost_micro_usd BIGINT NOT NULL DEFAULT 0 CHECK (projected_cost_micro_usd >= 0),
  actual_cost_micro_usd BIGINT CHECK (actual_cost_micro_usd >= 0),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,
  PRIMARY KEY (workspace_id, run_id),
  FOREIGN KEY(workspace_id, account_subject)
    REFERENCES niccolo.evaluation_policies(workspace_id, account_subject)
);
CREATE INDEX IF NOT EXISTS bench_runs_workspace_created
  ON niccolo.benchmark_runs(workspace_id,created_at DESC);
CREATE TABLE IF NOT EXISTS niccolo.benchmark_scores (
  workspace_id UUID NOT NULL,
  run_id UUID NOT NULL,
  criterion TEXT NOT NULL CHECK (criterion IN (
    'FACTUAL_CORRECTNESS','NESTED_BELIEF_ATTRIBUTION','EPISTEMIC_DISCIPLINE',
    'SOURCE_TRACEABILITY','USEFUL_UNEXPECTED_QUESTIONS','CALIBRATED_UNCERTAINTY')),
  weight SMALLINT NOT NULL CHECK(weight BETWEEN 1 AND 100),
  niccolo_score SMALLINT NOT NULL CHECK(niccolo_score BETWEEN 0 AND 100),
  frontier_agent_score SMALLINT NOT NULL CHECK(frontier_agent_score BETWEEN 0 AND 100),
  independent_reviewer TEXT NOT NULL,
  adjudication_reference TEXT NOT NULL,
  explanation TEXT NOT NULL,
  PRIMARY KEY(workspace_id, run_id, criterion),
  FOREIGN KEY(workspace_id, run_id) REFERENCES niccolo.benchmark_runs(workspace_id,run_id)
);
CREATE TABLE IF NOT EXISTS niccolo.improvement_experiments (
  workspace_id UUID NOT NULL,
  experiment_id UUID NOT NULL,
  run_id UUID NOT NULL,
  status TEXT NOT NULL DEFAULT 'PROPOSED' CHECK (status IN ('PROPOSED','POC_RUNNING','POC_FAILED','POC_PASSED','REVIEWED','REJECTED','ROLLED_OUT')),
  observed_failure TEXT NOT NULL,
  reproduction_recipe TEXT NOT NULL,
  algorithmic_hypothesis TEXT NOT NULL,
  proposed_patch_ref TEXT,
  adversarial_tests_ref TEXT,
  independently_reviewed BOOLEAN NOT NULL DEFAULT FALSE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY(workspace_id, experiment_id),
  FOREIGN KEY(workspace_id, run_id) REFERENCES niccolo.benchmark_runs(workspace_id,run_id)
);
GRANT SELECT, INSERT, UPDATE, DELETE ON
  niccolo.evaluation_policies, niccolo.benchmark_runs, niccolo.benchmark_scores,
  niccolo.improvement_experiments TO niccolo_app;
DO $$
DECLARE tablename TEXT;
BEGIN
  FOREACH tablename IN ARRAY ARRAY[
    'evaluation_policies','benchmark_runs','benchmark_scores','improvement_experiments'
  ] LOOP
    EXECUTE format('ALTER TABLE niccolo.%I ENABLE ROW LEVEL SECURITY', tablename);
    EXECUTE format('ALTER TABLE niccolo.%I FORCE ROW LEVEL SECURITY', tablename);
    EXECUTE format('CREATE POLICY workspace_filter ON niccolo.%I USING (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid) WITH CHECK (workspace_id = NULLIF(current_setting(''niccolo.workspace_id'',true),'''')::uuid)', tablename);
  END LOOP;
END $$;
COMMIT;
