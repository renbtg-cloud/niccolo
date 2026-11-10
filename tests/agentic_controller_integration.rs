#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    agentic_controller::{self, StepInput},
    agentic_runtime,
    bff_operations::{self, NewOperation, OperationScope},
    model_adapter::{self, InvocationPolicy, InvocationScope, ModelError, ModelFuture, ModelRequest, ModelResponse, ReasoningModel},
    model_adapter::DeterministicModel,
    operation_budget::{self, BudgetError},
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    let a = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let p = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    (a, p)
}
async fn seed(a: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        analysis: Uuid::new_v4(),
        principal: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(s.principal)
        .execute(a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'agentic')")
        .bind(s.workspace)
        .execute(a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus',$3,1)").bind(s.workspace).bind(s.situation).bind(s.principal).execute(a).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'focus',$3,$4,'CREATED')").bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind(s.principal).execute(a).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind("a".repeat(64)).bind(s.principal).execute(a).await.unwrap();
    s
}
async fn op(app: &PgPool, s: Seed, budget: i32) -> OperationScope {
    let sc = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: Uuid::new_v4(),
    };
    bff_operations::submit(
        app,
        sc,
        &NewOperation {
            kind: "AGENTIC_ANALYSIS".into(),
            target_analysis_id: Some(s.analysis),
            parent_operation_id: None,
            idempotency_key: None,
            input: json!({}),
            deadline_at: None,
            step_budget: budget,
            requested_by: s.principal,
        },
    )
    .await
    .unwrap();
    sc
}
fn step(id: Uuid, n: i32, kind: &str, human: bool, deps: Vec<Uuid>) -> StepInput {
    StepInput {
        step_id: id,
        ordinal: n,
        kind: kind.into(),
        label: format!("step {n}"),
        requires_human: human,
        depends_on: deps,
    }
}
struct OverBudgetModel {
    calls: AtomicUsize,
}

impl ReasoningModel for OverBudgetModel {
    fn provider(&self) -> &str { "over-budget-fixture" }
    fn model(&self) -> &str { "static-response" }
    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ModelResponse {
                value: json!({"status":"ok"}),
                prompt_tokens: Some(90),
                completion_tokens: Some(20),
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

struct HumanSteeringAssertingModel {
    expected_text: &'static str,
}

impl ReasoningModel for HumanSteeringAssertingModel {
    fn provider(&self) -> &str {
        "human-steering-fixture"
    }
    fn model(&self) -> &str {
        "static-response"
    }
    fn invoke<'a>(&'a self, request: &'a ModelRequest) -> ModelFuture<'a> {
        let steering = request
            .input
            .get("human_steering")
            .expect("model request carries human steering");
        assert_eq!(steering["scope"], "THIS_OPERATION_ONLY");
        assert_eq!(steering["responses_are_evidence"], false);
        assert_eq!(steering["responses_are_facts"], false);
        assert_eq!(steering["frozen_input_unchanged"], true);
        assert_eq!(steering["truncated"], false);
        assert_eq!(
            request.output_contract["niccolo_rules"]["human_steering_is_not_evidence"],
            true
        );
        assert_eq!(
            request.output_contract["niccolo_rules"]
                ["human_steering_does_not_rewrite_frozen_input"],
            true
        );
        let responses = steering["responses"]
            .as_array()
            .expect("human steering responses are an array");
        assert!(responses.iter().any(|response| {
            response["decision"] == "REPLAN"
                && response["response_text"] == self.expected_text
        }));
        let purpose = request.purpose.clone();
        Box::pin(async move {
            Ok(ModelResponse {
                value: json!({"purpose":purpose,"status":"ok"}),
                prompt_tokens: Some(8),
                completion_tokens: Some(4),
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

struct NoUsageModel;
impl ReasoningModel for NoUsageModel {
    fn provider(&self) -> &str { "usage-omitting-fixture" }
    fn model(&self) -> &str { "static-response" }
    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        Box::pin(async {
            Ok(ModelResponse {
                value: json!({"status":"ok"}),
                prompt_tokens: None,
                completion_tokens: None,
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

#[tokio::test]
async fn successful_model_call_without_usage_is_flagged_as_unaccounted() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let (revision, _) = agentic_controller::create_plan(
        &app, scope, s.principal,
        &[step(step_id, 1, "SYNTHESIZE", false, vec![])],
    )
    .await
    .unwrap();
    model_adapter::invoke_recorded(
        &app,
        InvocationScope { operation: scope, plan_revision: revision, step_id },
        s.principal,
        &NoUsageModel,
        &ModelRequest {
            purpose: "SYNTHESIZE".into(),
            input: json!({}),
            output_contract: json!({}),
        },
    )
    .await
    .unwrap();
    let budget = operation_budget::view(&app, scope).await.unwrap();
    assert_eq!(budget.used_invocations, 1);
    assert_eq!(budget.unreported_token_usage_invocations, 1);
    assert!(!budget.token_usage_complete);
    assert!(budget.remaining_total_tokens_is_upper_bound);
    assert_eq!(budget.used_total_tokens, 0);
    assert!(budget.can_start_model_call);
}

#[tokio::test]
async fn model_token_budget_breach_is_not_retried_or_misreported() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let (revision, _) = agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(step_id, 1, "SYNTHESIZE", false, vec![])],
    )
    .await
    .unwrap();
    let model = OverBudgetModel { calls: AtomicUsize::new(0) };
    let result = model_adapter::invoke_recorded_with_policy(
        &app,
        InvocationScope { operation: scope, plan_revision: revision, step_id },
        s.principal,
        &model,
        &ModelRequest {
            purpose: "SYNTHESIZE".into(),
            input: json!({}),
            output_contract: json!({}),
        },
        InvocationPolicy { timeout_ms: 5_000, max_attempts: 4, max_total_tokens: Some(100) },
    )
    .await;
    assert!(matches!(result, Err(ModelError::Budget)));
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    let row: (i32, bool, String, String) = sqlx::query_as(
        "SELECT attempt_count,budget_exceeded,status,error_code FROM niccolo.model_invocations          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5"
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(revision)
    .bind(step_id)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(row, (1, true, "FAILED".into(), "BUDGET_EXCEEDED".into()));
}

#[tokio::test]
async fn invalid_operation_budget_cannot_strand_an_agentic_step_in_started() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: Uuid::new_v4(),
    };
    let mut request = NewOperation {
        kind: "AGENTIC_ANALYSIS".into(),
        target_analysis_id: Some(s.analysis),
        parent_operation_id: None,
        idempotency_key: None,
        input: json!({"model_budget":{"max_invocations":0}}),
        deadline_at: None,
        step_budget: 32,
        requested_by: s.principal,
    };
    bff_operations::submit(&app, scope, &request).await.unwrap();
    let step_id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let model = DeterministicModel::new("local-fixture", "none").unwrap();
    let result = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model).await;
    assert!(matches!(
        result,
        Err(agentic_runtime::RuntimeError::Budget(BudgetError::Invalid))
    ));
    let last_event: String = sqlx::query_scalar(
        "SELECT event_kind FROM niccolo.agentic_step_events \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4 \
         ORDER BY event_seq DESC LIMIT 1"
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(step_id)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(last_event, "FAILED");
    request.input = json!({"model_budget":{"max_invocations":1}});
    assert!(matches!(
        bff_operations::submit(&app, scope, &request).await,
        Err(bff_operations::BffOperationError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn v211_v214_plan_is_versioned_hashed_and_rejects_cycles() {
    let (a, p) = pools().await;
    let s = seed(&a).await;
    let sc = op(&p, s, 10).await;
    let x = Uuid::new_v4();
    let y = Uuid::new_v4();
    assert!(
        agentic_controller::create_plan(
            &p,
            sc,
            s.principal,
            &[
                step(x, 1, "INSPECT_SOURCES", false, vec![y]),
                step(y, 2, "SYNTHESIZE", false, vec![x])
            ]
        )
        .await
        .is_err()
    );
    let (rev, sha) = agentic_controller::create_plan(
        &p,
        sc,
        s.principal,
        &[
            step(x, 1, "INSPECT_SOURCES", false, vec![]),
            step(y, 2, "SYNTHESIZE", false, vec![x]),
        ],
    )
    .await
    .unwrap();
    assert_eq!(rev, 1);
    assert_eq!(sha.len(), 64);
}
#[tokio::test]
async fn v215_v218_controller_only_starts_dependency_ready_steps() {
    let (a, p) = pools().await;
    let s = seed(&a).await;
    let sc = op(&p, s, 10).await;
    let x = Uuid::new_v4();
    let y = Uuid::new_v4();
    agentic_controller::create_plan(
        &p,
        sc,
        s.principal,
        &[
            step(x, 1, "INSPECT_SOURCES", false, vec![]),
            step(y, 2, "SYNTHESIZE", false, vec![x]),
        ],
    )
    .await
    .unwrap();
    let v = agentic_controller::view(&p, sc).await.unwrap();
    assert_eq!(v.ready_step_ids, vec![x]);
    assert!(
        agentic_controller::start_step(&p, sc, y, s.principal)
            .await
            .is_err()
    );
    agentic_controller::start_step(&p, sc, x, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &p,
        sc,
        x,
        s.principal,
        "SUCCEEDED",
        "source inspection complete",
    )
    .await
    .unwrap();
    let v = agentic_controller::view(&p, sc).await.unwrap();
    assert_eq!(v.ready_step_ids, vec![y]);
}
#[tokio::test]
async fn v219_v222_human_gate_pauses_and_resumes_same_operation() {
    let (a, p) = pools().await;
    let s = seed(&a).await;
    let sc = op(&p, s, 10).await;
    let h = Uuid::new_v4();
    agentic_controller::create_plan(
        &p,
        sc,
        s.principal,
        &[step(h, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&p, sc, h, s.principal)
        .await
        .unwrap();
    let w = agentic_controller::finish_step(
        &p,
        sc,
        h,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();
    assert!(w.waiting_for_human);
    assert_eq!(
        bff_operations::get(&p, sc).await.unwrap().state,
        "WAITING_FOR_HUMAN"
    );
    let r = agentic_controller::resume_step(&p, sc, h, s.principal, "human answered")
        .await
        .unwrap();
    assert_eq!(r.steps[0].state, "STARTED");
    assert_eq!(bff_operations::get(&p, sc).await.unwrap().state, "RUNNING");
}
#[tokio::test]
async fn concurrent_exact_human_response_is_serialized_without_duplicate_events() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let human = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, human, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &app,
        scope,
        human,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();

    let response_id = Uuid::new_v4();
    let first = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        response_id,
        s.principal,
        "CONTINUE",
        "Continue with the current evidence boundaries.",
    );
    let second = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        response_id,
        s.principal,
        "CONTINUE",
        "Continue with the current evidence boundaries.",
    );
    let (first, second) = tokio::join!(first, second);
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.operation_state, "RUNNING");
    assert_eq!(second.operation_state, "RUNNING");
    let resume_flags = [first.executor_resume_required, second.executor_resume_required];
    assert_eq!(resume_flags.iter().filter(|&&flag| flag).count(), 1);

    let response_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_human_responses WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(human)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(response_count, 1);

    let (resumed, succeeded): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE event_kind='RESUMED'), count(*) FILTER (WHERE event_kind='SUCCEEDED') FROM niccolo.agentic_step_events WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(human)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!((resumed, succeeded), (1, 1));
}

#[tokio::test]
async fn exhausted_step_budget_rejects_new_replan_without_consuming_human_choice() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 1).await;
    let human = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, human, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &app,
        scope,
        human,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();

    let rejected = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        Uuid::new_v4(),
        s.principal,
        "REPLAN",
        "Try a different reasoning path.",
    )
    .await;
    assert!(matches!(
        rejected,
        Err(agentic_controller::ControllerError::BudgetExhausted)
    ));
    let still_waiting = agentic_controller::view(&app, scope).await.unwrap();
    assert!(still_waiting.waiting_for_human);
    assert_eq!(still_waiting.budget_used, 1);
    let response_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_human_responses WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(human)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(response_count, 0);

    let stopped = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        Uuid::new_v4(),
        s.principal,
        "STOP",
        "",
    )
    .await
    .unwrap();
    assert_eq!(stopped.operation_state, "CANCELLED");
}

#[tokio::test]
async fn step_budget_remains_operation_wide_across_replans() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 2).await;
    let human = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, human, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &app,
        scope,
        human,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();
    assert_eq!(agentic_controller::view(&app, scope).await.unwrap().budget_used, 1);

    agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        Uuid::new_v4(),
        s.principal,
        "REPLAN",
        "Use the same evidence but reconsider the interpretation.",
    )
    .await
    .unwrap();
    let replanned = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(replanned.budget_used, 1);
    assert_eq!(replanned.budget_total, 2);
    let first = *replanned.ready_step_ids.first().expect("replanned work is ready");
    agentic_controller::start_step(&app, scope, first, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(&app, scope, first, s.principal, "SUCCEEDED", "done")
        .await
        .unwrap();
    let after = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(after.budget_used, 2);
    let next = *after.ready_step_ids.first().expect("another replanned step is ready");
    assert!(matches!(
        agentic_controller::start_step(&app, scope, next, s.principal).await,
        Err(agentic_controller::ControllerError::BudgetExhausted)
    ));
}

#[tokio::test]
async fn replan_preserves_the_original_human_intent_and_its_required_boundary() {
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
            idempotency_key: None,
            input: json!({"human_intent":"ABOUT_TO_ACT_OR_SAY"}),
            deadline_at: None,
            step_budget: 32,
            requested_by: s.principal,
        },
    )
    .await
    .unwrap();

    let human = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, human, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &app,
        scope,
        human,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();

    let resolution = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        Uuid::new_v4(),
        s.principal,
        "REPLAN",
        "Reconsider the evidence before I decide what to say.",
    )
    .await
    .unwrap();
    assert!(resolution.executor_resume_required);

    let replanned = agentic_controller::view(&app, scope).await.unwrap();
    let human_step = replanned
        .steps
        .iter()
        .find(|candidate| candidate.kind == "REQUEST_HUMAN_DECISION")
        .expect("ABOUT_TO_ACT_OR_SAY keeps an explicit human boundary after replan");
    assert!(human_step.requires_human);
    assert!(human_step.label.contains("before action"));
}

#[tokio::test]
async fn bounded_human_steering_keeps_the_newest_responses_in_chronological_order() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 64).await;

    for index in 1..=33 {
        let human = Uuid::new_v4();
        let (revision, _) = agentic_controller::create_plan(
            &app,
            scope,
            s.principal,
            &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO niccolo.agentic_human_responses(
                workspace_id,situation_id,operation_id,plan_revision,step_id,
                response_id,decision,response_text,actor_principal_id
             ) VALUES($1,$2,$3,$4,$5,$6,'REPLAN',$7,$8)",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .bind(revision)
        .bind(human)
        .bind(Uuid::new_v4())
        .bind(format!("response-{index:02}"))
        .bind(s.principal)
        .execute(&admin)
        .await
        .unwrap();
    }

    let steering = agentic_controller::human_steering(&app, scope, 32)
        .await
        .unwrap();
    assert!(steering.truncated);
    assert_eq!(steering.responses.len(), 32);
    assert_eq!(steering.responses.first().unwrap().response_text, "response-02");
    assert_eq!(steering.responses.last().unwrap().response_text, "response-33");
    assert!(steering
        .responses
        .windows(2)
        .all(|pair| pair[0].plan_revision < pair[1].plan_revision));
}

#[tokio::test]
async fn exact_stop_replay_is_idempotent_after_operation_is_cancelled() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let human = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, human, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &app,
        scope,
        human,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();

    let response_id = Uuid::new_v4();
    let first = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        response_id,
        s.principal,
        "STOP",
        "",
    )
    .await
    .unwrap();
    assert_eq!(first.operation_state, "CANCELLED");
    assert!(first.human_response_persisted);
    assert!(!first.system_inferred_consent);
    assert!(!first.executor_resume_required);

    let events_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_step_events WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(human)
    .fetch_one(&admin)
    .await
    .unwrap();

    let replay = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        response_id,
        s.principal,
        "STOP",
        "",
    )
    .await
    .unwrap();
    assert_eq!(replay.operation_state, "CANCELLED");
    assert!(!replay.executor_resume_required);

    let events_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_step_events WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(human)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(events_before, events_after);

    let conflicting = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        Uuid::new_v4(),
        s.principal,
        "STOP",
        "",
    )
    .await;
    assert!(matches!(
        conflicting,
        Err(agentic_controller::ControllerError::StateConflict)
    ));
}

#[tokio::test]
async fn exact_replan_replay_does_not_create_another_plan_revision() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 32).await;
    let human = Uuid::new_v4();
    let (original_revision, _) = agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(human, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, human, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(
        &app,
        scope,
        human,
        s.principal,
        "WAITING_FOR_HUMAN",
        "need explicit human choice",
    )
    .await
    .unwrap();

    let response_id = Uuid::new_v4();
    let first = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        response_id,
        s.principal,
        "REPLAN",
        "Replan using the evidence already available.",
    )
    .await
    .unwrap();
    assert_eq!(first.operation_state, "RUNNING");
    assert!(first.plan_revision > original_revision);
    assert!(first.executor_resume_required);
    let replanned_revision = first.plan_revision;

    let replay = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        response_id,
        s.principal,
        "REPLAN",
        "Replan using the evidence already available.",
    )
    .await
    .unwrap();
    assert_eq!(replay.operation_state, "RUNNING");
    assert!(!replay.executor_resume_required);
    assert_eq!(replay.plan_revision, replanned_revision);
    assert_eq!(
        agentic_controller::view(&app, scope)
            .await
            .unwrap()
            .plan_revision,
        replanned_revision
    );

    let response_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_human_responses WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(human)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(response_count, 1);

    let steering = agentic_controller::human_steering(&app, scope, 32)
        .await
        .unwrap();
    assert_eq!(steering.scope, "THIS_OPERATION_ONLY");
    assert!(!steering.responses_are_evidence);
    assert!(!steering.responses_are_facts);
    assert!(steering.frozen_input_unchanged);
    assert!(!steering.truncated);
    assert_eq!(steering.responses.len(), 1);
    assert_eq!(steering.responses[0].decision, "REPLAN");
    assert_eq!(
        steering.responses[0].response_text,
        "Replan using the evidence already available."
    );

    let inspect = Uuid::new_v4();
    let (steering_revision, _) = agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(inspect, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    assert!(steering_revision > replanned_revision);
    let outcome = agentic_runtime::run_until_boundary(
        &app,
        scope,
        s.principal,
        &HumanSteeringAssertingModel {
            expected_text: "Replan using the evidence already available.",
        },
    )
    .await
    .unwrap();
    assert_eq!(outcome.state, "SUCCEEDED");

    let conflicting = agentic_controller::resolve_human_boundary(
        &app,
        scope,
        human,
        Uuid::new_v4(),
        s.principal,
        "REPLAN",
        "Replan using the evidence already available.",
    )
    .await;
    assert!(matches!(
        conflicting,
        Err(agentic_controller::ControllerError::StateConflict)
    ));
}

#[tokio::test]
async fn v223_v225_step_budget_is_hard_and_inspectable() {
    let (a, p) = pools().await;
    let s = seed(&a).await;
    let sc = op(&p, s, 1).await;
    let x = Uuid::new_v4();
    let y = Uuid::new_v4();
    agentic_controller::create_plan(
        &p,
        sc,
        s.principal,
        &[
            step(x, 1, "INSPECT_SOURCES", false, vec![]),
            step(y, 2, "ASSESS_CONTEXT", false, vec![x]),
        ],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&p, sc, x, s.principal)
        .await
        .unwrap();
    agentic_controller::finish_step(&p, sc, x, s.principal, "SUCCEEDED", "done")
        .await
        .unwrap();
    let v = agentic_controller::view(&p, sc).await.unwrap();
    assert_eq!(v.budget_used, 1);
    assert_eq!(v.budget_total, 1);
    assert!(matches!(
        agentic_controller::start_step(&p, sc, y, s.principal).await,
        Err(agentic_controller::ControllerError::BudgetExhausted)
    ));
}
