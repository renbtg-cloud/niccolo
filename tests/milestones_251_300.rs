#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    agentic_controller, agentic_policy, agentic_runtime,
    analysis_structure::AnalysisScope,
    authz, bff_executor,
    bff_operations::{self, NewOperation, OperationScope},
    model_adapter::{self, DeterministicModel, InvocationScope, ModelRequest},
    reasoning_packet,
    resource_policy::ResourcePolicy,
    runtime_resources::ResourceSnapshot,
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    principal: Uuid,
}
async fn pools() -> (PgPool, PgPool) {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap_or_else(|_| {
            "postgres://oai@127.0.0.1:55432/niccolo_integrated_v250_final".into()
        }))
        .await
        .unwrap();
    let app = PgPoolOptions::new()
        .max_connections(32)
        .connect(
            &std::env::var("NICCOLO_PG_SERVICE_URL").unwrap_or_else(|_| {
                "postgres://niccolo_service_test@127.0.0.1:55432/niccolo_integrated_v250_final"
                    .into()
            }),
        )
        .await
        .unwrap();
    (admin, app)
}
async fn seed(admin: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        analysis: Uuid::new_v4(),
        principal: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'v300')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'Should I proceed?',$3,1)").bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'Should I proceed?',$3,$4,'CREATED')").bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    s
}
async fn op(app: &PgPool, s: Seed) -> OperationScope {
    let scope = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: Uuid::new_v4(),
    };
    bff_operations::submit(
        app,
        scope,
        &NewOperation {
            kind: "AGENTIC_ANALYSIS".into(),
            target_analysis_id: Some(s.analysis),
            parent_operation_id: None,
            idempotency_key: Some(format!("v300-{}", scope.operation)),
            input: json!({"intent":"understand"}),
            deadline_at: None,
            step_budget: 32,
            requested_by: s.principal,
        },
    )
    .await
    .unwrap();
    scope
}

#[tokio::test]
async fn v251_v260_bounded_pull_wait_returns_on_version_change_or_timeout() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    let before = bff_operations::get(&app, scope).await.unwrap();
    let a = app.clone();
    let x = scope;
    let actor = s.principal;
    let task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        bff_operations::start(&a, x, 1, actor).await.unwrap();
    });
    let changed = bff_operations::wait_for_version(&app, scope, before.state_version, 1000)
        .await
        .unwrap();
    task.await.unwrap();
    assert!(changed.state_version > before.state_version);
    assert_eq!(changed.state, "RUNNING");
    let t = std::time::Instant::now();
    let same = bff_operations::wait_for_version(&app, scope, changed.state_version, 40)
        .await
        .unwrap();
    assert_eq!(same.state_version, changed.state_version);
    assert!(t.elapsed() >= std::time::Duration::from_millis(35));
}

#[tokio::test]
async fn v261_v275_policy_is_state_derived_and_preserves_human_ownership() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    authz::with_auth_digest("test-digest".into(), async {
        let steps = agentic_policy::default_plan(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        assert!(steps.iter().any(|x| x.kind == "CHECK_ASSUMPTIONS"));
        assert!(steps.iter().any(|x| x.kind == "CHECK_ALTERNATIVES"));
        assert!(
            steps
                .iter()
                .all(|x| x.kind != "REQUEST_HUMAN_DECISION" || x.requires_human)
        );
        assert_eq!(steps.last().unwrap().kind, "PREPARE_HANDOFF");
    })
    .await;
}

#[tokio::test]
async fn v276_v290_model_invocation_is_hashed_attributed_and_auditable() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(),async {
   let steps=agentic_policy::default_plan(&app,s.workspace,s.situation,s.analysis).await.unwrap();
   let (rev,_)=agentic_controller::create_plan(&app,scope,s.principal,&steps).await.unwrap();
   let first=steps[0].step_id;
   let m=DeterministicModel::new("test","deterministic-v1").unwrap();
   let v=model_adapter::invoke_recorded(&app,InvocationScope{operation:scope,plan_revision:rev,step_id:first},s.principal,&m,&ModelRequest{purpose:"INSPECT_SOURCES".into(),input:json!({"x":1}),output_contract:json!({"type":"object"})}).await.unwrap();
   assert_eq!(v.status,"SUCCEEDED"); assert_eq!(v.request_sha256.len(),64); assert_eq!(v.response_sha256.as_ref().unwrap().len(),64);
   let row:(String,String,String)=sqlx::query_as("SELECT provider,model,status FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND invocation_id=$4").bind(s.workspace).bind(s.situation).bind(scope.operation).bind(v.invocation_id).fetch_one(&admin).await.unwrap();
   assert_eq!(row,("test".into(),"deterministic-v1".into(),"SUCCEEDED".into()));
 }).await;
}

#[tokio::test]
async fn v291_v300_runtime_runs_until_terminal_or_explicit_human_boundary() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let model = DeterministicModel::new("test", "deterministic-v1").unwrap();
        let out = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(out.terminal || out.waiting_for_human);
        let v = agentic_controller::view(&app, scope).await.unwrap();
        assert!(v.budget_used > 0);
        assert!(v.budget_used <= i64::from(v.budget_total));
        if out.terminal {
            assert_eq!(
                bff_operations::get(&app, scope).await.unwrap().state,
                "SUCCEEDED"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn v1361_v1400_stop_check_and_handoff_are_deterministic_not_model_calls() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let model = DeterministicModel::new("test", "deterministic-v1").unwrap();
        let out = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(out.terminal);
        let deterministic_model_calls: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND (request_json LIKE '%CHECK_STOP_CONDITIONS%' OR request_json LIKE '%PREPARE_HANDOFF%')",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(deterministic_model_calls, 0);
        let operation = bff_operations::get(&app, scope).await.unwrap();
        let result = operation.result.expect("terminal agentic result");
        assert_eq!(result["handoff"]["handoff_grants_authority"], false);
        assert_eq!(result["handoff"]["handoff_starts_analysis"], false);
        assert_eq!(result["handoff"]["handoff_executes_actions"], false);
        assert_eq!(
            result["handoff_sha256"].as_str().unwrap().len(),
            64
        );
        let view = agentic_controller::view(&app, scope).await.unwrap();
        for kind in ["CHECK_STOP_CONDITIONS", "PREPARE_HANDOFF"] {
            assert_eq!(
                view.steps.iter().find(|step| step.kind == kind).unwrap().state,
                "SUCCEEDED"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn v326_v350_policy_escalates_recorded_stop_gaps_conflicts_and_source_requests_to_human() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let condition = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.situation_analysis_stop_conditions(workspace_id,situation_id,analysis_id,condition_id,condition_text,escalation_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Do not act without confirmation','Ask the human',$5)")
      .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(condition).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_stop_condition_events(workspace_id,situation_id,analysis_id,condition_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'TRIGGERED','material stop',$6)")
      .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(condition).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Need direct confirmation','message',$5)")
      .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_conflict_sets(workspace_id,situation_id,analysis_id,conflict_id,description,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Two plausible interpretations conflict',$5)")
      .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_source_requests(workspace_id,situation_id,analysis_id,request_id,request_text,source_kind,rationale,proposed_by_principal_id) VALUES($1,$2,$3,$4,'Obtain the missing exchange','message','High information value',$5)")
      .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    authz::with_auth_digest("test-digest".into(), async {
        let sig = agentic_policy::signals(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        assert_eq!(sig.triggered_stop_conditions, 1);
        assert_eq!(sig.open_evidence_gaps, 1);
        assert_eq!(sig.conflict_sets, 1);
        assert_eq!(sig.source_requests, 1);
        let steps = agentic_policy::default_plan(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        assert!(steps.iter().any(|x| x.kind == "CHECK_STOP_CONDITIONS"));
        let gate = steps
            .iter()
            .find(|x| x.kind == "REQUEST_HUMAN_DECISION")
            .expect("human gate");
        assert!(gate.requires_human);
        assert_eq!(steps.last().unwrap().kind, "PREPARE_HANDOFF");
    })
    .await;
}

#[tokio::test]
async fn v351_v375_reasoning_packet_separates_frozen_input_from_current_human_control() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    sqlx::query("INSERT INTO niccolo.situation_human_priorities(workspace_id,situation_id,priority_id,ordinal,priority_text,rationale,declared_by_principal_id) VALUES($1,$2,$3,1,'Do not rush','Human preference',$4)")
      .bind(s.workspace).bind(s.situation).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_human_constraints(workspace_id,situation_id,constraint_id,constraint_kind,constraint_text,declared_by_principal_id) VALUES($1,$2,$3,'MUST_NOT','Do not contact anyone automatically',$4)")
      .bind(s.workspace).bind(s.situation).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_goals(workspace_id,situation_id,goal_id,goal_text,success_criterion,owner_principal_id,created_by_principal_id) VALUES($1,$2,$3,'Understand before acting','Explain uncertainty',$4,$4)")
      .bind(s.workspace).bind(s.situation).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    authz::with_auth_digest("test-digest".into(), async {
        let p = reasoning_packet::build(
            &app,
            AnalysisScope {
                workspace: s.workspace,
                situation: s.situation,
                analysis: s.analysis,
            },
        )
        .await
        .unwrap();
        assert_eq!(p.frozen_input.revision, 1);
        assert!(p.frozen_input.contributions.is_empty());
        assert_eq!(p.current_human_control.priorities.len(), 1);
        assert_eq!(p.current_human_control.constraints.len(), 1);
        assert_eq!(p.current_human_control.goals.len(), 1);
        assert!(
            !p.current_human_control
                .current_control_was_part_of_frozen_input
        );
        assert!(!p.current_human_control.system_owns_project);
        assert!(
            !p.current_human_control
                .system_may_execute_proposed_steps_without_human
        );
        assert!(!p.packet_asserts_truth);
        assert_eq!(reasoning_packet::sha256(&p).unwrap().len(), 64);
    })
    .await;
}

#[tokio::test]
async fn reasoning_packet_round_robins_sources_before_advancing_fragment_depth() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();

    for (document, locator, added_at, prefix) in [
        (first, "fixture://packet-first", "2026-01-01T00:00:00Z", "A"),
        (
            second,
            "fixture://packet-second",
            "2026-01-01T00:00:01Z",
            "B",
        ),
    ] {
        sqlx::query(
            "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length)
             VALUES($1,$2,'WHATSAPP_EXPORT',$3,$4,4)",
        )
        .bind(s.workspace)
        .bind(document)
        .bind(locator)
        .bind(if document == first { "b".repeat(64) } else { "c".repeat(64) })
        .execute(&admin)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO niccolo.situation_sources(workspace_id,situation_id,document_id,added_by_principal_id,added_at)
             VALUES($1,$2,$3,$4,$5::timestamptz)",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(document)
        .bind(s.principal)
        .bind(added_at)
        .execute(&admin)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id)
             VALUES($1,$2,1,$3)",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(document)
        .execute(&admin)
        .await
        .unwrap();

        for ordinal in 0..2_i32 {
            let evidence = Uuid::new_v4();
            let content = format!("{prefix}{ordinal}");
            sqlx::query(
                "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class)
                 VALUES($1,$2,$3,$4,'DIRECT_OBSERVATION')",
            )
            .bind(s.workspace)
            .bind(evidence)
            .bind(format!("{locator}#{ordinal}"))
            .bind(&content)
            .execute(&admin)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label)
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(s.workspace)
            .bind(document)
            .bind(ordinal)
            .bind(evidence)
            .bind(ordinal * 2)
            .bind(ordinal * 2 + 2)
            .bind(format!("{prefix}-speaker"))
            .execute(&admin)
            .await
            .unwrap();
        }
    }

    authz::with_auth_digest("test-digest".into(), async {
        let packet = reasoning_packet::build(
            &app,
            AnalysisScope {
                workspace: s.workspace,
                situation: s.situation,
                analysis: s.analysis,
            },
        )
        .await
        .unwrap();
        let order = packet
            .frozen_input
            .source_fragments
            .iter()
            .map(|fragment| (fragment.document_id, fragment.ordinal))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec![(first, 0), (second, 0), (first, 1), (second, 1)]
        );
    })
    .await;
}

#[tokio::test]
async fn reasoning_packet_clarification_budget_is_explicit_and_deterministic() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    for i in 0..101 {
        let question = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,$3)",
        )
        .bind(s.workspace)
        .bind(question)
        .bind(format!("Clarification {i:03}"))
        .execute(&admin)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,$5,$6)",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(s.analysis)
        .bind(question)
        .bind(format!("Reason {i:03}"))
        .bind(s.principal)
        .execute(&admin)
        .await
        .unwrap();
    }
    authz::with_auth_digest("test-digest".into(), async {
        let scope = AnalysisScope {
            workspace: s.workspace,
            situation: s.situation,
            analysis: s.analysis,
        };
        let first = reasoning_packet::build(&app, scope).await.unwrap();
        let second = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(first.clarifications.len(), 100);
        assert!(first.clarifications_truncated);
        assert_eq!(first.clarifications, second.clarifications);
        assert_eq!(
            reasoning_packet::sha256(&first).unwrap(),
            reasoning_packet::sha256(&second).unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn v376_v400_runtime_model_request_contains_canonical_reasoning_packet() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(),async {
      let model=DeterministicModel::new("test","packet-aware-v1").unwrap();
      let out=agentic_runtime::run_until_boundary(&app,scope,s.principal,&model).await.unwrap();
      assert!(out.terminal || out.waiting_for_human);
      let row:(String,String)=sqlx::query_as("SELECT request_json,request_sha256 FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 ORDER BY created_at LIMIT 1")
        .bind(s.workspace).bind(s.situation).bind(scope.operation).fetch_one(&admin).await.unwrap();
      assert!(row.0.contains("niccolo.reasoning-packet")); assert!(row.0.contains("current_human_control")); assert!(row.0.contains("frozen_input")); assert_eq!(row.1.len(),64);
    }).await;
}

#[derive(Clone)]
struct FlakyModel {
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl model_adapter::ReasoningModel for FlakyModel {
    fn provider(&self) -> &str {
        "test-flaky"
    }
    fn model(&self) -> &str {
        "flaky-v1"
    }
    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> model_adapter::ModelFuture<'a> {
        Box::pin(async move {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 {
                Err(model_adapter::ModelError::Provider("transient".into()))
            } else {
                Ok(model_adapter::ModelResponse {
                    value: json!({"ok":true}),
                    prompt_tokens: Some(3),
                    completion_tokens: Some(2),
                    effective_provider: None,
                    effective_model: None,
                    fallback_used: false,
                })
            }
        })
    }
}

#[derive(Clone)]
struct ClarifyingModel {
    question: Uuid,
}
impl model_adapter::ReasoningModel for ClarifyingModel {
    fn provider(&self) -> &str {
        "test-clarifying"
    }
    fn model(&self) -> &str {
        "clarifying-v1"
    }
    fn invoke<'a>(&'a self, request: &'a ModelRequest) -> model_adapter::ModelFuture<'a> {
        Box::pin(async move {
            let value = match request.purpose.as_str() {
                "SYNTHESIZE" => {
                    let questions = request
                        .input
                        .pointer("/reasoning_packet/clarifications")
                        .and_then(serde_json::Value::as_array)
                        .expect("synthesis receives persisted clarification questions");
                    assert_eq!(questions.len(), 1);
                    assert_eq!(questions[0]["question_id"], self.question.to_string());
                    assert_eq!(
                        questions[0]["text"],
                        "Was the message sent before the decision?"
                    );
                    assert_eq!(
                        request.input["reasoning_packet"]["clarifications_truncated"],
                        false
                    );
                    json!({
                        "purpose":"SYNTHESIZE","status":"ok",
                        "niccolo_artifact":{"answer_text":"Provisional.","coverage_note":"Synthetic clarification test.","claims":[]}
                    })
                }
                "CHALLENGE_SYNTHESIS" => json!({
                    "purpose":"CHALLENGE_SYNTHESIS","status":"ok",
                    "niccolo_challenge":{"challenges":[]}
                }),
                "PROPOSE_CLARIFICATION" => json!({
                    "purpose":"PROPOSE_CLARIFICATION","status":"ok",
                    "niccolo_update":{
                        "used_contribution_ids":[],
                        "assumptions":[],
                        "evidence_gaps":[],
                        "clarifications":[{
                            "question_id":self.question,
                            "text":"Was the message sent before the decision?",
                            "reason":"The ordering could discriminate between interpretations"
                        }]
                    }
                }),
                "ASSESS_CONTEXT" | "CHECK_ASSUMPTIONS" | "CHECK_ALTERNATIVES" => json!({
                    "purpose":request.purpose,"status":"ok",
                    "niccolo_update":{"used_contribution_ids":[],"candidate_interpretations":[],"assumptions":[],"evidence_gaps":[],"clarifications":[]}
                }),
                _ => json!({"purpose":request.purpose,"status":"ok"}),
            };
            Ok(model_adapter::ModelResponse {
                value,
                prompt_tokens: Some(4),
                completion_tokens: Some(2),
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

#[tokio::test]
async fn critical_resource_pressure_skips_only_clarification_pass_and_records_provisional_checkpoint()
 {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Ordering remains unclear','message',$5)")
        .bind(s.workspace)
        .bind(s.situation)
        .bind(s.analysis)
        .bind(Uuid::new_v4())
        .bind(s.principal)
        .execute(&admin)
        .await
        .unwrap();
    let scope = op(&app, s).await;
    let policy = ResourcePolicy::default();
    let resources = ResourceSnapshot {
        platform: "test-linux".into(),
        logical_parallelism: 4,
        process_rss_bytes: Some(900),
        process_peak_rss_bytes: Some(950),
        cgroup_v2: true,
        cgroup_path: Some("/deterministic-test".into()),
        memory_current_bytes: Some(990),
        memory_limit_bytes: Some(1000),
        memory_limit_observed: true,
        memory_limit_unlimited: false,
        memory_headroom_bytes: Some(10),
        memory_pressure_basis_points: Some(9900),
        cpu_quota_millicores: Some(2000),
        cpu_quota_observed: true,
        cpu_quota_unlimited: false,
        cpuset_cpu_count: Some(2),
        effective_cpu_capacity_millicores: 2000,
        cpu_throttled_events: None,
        cpu_throttled_usec: None,
        pids_current: Some(8),
        pids_limit: Some(256),
        pids_limit_observed: true,
        pids_limit_unlimited: false,
        gpu_observation_implemented: false,
        mutates_resource_limits: false,
        establishes_epistemic_quality: false,
    };
    let model = DeterministicModel::new("test", "resource-degradation-v1").unwrap();

    authz::with_auth_digest("test-digest".into(), async {
        let outcome = agentic_runtime::run_until_boundary_with_resource_snapshot(
            &app,
            scope,
            s.principal,
            &model,
            &policy,
            &resources,
        )
        .await
        .unwrap();
        assert!(outcome.waiting_for_human);

        let controller = agentic_controller::view(&app, scope).await.unwrap();
        let clarification = controller
            .steps
            .iter()
            .find(|step| step.kind == "PROPOSE_CLARIFICATION")
            .expect("clarification step planned");
        assert_eq!(clarification.state, "SKIPPED");
        assert_eq!(
            controller
                .steps
                .iter()
                .find(|step| step.kind == "SYNTHESIZE")
                .unwrap()
                .state,
            "SUCCEEDED"
        );
        assert_eq!(
            controller
                .steps
                .iter()
                .find(|step| step.kind == "CHALLENGE_SYNTHESIS")
                .unwrap()
                .state,
            "SUCCEEDED"
        );

        let checkpoint: (String, bool, bool, String) = sqlx::query_as(
            "SELECT checkpoint_kind,provisional,terminal,summary_json::text FROM niccolo.bff_operation_checkpoints WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND checkpoint_kind='RESOURCE_DEGRADATION'",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(checkpoint.0, "RESOURCE_DEGRADATION");
        assert!(checkpoint.1);
        assert!(!checkpoint.2);
        assert!(checkpoint.3.contains("CRITICAL_RESOURCE_PRESSURE"));
        assert!(checkpoint
            .3
            .contains("resource_pressure_is_not_epistemic_quality"));

        let clarification_invocations: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM niccolo.model_invocations m JOIN niccolo.agentic_steps s ON s.workspace_id=m.workspace_id AND s.situation_id=m.situation_id AND s.operation_id=m.operation_id AND s.plan_revision=m.plan_revision AND s.step_id=m.step_id WHERE m.workspace_id=$1 AND m.situation_id=$2 AND m.operation_id=$3 AND s.step_kind='PROPOSE_CLARIFICATION'",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(clarification_invocations, 0);

        let human = controller
            .steps
            .iter()
            .find(|step| step.state == "WAITING_FOR_HUMAN")
            .expect("resource-degraded plan still reaches explicit human control");
        agentic_controller::resolve_human_boundary(
            &app,
            scope,
            human.step_id,
            Uuid::new_v4(),
            s.principal,
            "CONTINUE",
            "Continue with a visibly provisional resource-degraded handoff.",
        )
        .await
        .unwrap();
        let done = agentic_runtime::run_until_boundary_with_resource_snapshot(
            &app,
            scope,
            s.principal,
            &model,
            &policy,
            &resources,
        )
        .await
        .unwrap();
        assert!(done.terminal);
        let operation = bff_operations::get(&app, scope).await.unwrap();
        let result = operation.result.expect("terminal operation has result");
        assert_eq!(
            result["completion_status"],
            "DEGRADED_RESOURCE_PRESSURE"
        );
        assert_eq!(result["provisional"], true);
        assert_eq!(result["resource_pressure_is_not_epistemic_quality"], true);
        assert_eq!(
            result["resource_degraded_steps"][0]["kind"],
            "PROPOSE_CLARIFICATION"
        );
    })
    .await;
}

#[tokio::test]
async fn v1321_v1360_runtime_persists_model_proposed_clarification_before_human_handoff() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Ordering remains unclear','message',$5)")
        .bind(s.workspace)
        .bind(s.situation)
        .bind(s.analysis)
        .bind(Uuid::new_v4())
        .bind(s.principal)
        .execute(&admin)
        .await
        .unwrap();
    let scope = op(&app, s).await;
    let question = Uuid::new_v4();
    authz::with_auth_digest("test-digest".into(), async {
        let model = ClarifyingModel { question };
        let out = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(out.waiting_for_human);
        let row: (String, String) = sqlx::query_as(
            "SELECT q.question,c.reason FROM niccolo.investigative_questions q JOIN niccolo.situation_analysis_clarification_questions c USING(workspace_id,question_id) WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND c.question_id=$4",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(s.analysis)
        .bind(question)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(row.0, "Was the message sent before the decision?");
        assert_eq!(
            row.1,
            "The ordering could discriminate between interpretations"
        );
        let propose_invocations: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND status='SUCCEEDED' AND request_json LIKE '%PROPOSE_CLARIFICATION%'",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(propose_invocations, 1);
    })
    .await;
}

#[tokio::test]
async fn v401_v450_model_governance_bounds_retry_timeout_and_token_budget() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(),async {
   let steps=agentic_policy::default_plan(&app,s.workspace,s.situation,s.analysis).await.unwrap();
   let (rev,_)=agentic_controller::create_plan(&app,scope,s.principal,&steps).await.unwrap();
   let calls=std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)); let model=FlakyModel{calls:calls.clone()};
   let policy=model_adapter::InvocationPolicy{timeout_ms:1000,max_attempts:2,max_total_tokens:Some(10)};
   let v=model_adapter::invoke_recorded_with_policy(&app,InvocationScope{operation:scope,plan_revision:rev,step_id:steps[0].step_id},s.principal,&model,&ModelRequest{purpose:"governed".into(),input:json!({}),output_contract:json!({})},policy).await.unwrap();
   assert_eq!(v.status,"SUCCEEDED"); assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst),2);
   let row:(i32,Option<i32>,Option<i64>,bool)=sqlx::query_as("SELECT attempt_count,timeout_ms,max_total_tokens,budget_exceeded FROM niccolo.model_invocations WHERE invocation_id=$1").bind(v.invocation_id).fetch_one(&admin).await.unwrap();
   assert_eq!(row,(2,Some(1000),Some(10),false));
 }).await;
}

#[tokio::test]
async fn v451_v500_investigation_queue_is_bounded_ranked_and_never_executes_external_actions() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let c1 = Uuid::new_v4();
    let c2 = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'Provisional answer','fixture coverage',$4)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.principal).execute(&admin).await.unwrap();
    for (id, ordinal, text) in [(c1, 1, "A may explain it"), (c2, 2, "B may explain it")] {
        sqlx::query("INSERT INTO niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,created_by_principal_id) VALUES($1,$2,$3,$4,$5,'INFERENCE',$6,$7)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(id).bind(ordinal).bind(text).bind(s.principal).execute(&admin).await.unwrap();
    }
    sqlx::query("INSERT INTO niccolo.situation_analysis_conflict_sets(workspace_id,situation_id,analysis_id,conflict_id,description,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Competing explanations',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    let conflict:Uuid=sqlx::query_scalar("SELECT conflict_id FROM niccolo.situation_analysis_conflict_sets WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 LIMIT 1").bind(s.workspace).bind(s.situation).bind(s.analysis).fetch_one(&admin).await.unwrap();
    for claim in [c1, c2] {
        sqlx::query("INSERT INTO niccolo.situation_analysis_conflict_claims(workspace_id,situation_id,analysis_id,conflict_id,claim_id) VALUES($1,$2,$3,$4,$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(conflict).bind(claim).execute(&admin).await.unwrap();
    }
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,claim_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'Need direct corroboration','message',$6)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(c1).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_source_requests(workspace_id,situation_id,analysis_id,request_id,request_text,source_kind,rationale,proposed_by_principal_id) VALUES($1,$2,$3,$4,'Ask user to upload the missing exchange','message','Could discriminate between A and B',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    authz::with_auth_digest("test-digest".into(), async {
        let q = niccolo_evidence::investigation_policy::queue(&app, scope, 2)
            .await
            .unwrap();
        assert_eq!(q.items.len(), 2);
        assert!(q.candidate_count >= 3);
        assert!(q.truncated);
        assert!(!q.ranking_establishes_truth);
        assert!(q.human_owns_external_actions);
        assert!(
            q.items
                .iter()
                .all(|x| x.score_is_heuristic && !x.system_may_execute_external_action)
        );
    })
    .await;
}

#[tokio::test]
async fn v501_v550_material_change_recommends_but_never_secretly_starts_reanalysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    authz::with_auth_digest("test-digest".into(), async {
        let before = niccolo_evidence::reanalysis_advisor::assess(&app, scope)
            .await
            .unwrap();
        assert!(!before.recommended);
        niccolo_evidence::situation_store::set_focus(
            &app,
            s.workspace,
            s.principal,
            s.situation,
            "Should I proceed after the new information?",
        )
        .await
        .unwrap();
        let after = niccolo_evidence::reanalysis_advisor::assess(&app, scope)
            .await
            .unwrap();
        assert!(after.recommended);
        assert!(after.materiality_score >= 45);
        assert!(after.human_approval_required);
        assert!(!after.recommendation_starts_analysis);
        assert!(after.old_output_must_be_preserved);
        let suggestion = Uuid::new_v4();
        niccolo_evidence::reanalysis_advisor::persist_suggestion(
            &app,
            scope,
            suggestion,
            s.principal,
            &after,
        )
        .await
        .unwrap();
        let statuses = niccolo_evidence::reanalysis_control::suggestion_statuses(&app, scope)
            .await
            .unwrap();
        let saved = statuses
            .iter()
            .find(|x| x.suggestion_id == suggestion)
            .unwrap();
        assert!(saved.latest_decision.is_none());
        assert!(!saved.requested_means_analysis_started);
    })
    .await;
    let analysis_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2").bind(s.workspace).bind(s.situation).fetch_one(&admin).await.unwrap();
    assert_eq!(analysis_count, 1);
}

#[tokio::test]
async fn v551_v600_bff_dashboard_aggregates_domain_state_without_making_browser_authoritative() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    authz::with_auth_digest("test-digest".into(), async {
        let d = niccolo_evidence::bff_dashboard::get(&app, scope, None)
            .await
            .unwrap();
        assert_eq!(d.situation_id, s.situation);
        assert_eq!(d.analysis.analysis_id, s.analysis);
        assert!(d.browser_may_render_simple_views);
        assert!(!d.browser_is_domain_authority);
        assert!(!d.assembled_atomically);
        assert!(d.operation.is_none());
        assert!(d.controller.is_none());
    })
    .await;
}

#[tokio::test]
async fn v601_v700_synthesis_artifact_is_atomic_typed_and_feeds_epistemic_state() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let claim = Uuid::new_v4();
    let assumption = Uuid::new_v4();
    let alt = Uuid::new_v4();
    let gap = Uuid::new_v4();
    let question = Uuid::new_v4();
    let artifact = niccolo_evidence::model_artifacts::SynthesisArtifact {
        answer_text: "Provisional: A is plausible, B remains possible.".into(),
        coverage_note: "Based on frozen input only.".into(),
        claims: vec![niccolo_evidence::model_artifacts::ArtifactClaim {
            claim_id: claim,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "A is plausible".into(),
            qualification: Some("not established".into()),
        }],
        source_links: vec![],
        context_links: vec![],
        assumptions: vec![niccolo_evidence::model_artifacts::ArtifactAssumption {
            assumption_id: assumption,
            claim_id: Some(claim),
            statement: "The report is contemporaneous".into(),
            basis_note: "Timing metadata".into(),
        }],
        alternatives: vec![niccolo_evidence::model_artifacts::ArtifactAlternative {
            alternative_id: alt,
            claim_id: claim,
            text: "B explains the same pattern".into(),
            rationale: "Evidence is not discriminating yet".into(),
        }],
        evidence_gaps: vec![niccolo_evidence::model_artifacts::ArtifactGap {
            gap_id: gap,
            claim_id: Some(claim),
            description: "Need direct corroboration".into(),
            requested_source_kind: Some("message".into()),
        }],
        clarifications: vec![niccolo_evidence::model_artifacts::ArtifactClarification {
            question_id: question,
            text: "Was this before or after the meeting?".into(),
            reason: "Timing could discriminate A from B".into(),
        }],
    };
    authz::with_auth_digest("test-digest".into(), async {
        let applied = niccolo_evidence::model_artifacts::apply(&app, scope, s.principal, &artifact)
            .await
            .unwrap();
        assert_eq!(
            (
                applied.claim_count,
                applied.assumption_count,
                applied.alternative_count,
                applied.evidence_gap_count,
                applied.clarification_count
            ),
            (1, 1, 1, 1, 1)
        );
        assert!(!applied.recorded_output_is_truth);
    })
    .await;
    let counts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3),(SELECT count(*) FROM niccolo.situation_analysis_assumptions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3),(SELECT count(*) FROM niccolo.situation_analysis_claim_alternatives WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3),(SELECT count(*) FROM niccolo.situation_analysis_evidence_gaps WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3),(SELECT count(*) FROM niccolo.situation_analysis_clarification_questions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)").bind(s.workspace).bind(s.situation).bind(s.analysis).fetch_one(&admin).await.unwrap();
    assert_eq!(counts, (1, 1, 1, 1, 1));
    let s2 = seed(&admin).await;
    let invalid = niccolo_evidence::model_artifacts::SynthesisArtifact {
        answer_text: "x".into(),
        coverage_note: "y".into(),
        claims: vec![
            niccolo_evidence::model_artifacts::ArtifactClaim {
                claim_id: Uuid::new_v4(),
                ordinal: 1,
                kind: "INFERENCE".into(),
                text: "one".into(),
                qualification: None,
            },
            niccolo_evidence::model_artifacts::ArtifactClaim {
                claim_id: Uuid::new_v4(),
                ordinal: 1,
                kind: "INFERENCE".into(),
                text: "duplicate ordinal".into(),
                qualification: None,
            },
        ],
        source_links: vec![],
        context_links: vec![],
        assumptions: vec![],
        alternatives: vec![],
        evidence_gaps: vec![],
        clarifications: vec![],
    };
    authz::with_auth_digest("test-digest".into(), async {
        assert!(
            niccolo_evidence::model_artifacts::apply(
                &app,
                AnalysisScope {
                    workspace: s2.workspace,
                    situation: s2.situation,
                    analysis: s2.analysis
                },
                s2.principal,
                &invalid
            )
            .await
            .is_err()
        );
    })
    .await;
    let output_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_outputs WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(s2.workspace).bind(s2.situation).bind(s2.analysis).fetch_one(&admin).await.unwrap();
    assert_eq!(output_count, 0);
}

#[tokio::test]
async fn v1201_v1240_synthesis_artifact_claim_links_require_examined_frozen_evidence() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let document = Uuid::new_v4();
    let evidence = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','provider://frozen/1',$3,24)",
    )
    .bind(s.workspace)
    .bind(document)
    .bind("b".repeat(64))
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'provider://frozen/1','Frozen provider evidence','DIRECT_OBSERVATION')",
    )
    .bind(s.workspace)
    .bind(evidence)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,24,'Fixture')",
    )
    .bind(s.workspace)
    .bind(document)
    .bind(evidence)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id) VALUES($1,$2,1,$3)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(document)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(evidence)
    .bind(s.principal)
    .execute(&admin)
    .await
    .unwrap();

    let claim = Uuid::new_v4();
    let artifact = niccolo_evidence::model_artifacts::SynthesisArtifact {
        answer_text: "A claim with explicit frozen-source provenance.".into(),
        coverage_note: "One examined fragment.".into(),
        claims: vec![niccolo_evidence::model_artifacts::ArtifactClaim {
            claim_id: claim,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "The frozen fragment supports this interpretation".into(),
            qualification: Some("support as used is not proof".into()),
        }],
        source_links: vec![niccolo_evidence::model_artifacts::ArtifactSourceLink {
            claim_id: claim,
            evidence_id: evidence,
            relation: "SUPPORTS_AS_USED".into(),
        }],
        context_links: vec![],
        assumptions: vec![],
        alternatives: vec![],
        evidence_gaps: vec![],
        clarifications: vec![],
    };

    authz::with_auth_digest("test-digest".into(), async {
        let applied = niccolo_evidence::model_artifacts::apply(&app, scope, s.principal, &artifact)
            .await
            .unwrap();
        assert_eq!(applied.source_link_count, 1);
        assert!(!applied.recorded_output_is_truth);
    })
    .await;

    let stored: (String, Uuid) = sqlx::query_as(
        "SELECT relation,recorded_by_principal_id FROM niccolo.situation_analysis_claim_fragments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND evidence_id=$5",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(claim)
    .bind(evidence)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(stored, ("SUPPORTS_AS_USED".into(), s.principal));

    let s2 = seed(&admin).await;
    let invalid_claim = Uuid::new_v4();
    let invalid = niccolo_evidence::model_artifacts::SynthesisArtifact {
        answer_text: "Must roll back.".into(),
        coverage_note: "Invalid source link.".into(),
        claims: vec![niccolo_evidence::model_artifacts::ArtifactClaim {
            claim_id: invalid_claim,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "Cannot cite evidence that was not examined".into(),
            qualification: None,
        }],
        source_links: vec![niccolo_evidence::model_artifacts::ArtifactSourceLink {
            claim_id: invalid_claim,
            evidence_id: Uuid::new_v4(),
            relation: "SUPPORTS_AS_USED".into(),
        }],
        context_links: vec![],
        assumptions: vec![],
        alternatives: vec![],
        evidence_gaps: vec![],
        clarifications: vec![],
    };
    let failed = authz::with_auth_digest("test-digest".into(), async {
        niccolo_evidence::model_artifacts::apply(
            &app,
            AnalysisScope {
                workspace: s2.workspace,
                situation: s2.situation,
                analysis: s2.analysis,
            },
            s2.principal,
            &invalid,
        )
        .await
    })
    .await;
    assert!(failed.is_err());
    let output_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_outputs WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s2.workspace)
    .bind(s2.situation)
    .bind(s2.analysis)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(output_count, 0);
}

#[tokio::test]
async fn v701_v800_reanalysis_is_explicit_bff_task_and_preserves_old_analysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let operation = Uuid::new_v4();
    let scope = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation,
    };
    let new_analysis = Uuid::new_v4();
    authz::with_auth_digest("test-digest".into(),async {
  niccolo_evidence::situation_store::set_focus(&app,s.workspace,s.principal,s.situation,"Changed focus after human correction").await.unwrap();
  bff_operations::submit(&app,scope,&NewOperation{kind:"REANALYZE".into(),target_analysis_id:Some(s.analysis),parent_operation_id:None,idempotency_key:Some(format!("reanalyze-{operation}")),input:json!({"new_analysis_id":new_analysis,"goal":"Re-evaluate after the human correction"}),deadline_at:None,step_budget:8,requested_by:s.principal}).await.unwrap();
  let out=niccolo_evidence::reanalysis_runtime::execute(&app,scope,s.principal).await.unwrap();assert_eq!(out.state,"SUCCEEDED");assert_eq!(out.result.as_ref().unwrap()["new_analysis_id"],json!(new_analysis));assert_eq!(out.result.as_ref().unwrap()["historical_output_preserved"],json!(true));assert_eq!(out.result.as_ref().unwrap()["agentic_execution_started"],json!(false));
  let snap=niccolo_evidence::analysis_snapshot::get(&app,s.workspace,s.situation,new_analysis).await.unwrap();assert_eq!(snap.reanalysis_of,Some(s.analysis));assert_eq!(snap.input_revision,2);
 }).await;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2").bind(s.workspace).bind(s.situation).fetch_one(&admin).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn v801_v900_intermediate_updates_feed_later_reasoning_and_quality_without_becoming_truth() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let assumption = Uuid::new_v4();
    let gap = Uuid::new_v4();
    let question = Uuid::new_v4();
    let update = niccolo_evidence::reasoning_updates::ReasoningUpdate {
        used_contribution_ids: vec![],
        candidate_interpretations: vec![],
        assumptions: vec![niccolo_evidence::reasoning_updates::UpdateAssumption {
            assumption_id: assumption,
            statement: "The timeline may be incomplete".into(),
            basis_note: "Only one side is represented".into(),
        }],
        evidence_gaps: vec![niccolo_evidence::reasoning_updates::UpdateGap {
            gap_id: gap,
            description: "Need the other participant's contemporaneous message".into(),
            requested_source_kind: Some("message".into()),
        }],
        clarifications: vec![niccolo_evidence::reasoning_updates::UpdateClarification {
            question_id: question,
            text: "Did this happen before or after the call?".into(),
            reason: "Sequence changes the competing explanations".into(),
            discriminates_interpretation_ids: vec![],
            possible_outcomes: vec![],
            decision_relevance: None,
            acquisition_cost: None,
            cheaper_alternative: None,
        }],
        source_queries: vec![],
    };
    authz::with_auth_digest("test-digest".into(), async {
        let applied = niccolo_evidence::reasoning_updates::apply(&app, scope, s.principal, &update)
            .await
            .unwrap();
        assert_eq!(
            (
                applied.assumption_count,
                applied.evidence_gap_count,
                applied.clarification_count
            ),
            (1, 1, 1)
        );
        assert!(!applied.update_asserts_truth);
        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert!(
            packet
                .lens
                .assumptions
                .iter()
                .any(|a| a.assumption_id == assumption)
        );
        assert!(packet.evidence_gaps.iter().any(|g| g.gap_id == gap));
        let q = niccolo_evidence::analysis_quality::report(&app, scope)
            .await
            .unwrap();
        assert!(q.provisional);
        assert!(q.human_attention_recommended);
        assert_eq!(q.open_evidence_gap_count, 1);
        assert_eq!(q.clarification_count, 1);
        assert!(!q.completeness_is_truth_score);
    })
    .await;
}

#[tokio::test]
async fn v1281_v1320_model_reported_context_usage_rejects_non_frozen_contribution_atomically() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let assumption = Uuid::new_v4();
    let invalid = niccolo_evidence::reasoning_updates::ReasoningUpdate {
        used_contribution_ids: vec![Uuid::new_v4()],
        candidate_interpretations: vec![],
        assumptions: vec![niccolo_evidence::reasoning_updates::UpdateAssumption {
            assumption_id: assumption,
            statement: "Must roll back with invalid human context provenance".into(),
            basis_note: "The provider invented a contribution identifier".into(),
        }],
        evidence_gaps: vec![],
        clarifications: vec![],
        source_queries: vec![],
    };
    let result = authz::with_auth_digest("test-digest".into(), async {
        niccolo_evidence::reasoning_updates::apply(&app, scope, s.principal, &invalid).await
    })
    .await;
    assert!(result.is_err());
    let assumption_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_assumptions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND assumption_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(assumption)
    .fetch_one(&admin)
    .await
    .unwrap();
    let used_context_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_used_contributions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(assumption_count, 0);
    assert_eq!(used_context_count, 0);
}

#[tokio::test]
async fn candidate_interpretation_replay_conflict_preserves_original_non_fact_state() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let interpretation_id = Uuid::new_v4();
    let original = niccolo_evidence::reasoning_updates::ReasoningUpdate {
        used_contribution_ids: vec![],
        candidate_interpretations: vec![
            niccolo_evidence::reasoning_updates::UpdateInterpretation {
                interpretation_id,
                text: "A benign coordination failure remains plausible".into(),
                rationale: "The available record does not establish intent".into(),
                predictions: vec![],
                evidence_links: vec![],
                relations: vec![],
            },
        ],
        assumptions: vec![],
        evidence_gaps: vec![],
        clarifications: vec![],
        source_queries: vec![],
    };
    authz::with_auth_digest("test-digest".into(), async {
        let applied =
            niccolo_evidence::reasoning_updates::apply(&app, scope, s.principal, &original)
                .await
                .unwrap();
        assert_eq!(applied.candidate_interpretation_count, 1);
        assert!(!applied.update_asserts_truth);

        let conflicting = niccolo_evidence::reasoning_updates::ReasoningUpdate {
            used_contribution_ids: vec![],
            candidate_interpretations: vec![
                niccolo_evidence::reasoning_updates::UpdateInterpretation {
                    interpretation_id,
                    text: "A different interpretation with the same identifier".into(),
                    rationale: "This replay must not overwrite history".into(),
                    predictions: vec![],
                    evidence_links: vec![],
                    relations: vec![],
                },
            ],
            assumptions: vec![],
            evidence_gaps: vec![],
            clarifications: vec![],
            source_queries: vec![],
        };
        assert!(
            niccolo_evidence::reasoning_updates::apply(&app, scope, s.principal, &conflicting,)
                .await
                .is_err()
        );
        let rows = niccolo_evidence::analysis_assumptions::candidate_interpretations(&app, scope)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].interpretation_id, interpretation_id);
        assert_eq!(
            rows[0].text,
            "A benign coordination failure remains plausible"
        );
        assert!(!rows[0].is_fact);
        assert!(!rows[0].automatically_ranked);
    })
    .await;

    let persisted: (String, String) = sqlx::query_as(
        "SELECT interpretation_text,rationale FROM niccolo.situation_analysis_candidate_interpretations WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND interpretation_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(interpretation_id)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(
        persisted.0,
        "A benign coordination failure remains plausible"
    );
    assert_eq!(
        persisted.1,
        "The available record does not establish intent"
    );
}

#[tokio::test]
async fn v901_v1000_adversarial_challenge_is_persisted_reviewable_and_never_truth_probability() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let challenge = Uuid::new_v4();
    let artifact = niccolo_evidence::adversarial_challenge::ChallengeArtifact {
        challenges: vec![niccolo_evidence::adversarial_challenge::ChallengeItem {
            challenge_id: challenge,
            kind: "ALTERNATIVE".into(),
            text: "The same observations are also consistent with a benign coordination failure."
                .into(),
            materiality: 80,
        }],
    };
    authz::with_auth_digest("test-digest".into(), async {
        let steps = agentic_policy::default_plan(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        let synth = steps.iter().position(|x| x.kind == "SYNTHESIZE").unwrap();
        let ch = steps
            .iter()
            .position(|x| x.kind == "CHALLENGE_SYNTHESIS")
            .unwrap();
        assert!(ch > synth);
        assert_eq!(
            niccolo_evidence::adversarial_challenge::apply(&app, scope, s.principal, &artifact)
                .await
                .unwrap(),
            1
        );
        let rows = niccolo_evidence::adversarial_challenge::list(&app, scope)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].open);
        assert!(!rows[0].materiality_is_truth_probability);
        assert!(!rows[0].challenge_establishes_error);
        let q = niccolo_evidence::analysis_quality::report(&app, scope)
            .await
            .unwrap();
        assert_eq!(q.adversarial_challenge_count, 1);
        assert_eq!(q.open_material_challenge_count, 1);
        assert!(q.provisional);
        niccolo_evidence::adversarial_challenge::event(
            &app,
            scope,
            challenge,
            Uuid::new_v4(),
            "WAIVED",
            "Human reviewed this challenge and chose not to pursue it in this analysis.",
            s.principal,
        )
        .await
        .unwrap();
        let rows = niccolo_evidence::adversarial_challenge::list(&app, scope)
            .await
            .unwrap();
        assert!(!rows[0].open);
        let q = niccolo_evidence::analysis_quality::report(&app, scope)
            .await
            .unwrap();
        assert_eq!(q.open_material_challenge_count, 0);
    })
    .await;
}

#[tokio::test]
async fn v1001_v1100_human_boundary_is_durable_and_explicitly_resumes_without_inferred_consent() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    let analysis_scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,description,recorded_by_principal_id) VALUES($1,$2,$3,$4,'Missing source before decision',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(s.principal).execute(&admin).await.unwrap();
    authz::with_auth_digest("test-digest".into(), async {
        let model = DeterministicModel::new("test", "deterministic-v1").unwrap();
        let first = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(first.waiting_for_human);
        assert_eq!(first.state, "WAITING_FOR_HUMAN");
        let view = agentic_controller::view(&app, scope).await.unwrap();
        let human = view
            .steps
            .iter()
            .find(|x| x.state == "WAITING_FOR_HUMAN")
            .unwrap();
        let r = agentic_controller::resolve_human_boundary(
            &app,
            scope,
            human.step_id,
            Uuid::new_v4(),
            s.principal,
            "CONTINUE",
            "I understand the unresolved gap; continue with a provisional handoff.",
        )
        .await
        .unwrap();
        assert!(r.human_response_persisted);
        assert!(!r.system_inferred_consent);
        assert_eq!(r.operation_state, "RUNNING");
        let second = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(second.terminal);
        assert_eq!(second.state, "SUCCEEDED");
        let q = niccolo_evidence::analysis_quality::report(&app, analysis_scope)
            .await
            .unwrap();
        assert!(q.provisional);
    })
    .await;
    let n:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.agentic_human_responses WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3").bind(s.workspace).bind(s.situation).bind(scope.operation).fetch_one(&admin).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn v1101_v1200_operation_budget_is_cumulative_bounded_and_not_a_quality_score() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: Uuid::new_v4(),
    };
    authz::with_auth_digest("test-digest".into(), async {
        bff_operations::submit(
            &app,
            scope,
            &NewOperation {
                kind: "AGENTIC_ANALYSIS".into(),
                target_analysis_id: Some(s.analysis),
                parent_operation_id: None,
                idempotency_key: Some(format!("budget-{}", scope.operation)),
                input: json!({"model_budget":{"max_invocations":1,"max_total_tokens":10}}),
                deadline_at: None,
                step_budget: 32,
                requested_by: s.principal,
            },
        )
        .await
        .unwrap();
        let before = niccolo_evidence::operation_budget::require_available(&app, scope)
            .await
            .unwrap();
        assert_eq!(before.max_invocations, 1);
        assert_eq!(before.used_invocations, 0);
        assert!(!before.budget_is_quality_score);
        let steps = agentic_policy::default_plan(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        let (rev, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
            .await
            .unwrap();
        let model = DeterministicModel::new("test", "budget-model").unwrap();
        model_adapter::invoke_recorded(
            &app,
            InvocationScope {
                operation: scope,
                plan_revision: rev,
                step_id: steps[0].step_id,
            },
            s.principal,
            &model,
            &ModelRequest {
                purpose: "INSPECT_SOURCES".into(),
                input: json!({}),
                output_contract: json!({}),
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            niccolo_evidence::operation_budget::require_available(&app, scope).await,
            Err(niccolo_evidence::operation_budget::BudgetError::Invocations)
        ));
    })
    .await;
}

#[tokio::test]
async fn provider_controlled_failure_text_never_enters_model_invocation_ledger() {
    #[derive(Clone)]
    struct SensitiveFailure;
    impl model_adapter::ReasoningModel for SensitiveFailure {
        fn provider(&self) -> &str {
            "sensitive-test-provider"
        }
        fn model(&self) -> &str {
            "sensitive-test-model"
        }
        fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> model_adapter::ModelFuture<'a> {
            Box::pin(async {
                Err(model_adapter::ModelError::Provider(
                    "Bearer secret-token; private source sentence".into(),
                ))
            })
        }
    }

    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let steps = agentic_policy::default_plan(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        let (rev, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
            .await
            .unwrap();
        assert!(
            model_adapter::invoke_recorded_with_policy(
                &app,
                InvocationScope {
                    operation: scope,
                    plan_revision: rev,
                    step_id: steps[0].step_id,
                },
                s.principal,
                &SensitiveFailure,
                &ModelRequest {
                    purpose: "INSPECT_SOURCES".into(),
                    input: json!({}),
                    output_contract: json!({}),
                },
                model_adapter::InvocationPolicy {
                    timeout_ms: 1000,
                    max_attempts: 1,
                    max_total_tokens: None,
                },
            )
            .await
            .is_err()
        );
        let (code, message): (String, String) = sqlx::query_as(
            "SELECT error_code,error_message FROM niccolo.model_invocations
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(code, "PROVIDER_ERROR");
        assert_eq!(message, "model provider invocation failed");
        assert!(!message.contains("secret-token"));
        assert!(!message.contains("private source"));
    })
    .await;
}

#[tokio::test]
async fn v1201_v1300_failover_records_the_provider_that_actually_answered() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(),async {
  let steps=agentic_policy::default_plan(&app,s.workspace,s.situation,s.analysis).await.unwrap();let(rev,_)=agentic_controller::create_plan(&app,scope,s.principal,&steps).await.unwrap();
  let primary:std::sync::Arc<dyn model_adapter::ReasoningModel>=std::sync::Arc::new(model_adapter::UnavailableModel);
  let fallback:std::sync::Arc<dyn model_adapter::ReasoningModel>=std::sync::Arc::new(DeterministicModel::new("fallback-provider","fallback-model").unwrap());
  let router=model_adapter::FailoverModel::new(primary,fallback);
  let v=model_adapter::invoke_recorded(&app,InvocationScope{operation:scope,plan_revision:rev,step_id:steps[0].step_id},s.principal,&router,&ModelRequest{purpose:"INSPECT_SOURCES".into(),input:json!({}),output_contract:json!({})}).await.unwrap();
  assert_eq!(v.provider,"niccolo-router");assert_eq!(v.effective_provider.as_deref(),Some("fallback-provider"));assert_eq!(v.effective_model.as_deref(),Some("fallback-model"));assert!(v.fallback_used);
  let row:(Option<String>,Option<String>,bool)=sqlx::query_as("SELECT effective_provider,effective_model,fallback_used FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND invocation_id=$4").bind(s.workspace).bind(s.situation).bind(scope.operation).bind(v.invocation_id).fetch_one(&admin).await.unwrap();assert_eq!(row,(Some("fallback-provider".into()),Some("fallback-model".into()),true));
  let budget=niccolo_evidence::operation_budget::view(&app,scope).await.unwrap();
  assert_eq!(budget.used_invocations,2);
  assert_eq!(budget.unreported_token_usage_invocations,1);
  assert!(!budget.token_usage_complete);
  assert!(budget.remaining_total_tokens_is_upper_bound);
 }).await;
}

#[tokio::test]
async fn failover_chain_never_starts_when_its_attempt_ceiling_exceeds_remaining_budget() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: Uuid::new_v4(),
    };
    bff_operations::submit(
        &app,
        scope,
        &NewOperation {
            kind: "AGENTIC_ANALYSIS".into(),
            target_analysis_id: Some(s.analysis),
            parent_operation_id: None,
            idempotency_key: Some(format!("failover-budget-{}", scope.operation)),
            input: json!({
                "intent":"understand",
                "model_budget":{"max_invocations":1,"max_total_tokens":200000}
            }),
            deadline_at: None,
            step_budget: 32,
            requested_by: s.principal,
        },
    )
    .await
    .unwrap();

    let primary: std::sync::Arc<dyn model_adapter::ReasoningModel> =
        std::sync::Arc::new(model_adapter::UnavailableModel);
    let fallback: std::sync::Arc<dyn model_adapter::ReasoningModel> =
        std::sync::Arc::new(DeterministicModel::new("fallback", "fallback").unwrap());
    let router = model_adapter::FailoverModel::new(primary, fallback);
    assert_eq!(router.provider_attempt_ceiling(), 2);

    authz::with_auth_digest("test-digest".into(), async {
        let outcome = agentic_runtime::run_until_boundary(&app, scope, s.principal, &router)
            .await
            .unwrap();
        assert!(outcome.terminal);
        let operation = bff_operations::get(&app, scope).await.unwrap();
        assert_eq!(operation.state, "SUCCEEDED");
        let result = operation.result.expect("partial budget result");
        assert_eq!(result["completion_status"], "PARTIAL_BUDGET_BOUNDARY");
        assert_eq!(result["stop_reason"], "INVOCATION_LIMIT");

        let invocations: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM niccolo.model_invocations
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(invocations, 0);
    })
    .await;
}

#[tokio::test]
async fn failed_primary_and_fallback_are_both_counted_as_provider_attempts() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let steps = agentic_policy::default_plan(&app, s.workspace, s.situation, s.analysis)
            .await
            .unwrap();
        let (rev, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
            .await
            .unwrap();
        let primary: std::sync::Arc<dyn model_adapter::ReasoningModel> =
            std::sync::Arc::new(model_adapter::UnavailableModel);
        let fallback: std::sync::Arc<dyn model_adapter::ReasoningModel> =
            std::sync::Arc::new(model_adapter::UnavailableModel);
        let router = model_adapter::FailoverModel::new(primary, fallback);
        let error = model_adapter::invoke_recorded_with_policy(
            &app,
            InvocationScope {
                operation: scope,
                plan_revision: rev,
                step_id: steps[0].step_id,
            },
            s.principal,
            &router,
            &ModelRequest {
                purpose: "INSPECT_SOURCES".into(),
                input: json!({}),
                output_contract: json!({}),
            },
            model_adapter::InvocationPolicy {
                timeout_ms: 1000,
                max_attempts: 1,
                max_total_tokens: None,
            },
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            model_adapter::ModelError::ProviderChain { attempts: 2, .. }
        ));
        let row: (String, i32, bool) = sqlx::query_as(
            "SELECT status,attempt_count,fallback_used FROM niccolo.model_invocations
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(row, ("FAILED".into(), 2, false));
        let budget = niccolo_evidence::operation_budget::view(&app, scope)
            .await
            .unwrap();
        assert_eq!(budget.used_invocations, 2);
        assert_eq!(budget.unreported_token_usage_invocations, 2);
    })
    .await;
}

#[tokio::test]
async fn v1301_v1400_material_state_discovered_after_planning_creates_versioned_human_gate() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    let ascope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let challenge = Uuid::new_v4();
    authz::with_auth_digest("test-digest".into(), async {
        niccolo_evidence::adversarial_challenge::apply(
            &app,
            ascope,
            s.principal,
            &niccolo_evidence::adversarial_challenge::ChallengeArtifact {
                challenges: vec![niccolo_evidence::adversarial_challenge::ChallengeItem {
                    challenge_id: challenge,
                    kind: "COUNTEREVIDENCE".into(),
                    text: "Material counterevidence remains unresolved".into(),
                    materiality: 90,
                }],
            },
        )
        .await
        .unwrap();
        let model = DeterministicModel::new("test", "dynamic-gate").unwrap();
        let out = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(out.waiting_for_human);
        assert_eq!(out.state, "WAITING_FOR_HUMAN");
        let view = agentic_controller::view(&app, scope).await.unwrap();
        assert_eq!(view.plan_revision, 2);
        let h = view
            .steps
            .iter()
            .find(|x| x.kind == "REQUEST_HUMAN_DECISION")
            .unwrap();
        assert_eq!(h.state, "WAITING_FOR_HUMAN");
        assert!(h.requires_human);
        let r = agentic_controller::resolve_human_boundary(
            &app,
            scope,
            h.step_id,
            Uuid::new_v4(),
            s.principal,
            "CONTINUE",
            "I reviewed the material counterevidence and accept a provisional handoff.",
        )
        .await
        .unwrap();
        assert!(!r.system_inferred_consent);
        let done = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
            .await
            .unwrap();
        assert!(done.terminal);
        assert_eq!(done.state, "SUCCEEDED");
    })
    .await;
}

#[tokio::test]
async fn v1401_v1500_stale_started_step_recovers_without_resetting_completed_work() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(),async {
  let steps=agentic_policy::default_plan(&app,s.workspace,s.situation,s.analysis).await.unwrap();let(rev,_)=agentic_controller::create_plan(&app,scope,s.principal,&steps).await.unwrap();let first=steps[0].step_id;
  agentic_controller::start_step(&app,scope,first,s.principal).await.unwrap();
  sqlx::query("UPDATE niccolo.agentic_step_events SET created_at=clock_timestamp()-interval '10 minutes' WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5 AND event_kind='STARTED'").bind(s.workspace).bind(s.situation).bind(scope.operation).bind(rev).bind(first).execute(&admin).await.unwrap();
  let recovered=agentic_controller::recover_stale_started(&app,scope,s.principal,120).await.unwrap();assert!(recovered.ready_step_ids.contains(&first));assert_eq!(recovered.steps.iter().find(|x|x.step_id==first).unwrap().state,"PENDING");
  agentic_controller::start_step(&app,scope,first,s.principal).await.unwrap();agentic_controller::finish_step(&app,scope,first,s.principal,"SUCCEEDED","retry completed").await.unwrap();
  let again=agentic_controller::recover_stale_started(&app,scope,s.principal,120).await.unwrap();assert_eq!(again.steps.iter().find(|x|x.step_id==first).unwrap().state,"SUCCEEDED");
 }).await;
}

#[tokio::test]
async fn v1601_v1700_proactive_briefing_is_two_or_three_bounded_leads_not_evidence() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    for i in 0..5 {
        sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,description,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(Uuid::new_v4()).bind(format!("Gap {i}")).bind(s.principal).execute(&admin).await.unwrap();
    }
    authz::with_auth_digest("test-digest".into(), async {
        let b = niccolo_evidence::proactive_briefing::build(&app, scope)
            .await
            .unwrap();
        assert_eq!(b.leads.len(), 3);
        assert!(b.truncated);
        assert!(b.user_owns_which_lead_to_pursue);
        assert!(b.leads.iter().all(|x| !x.lead_is_evidence
            && !x.lead_establishes_truth
            && x.user_may_ignore
            && !x.system_may_execute_external_action));
        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(packet.proactive_leads.leads.len(), 3);
    })
    .await;
}

#[tokio::test]
async fn v1701_v1800_reanalysis_explanation_separates_recorded_delta_lineage_and_causality() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let new = Uuid::new_v4();
    authz::with_auth_digest("test-digest".into(), async {
        niccolo_evidence::situation_store::set_focus(
            &app,
            s.workspace,
            s.principal,
            s.situation,
            "Changed focus for reanalysis",
        )
        .await
        .unwrap();
        niccolo_evidence::analysis_snapshot::begin(
            &app,
            s.workspace,
            s.principal,
            s.situation,
            new,
            "re-evaluate",
            Some(s.analysis),
        )
        .await
        .unwrap();
    })
    .await;
    for analysis in [s.analysis, new] {
        sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'fixture output','fixture coverage',$4)")
            .bind(s.workspace).bind(s.situation).bind(analysis).bind(s.principal).execute(&admin).await.unwrap();
    }
    let old_claim = Uuid::new_v4();
    let new_claim = Uuid::new_v4();
    for (a, c, t) in [
        (s.analysis, old_claim, "Old inference"),
        (new, new_claim, "New inference"),
    ] {
        sqlx::query("INSERT INTO niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,created_by_principal_id) VALUES($1,$2,$3,$4,1,'INFERENCE',$5,$6)").bind(s.workspace).bind(s.situation).bind(a).bind(c).bind(t).bind(s.principal).execute(&admin).await.unwrap();
    }
    authz::with_auth_digest("test-digest".into(), async {
        let e = niccolo_evidence::reanalysis_revision::explain(
            &app,
            AnalysisScope {
                workspace: s.workspace,
                situation: s.situation,
                analysis: new,
            },
        )
        .await
        .unwrap();
        assert_eq!(e.prior_analysis_id, s.analysis);
        assert!(e.focus_changed);
        assert_eq!(e.prior_input_revision, 1);
        assert_eq!(e.new_input_revision, 2);
        assert_eq!(e.unlinked_prior_claims, vec![old_claim]);
        assert_eq!(e.unlinked_new_claims, vec![new_claim]);
        assert!(!e.unlinked_claims_are_inferred_change);
        assert!(!e.input_change_proves_causal_reason_for_output_change);
    })
    .await;
}

#[tokio::test]
async fn v311_v320_execution_lease_serializes_delivery_surfaces() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        assert!(
            bff_operations::claim_execution(&app, scope, "sync-request-a", 30_000)
                .await
                .unwrap()
        );
        assert!(
            !bff_operations::claim_execution(&app, scope, "async-worker-b", 30_000)
                .await
                .unwrap()
        );
        assert!(
            bff_operations::heartbeat_execution(&app, scope, "sync-request-a", 30_000)
                .await
                .unwrap()
        );
        assert!(
            bff_operations::release_execution(&app, scope, "sync-request-a")
                .await
                .unwrap()
        );
        assert!(
            bff_operations::claim_execution(&app, scope, "async-worker-b", 30_000)
                .await
                .unwrap()
        );
        assert!(
            bff_operations::release_execution(&app, scope, "async-worker-b")
                .await
                .unwrap()
        );
    })
    .await;
    let attempts: i32 = sqlx::query_scalar(
        "SELECT execution_attempts FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(attempts, 2);
}

#[tokio::test]
async fn v301_v310_sync_and_async_share_one_executor_without_double_execution() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let model =
            std::sync::Arc::new(DeterministicModel::new("test", "shared-executor-v1").unwrap());
        let a_pool = app.clone();
        let b_pool = app.clone();
        let a_model = model.clone();
        let b_model = model.clone();
        let a = tokio::spawn(async move {
            authz::with_auth_digest("test-digest".into(), async move {
                bff_executor::execute(&a_pool, scope, s.principal, a_model.as_ref()).await
            })
            .await
        });
        let b = tokio::spawn(async move {
            authz::with_auth_digest("test-digest".into(), async move {
                bff_executor::execute(&b_pool, scope, s.principal, b_model.as_ref()).await
            })
            .await
        });
        let (a, b) = tokio::join!(a, b);
        assert!(a.unwrap().is_ok());
        assert!(b.unwrap().is_ok());
        let final_view = bff_operations::get(&app, scope).await.unwrap();
        assert_ne!(final_view.state, "QUEUED");
    })
    .await;
    let attempts: i32 = sqlx::query_scalar(
        "SELECT execution_attempts FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace).bind(s.situation).bind(scope.operation)
    .fetch_one(&admin).await.unwrap();
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn v321_v330_two_workers_claim_distinct_operations_in_one_workspace() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let first = op(&app, s).await;
    let second = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let (a, b) = tokio::join!(
            bff_operations::claim_next_in_workspace(&app, s.workspace, "worker-a", 30_000),
            bff_operations::claim_next_in_workspace(&app, s.workspace, "worker-b", 30_000),
        );
        let a = a.unwrap().expect("worker a claim");
        let b = b.unwrap().expect("worker b claim");
        assert_ne!(a.operation, b.operation);
        assert!([first.operation, second.operation].contains(&a.operation));
        assert!([first.operation, second.operation].contains(&b.operation));
        assert!(
            bff_operations::release_execution(&app, a, "worker-a")
                .await
                .unwrap()
        );
        assert!(
            bff_operations::release_execution(&app, b, "worker-b")
                .await
                .unwrap()
        );
    })
    .await;
}

#[tokio::test]
async fn v331_v340_expired_worker_lease_is_reclaimable_without_new_domain_task() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        assert!(
            bff_operations::claim_execution(&app, scope, "dead-worker", 30_000)
                .await
                .unwrap()
        );
    })
    .await;
    sqlx::query("UPDATE niccolo.bff_operations SET execution_lease_until=clock_timestamp()-interval '1 second' WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3")
        .bind(s.workspace).bind(s.situation).bind(scope.operation).execute(&admin).await.unwrap();
    authz::with_auth_digest("test-digest".into(), async {
        let reclaimed = bff_operations::claim_next_in_workspace(
            &app,
            s.workspace,
            "replacement-worker",
            30_000,
        )
        .await
        .unwrap()
        .expect("reclaimed operation");
        assert_eq!(reclaimed, scope);
        assert!(
            bff_operations::execution_owned_by(&app, scope, "replacement-worker")
                .await
                .unwrap()
        );
        assert!(
            bff_operations::release_execution(&app, scope, "replacement-worker")
                .await
                .unwrap()
        );
    })
    .await;
    let attempts:i32=sqlx::query_scalar("SELECT execution_attempts FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3")
        .bind(s.workspace).bind(s.situation).bind(scope.operation).fetch_one(&admin).await.unwrap();
    assert_eq!(attempts, 2);
}

#[tokio::test]
async fn v341_v360_pull_observation_wakes_on_event_version_or_timeout() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s).await;
    authz::with_auth_digest("test-digest".into(), async {
        let current = bff_operations::get(&app, scope).await.unwrap();
        let first = bff_operations::observe(&app, scope, current.state_version, 0, 1000, 50)
            .await
            .unwrap();
        assert!(!first.operation_changed);
        assert!(first.events_changed);
        assert!(!first.timed_out);
        assert!(first.next_after_event > 0);

        let started = std::time::Instant::now();
        let timeout = bff_operations::observe(
            &app,
            scope,
            current.state_version,
            first.next_after_event,
            40,
            50,
        )
        .await
        .unwrap();
        assert!(timeout.timed_out);
        assert!(!timeout.operation_changed);
        assert!(!timeout.events_changed);
        assert!(started.elapsed() >= std::time::Duration::from_millis(35));

        let a = app.clone();
        let actor = s.principal;
        let changer = tokio::spawn(async move {
            authz::with_auth_digest("test-digest".into(), async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                bff_operations::start(&a, scope, current.state_version, actor)
                    .await
                    .unwrap();
            })
            .await;
        });
        let changed = bff_operations::observe(
            &app,
            scope,
            current.state_version,
            first.next_after_event,
            1000,
            50,
        )
        .await
        .unwrap();
        changer.await.unwrap();
        assert!(changed.operation_changed || changed.events_changed);
        assert_eq!(changed.operation.state, "RUNNING");
    })
    .await;
}
