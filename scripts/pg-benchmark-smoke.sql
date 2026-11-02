-- Run in a dedicated temporary database after both migrations.
-- Everything below is rolled back, including role/data operations.
\set ON_ERROR_STOP on
BEGIN;
SET LOCAL ROLE niccolo_app;
SET LOCAL niccolo.workspace_id = 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa';
INSERT INTO niccolo.workspaces(workspace_id,label) VALUES
 ('aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa','Synthetic test workspace');
INSERT INTO niccolo.evaluation_policies(
 workspace_id,account_subject,background_evaluations_enabled,
 extra_model_probes_allowed,permitted_providers,monthly_cap_micro_usd,
 billing_cycle_start
) VALUES(
 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa','demo-user',TRUE,TRUE,
 ARRAY['approved-provider'],10000000,CURRENT_DATE
);
INSERT INTO niccolo.benchmark_runs(
 workspace_id,run_id,account_subject,case_label,mode,frontier_provider,
 frontier_model,raw_original_source_ids,baseline_recipe,external_egress_authorized
) VALUES (
 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa',
 'bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb',
 'demo-user','synthetic only','NICCOLO_VS_BLIND_FRONTIER_AGENT',
 'approved-provider','frontier-synthetic',ARRAY['raw-source-a'],
 '{"tools":["read_original","search_original","private_notes"],"steps":12}'::jsonb,TRUE
);
INSERT INTO niccolo.benchmark_scores(
 workspace_id,run_id,criterion,weight,niccolo_score,frontier_agent_score,
 independent_reviewer,adjudication_reference,explanation
) VALUES (
 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa',
 'bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb',
 'EPISTEMIC_DISCIPLINE',100,75,90,'synthetic-reviewer',
 'synthetic-rubric-001','Synthetic test only; not a model comparison'
);
INSERT INTO niccolo.improvement_experiments(
 workspace_id,experiment_id,run_id,observed_failure,
 reproduction_recipe,algorithmic_hypothesis
) VALUES (
 'aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa',
 'cccccccc-cccc-4ccc-cccc-cccccccccccc',
 'bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb',
 'synthetic gap','replay synthetic inputs','test source lineage improvement'
);
DO $$
DECLARE actual INTEGER;
BEGIN
 SELECT COUNT(*) INTO actual FROM niccolo.benchmark_runs;
 IF actual != 1 THEN RAISE EXCEPTION 'expected 1 own benchmark, saw %', actual; END IF;
END $$;
SET LOCAL niccolo.workspace_id = 'dddddddd-dddd-4ddd-dddd-dddddddddddd';
DO $$
DECLARE actual INTEGER;
BEGIN
 SELECT COUNT(*) INTO actual FROM niccolo.benchmark_runs;
 IF actual != 0 THEN RAISE EXCEPTION 'tenant isolation failed: saw %', actual; END IF;
 SELECT COUNT(*) INTO actual FROM niccolo.benchmark_scores;
 IF actual != 0 THEN RAISE EXCEPTION 'score tenant isolation failed: saw %', actual; END IF;
 SELECT COUNT(*) INTO actual FROM niccolo.improvement_experiments;
 IF actual != 0 THEN RAISE EXCEPTION 'experiment tenant isolation failed: saw %', actual; END IF;
END $$;
ROLLBACK;
SELECT 'benchmark_sql_smoke_passed' AS result;
