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
    op_with_deadline(app, s, budget, None).await
}

async fn op_with_deadline(
    app: &PgPool,
    s: Seed,
    budget: i32,
    deadline_at: Option<String>,
) -> OperationScope {
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
            deadline_at,
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
async fn last_step_event(
    admin: &PgPool,
    scope: OperationScope,
    step_id: Uuid,
) -> (String, String) {
    sqlx::query_as(
        "SELECT event_kind,note FROM niccolo.agentic_step_events
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4
         ORDER BY event_seq DESC LIMIT 1",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(step_id)
    .fetch_one(admin)
    .await
    .unwrap()
}

struct ReportedUsageModel {
    prompt_tokens: i64,
    completion_tokens: i64,
    effective_model: Option<&'static str>,
    calls: AtomicUsize,
}

impl ReasoningModel for ReportedUsageModel {
    fn provider(&self) -> &str { "over-budget-fixture" }
    fn model(&self) -> &str { "static-response" }
    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(ModelResponse {
                value: json!({"status":"ok"}),
                prompt_tokens: Some(self.prompt_tokens),
                completion_tokens: Some(self.completion_tokens),
                effective_provider: None,
                effective_model: self.effective_model.map(str::to_owned),
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
async fn invalid_provider_attribution_fails_the_invocation_without_stranding_it() {
    for model in ["fixture\0model", " \t"] {
        assert_invalid_provider_attribution(model).await;
    }
}

async fn assert_invalid_provider_attribution(effective_model: &'static str) {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 32).await;
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: 90,
        completion_tokens: 20,
        effective_model: Some(effective_model),
    };
    assert!(
        niccolo_evidence::bff_executor::execute(&app, scope, s.principal, &model)
            .await
            .is_err()
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(bff_operations::get(&app, scope).await.unwrap().state, "FAILED");
    let (status, effective_model): (String, Option<String>) = sqlx::query_as(
        "SELECT status,effective_model FROM niccolo.model_invocations
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(status, "FAILED");
    assert_eq!(effective_model, None);
    let budget = operation_budget::view(&app, scope).await.unwrap();
    assert_eq!(budget.used_invocations, 1);
    assert_eq!(budget.used_total_tokens, 110);
    assert!(budget.token_usage_complete);
}

#[tokio::test]
async fn model_token_totals_do_not_overflow_budget_accounting() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let (plan_revision, _) = agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let invocation = InvocationScope {
        operation: scope,
        plan_revision,
        step_id,
    };
    let request = ModelRequest {
        purpose: "INSPECT_SOURCES".into(),
        input: json!({}),
        output_contract: json!({}),
    };
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: i64::MAX,
        completion_tokens: 1,
        effective_model: None,
    };
    assert!(matches!(
        model_adapter::invoke_recorded_with_policy(
            &app,
            invocation,
            s.principal,
            &model,
            &request,
            InvocationPolicy {
                max_total_tokens: Some(i64::MAX),
                ..InvocationPolicy::default()
            },
        )
        .await,
        Err(ModelError::Budget)
    ));
    model_adapter::invoke_recorded(&app, invocation, s.principal, &model, &request)
        .await
        .unwrap();
    let budget = operation_budget::view(&app, scope).await.unwrap();
    assert_eq!(budget.used_total_tokens, i64::MAX);
    assert_eq!(budget.remaining_total_tokens, 0);
    assert_eq!(budget.stop_reasons, ["TOKEN_LIMIT"]);
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
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: 90,
        completion_tokens: 20,
        effective_model: None,
    };
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
async fn runtime_rejects_a_response_over_the_remaining_operation_budget() {
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
            input: json!({"model_budget":{"max_total_tokens":200}}),
            deadline_at: None,
            step_budget: 10,
            requested_by: s.principal,
        },
    )
    .await
    .unwrap();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[
            step(first, 1, "INSPECT_SOURCES", false, vec![]),
            step(second, 2, "INSPECT_SOURCES", false, vec![first]),
        ],
    )
    .await
    .unwrap();
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: 90,
        completion_tokens: 20,
        effective_model: None,
    };
    assert!(matches!(
        agentic_runtime::run_until_boundary(&app, scope, s.principal, &model).await,
        Err(agentic_runtime::RuntimeError::Model(ModelError::Budget))
    ));
    assert_eq!(model.calls.load(Ordering::SeqCst), 2);
    let rows: Vec<(Uuid, String, Option<i64>)> = sqlx::query_as(
        "SELECT step_id,status,max_total_tokens FROM niccolo.model_invocations
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .fetch_all(&admin)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&(first, "SUCCEEDED".into(), Some(200))));
    assert!(rows.contains(&(second, "FAILED".into(), Some(90))));
    let budget = operation_budget::view(&app, scope).await.unwrap();
    assert_eq!(budget.used_invocations, 2);
    assert_eq!(budget.used_total_tokens, 220);
    assert_eq!(budget.remaining_total_tokens, 0);
    assert_eq!(budget.stop_reasons, ["TOKEN_LIMIT"]);
    assert_eq!(
        last_step_event(&admin, scope, second).await,
        ("FAILED".into(), "model reported usage above the remaining operation token allowance".into())
    );
}

struct ExcessiveDiagnosticModel;

impl ReasoningModel for ExcessiveDiagnosticModel {
    fn provider(&self) -> &str {
        "diagnostic-fixture"
    }

    fn model(&self) -> &str {
        "failure-response"
    }

    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        Box::pin(async { Err(ModelError::Provider("diagnostic ".repeat(500))) })
    }
}

#[tokio::test]
async fn oversized_provider_diagnostic_does_not_strand_the_worker_operation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 32).await;
    assert!(
        niccolo_evidence::bff_executor::execute(&app, scope, s.principal, &ExcessiveDiagnosticModel)
            .await
            .is_err()
    );
    let view = bff_operations::get(&app, scope).await.unwrap();
    assert_eq!(view.state, "FAILED");
    assert_eq!(view.error_code.as_deref(), Some("AGENTIC_EXECUTION_FAILED"));
    assert_eq!(
        view.error_message.as_deref(),
        Some("failure details could not be recorded")
    );
    let (status, message): (String, String) = sqlx::query_as(
        "SELECT status,error_message FROM niccolo.model_invocations
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(status, "FAILED");
    assert_eq!(message, "failure details could not be recorded");
}

#[derive(Default)]
struct GatedResponseModel {
    fail: bool,
    calls: AtomicUsize,
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl ReasoningModel for GatedResponseModel {
    fn provider(&self) -> &str {
        "gated-fixture"
    }

    fn model(&self) -> &str {
        "delayed-response"
    }

    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        Box::pin(async move {
            self.release.notified().await;
            if self.fail {
                return Err(ModelError::Provider("transient fixture error".into()));
            }
            Ok(ModelResponse {
                value: json!({"status":"ok"}),
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

#[tokio::test]
async fn late_provider_response_does_not_publish_after_invocation_recovery() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let (revision, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let model = GatedResponseModel::default();
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            agentic_runtime::run_until_boundary(&app, scope, s.principal, &model),
            async {
                model.started.notified().await;
                sqlx::query(
                    "UPDATE niccolo.agentic_step_events
                     SET created_at=clock_timestamp()-interval '10 minutes'
                     WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
                       AND plan_revision=$4 AND step_id=$5 AND event_kind='STARTED'",
                )
                .bind(s.workspace)
                .bind(s.situation)
                .bind(scope.operation)
                .bind(revision)
                .bind(step_id)
                .execute(&admin)
                .await
                .unwrap();
                let view =
                    agentic_controller::recover_stale_started(&app, scope, s.principal, 120)
                        .await
                        .unwrap();
                assert_eq!(view.ready_step_ids, [step_id]);
                model.release.notify_one();
            },
        )
    })
    .await
    .unwrap();
    assert!(matches!(
        result,
        Err(agentic_runtime::RuntimeError::Model(ModelError::InvocationNoLongerActive))
    ));
    let (status, code, response, attribution): (String, String, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT status,error_code,response_json,effective_model FROM niccolo.model_invocations
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
        )
        .bind(s.workspace)
        .bind(s.situation)
        .bind(scope.operation)
        .fetch_one(&admin)
        .await
        .unwrap();
    assert_eq!((status.as_str(), code.as_str()), ("FAILED", "RECOVERY_STALE"));
    assert_eq!((response, attribution), (None, None));
    let view = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(view.ready_step_ids, [step_id]);
}

#[tokio::test]
async fn recovered_invocation_does_not_retry_after_provider_failure() {
    assert_retry_stops(RetryInterruption::Recovery).await;
}

#[tokio::test]
async fn cancelled_operation_does_not_retry_after_provider_failure() {
    assert_retry_stops(RetryInterruption::Cancellation).await;
}

#[tokio::test]
async fn superseded_plan_does_not_retry_after_provider_failure() {
    assert_retry_stops(RetryInterruption::Replan).await;
}

#[derive(Clone, Copy)]
enum RetryInterruption {
    Recovery,
    Cancellation,
    Replan,
}

async fn assert_retry_stops(interruption: RetryInterruption) {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let (plan_revision, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    if !matches!(interruption, RetryInterruption::Replan) {
        agentic_controller::start_step(&app, scope, step_id, s.principal).await.unwrap();
    }
    let model = GatedResponseModel { fail: true, ..Default::default() };
    let request = ModelRequest {
        purpose: "INSPECT_SOURCES".into(),
        input: json!({}),
        output_contract: json!({}),
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            model_adapter::invoke_recorded_with_policy(
                &app,
                InvocationScope { operation: scope, plan_revision, step_id },
                s.principal,
                &model,
                &request,
                InvocationPolicy { max_attempts: 2, ..Default::default() },
            ),
            async {
                model.started.notified().await;
                match interruption {
                    RetryInterruption::Recovery => {
                        sqlx::query(
                            "UPDATE niccolo.agentic_step_events
                             SET created_at=clock_timestamp()-interval '10 minutes'
                             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
                               AND plan_revision=$4 AND step_id=$5 AND event_kind='STARTED'",
                        )
                        .bind(s.workspace).bind(s.situation).bind(scope.operation)
                        .bind(plan_revision).bind(step_id).execute(&admin).await.unwrap();
                        agentic_controller::recover_stale_started(&app, scope, s.principal, 120)
                            .await.unwrap();
                    }
                    RetryInterruption::Cancellation => {
                        let op = bff_operations::get(&app, scope).await.unwrap();
                        bff_operations::cancel(&app, scope, op.state_version, s.principal, "stop")
                            .await
                            .unwrap();
                    }
                    RetryInterruption::Replan => {
                        agentic_controller::create_plan(
                            &app,
                            scope,
                            s.principal,
                            &[step(step_id, 1, "CHECK_ASSUMPTIONS", false, vec![])],
                        )
                        .await
                        .unwrap();
                    }
                }
                model.release.notify_one();
            },
        )
    })
    .await
    .unwrap();
    if matches!(interruption, RetryInterruption::Recovery) {
        assert!(matches!(result, Err(ModelError::InvocationNoLongerActive)));
    } else {
        assert!(matches!(result, Err(ModelError::Provider(_))));
        let status: String = sqlx::query_scalar(
            "SELECT status FROM niccolo.model_invocations
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
        )
        .bind(s.workspace).bind(s.situation).bind(scope.operation)
        .fetch_one(&admin).await.unwrap();
        assert_eq!(status, "FAILED");
    }
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn superseded_plan_cannot_reserve_a_model_invocation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let steps = [step(step_id, 1, "INSPECT_SOURCES", false, vec![])];
    let (old, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
        .await.unwrap();
    let (current, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
        .await.unwrap();
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: 2,
        completion_tokens: 1,
        effective_model: None,
    };
    let request = ModelRequest {
        purpose: "INSPECT_SOURCES".into(),
        input: json!({}),
        output_contract: json!({}),
    };
    let stale = model_adapter::invoke_recorded(
        &app, InvocationScope { operation: scope, plan_revision: old, step_id },
        s.principal, &model, &request,
    ).await;
    assert!(matches!(stale, Err(ModelError::InvocationNoLongerActive)));
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
    let current = model_adapter::invoke_recorded(
        &app, InvocationScope { operation: scope, plan_revision: current, step_id },
        s.principal, &model, &request,
    ).await.unwrap();
    assert_eq!(current.status, "SUCCEEDED");
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(operation_budget::view(&app, scope).await.unwrap().used_invocations, 1);
}

#[tokio::test]
async fn expired_operation_cannot_reserve_a_model_invocation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op_with_deadline(&app, s, 10, Some("-infinity".into())).await;
    let step_id = Uuid::new_v4();
    let (plan_revision, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: 2,
        completion_tokens: 1,
        effective_model: None,
    };
    let result = model_adapter::invoke_recorded(
        &app,
        InvocationScope { operation: scope, plan_revision, step_id },
        s.principal,
        &model,
        &ModelRequest {
            purpose: "INSPECT_SOURCES".into(),
            input: json!({}),
            output_contract: json!({}),
        },
    )
    .await;
    assert!(matches!(result, Err(ModelError::Deadline)));
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
    assert_eq!(operation_budget::view(&app, scope).await.unwrap().used_invocations, 0);
}

#[tokio::test]
async fn closed_operation_cannot_reserve_a_new_model_invocation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    let (plan_revision, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    ).await.unwrap();
    let op = bff_operations::get(&app, scope).await.unwrap();
    bff_operations::cancel(&app, scope, op.state_version, s.principal, "stop")
        .await
        .unwrap();
    let model = ReportedUsageModel {
        calls: AtomicUsize::new(0),
        prompt_tokens: 2,
        completion_tokens: 1,
        effective_model: None,
    };
    let result = model_adapter::invoke_recorded(
        &app,
        InvocationScope { operation: scope, plan_revision, step_id },
        s.principal,
        &model,
        &ModelRequest {
            purpose: "INSPECT_SOURCES".into(),
            input: json!({}),
            output_contract: json!({}),
        },
    ).await;
    assert!(matches!(result, Err(ModelError::InvocationNoLongerActive)));
    assert_eq!(model.calls.load(Ordering::SeqCst), 0);
    assert_eq!(operation_budget::view(&app, scope).await.unwrap().used_invocations, 0);
}

#[tokio::test]
async fn runtime_skips_postprocessing_when_model_returns_after_cancellation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app, scope, s.principal, &[step(step_id, 1, "SYNTHESIZE", false, vec![])],
    ).await.unwrap();
    let model = GatedResponseModel::default();
    let (outcome, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            agentic_runtime::run_until_boundary(&app, scope, s.principal, &model),
            async {
                model.started.notified().await;
                let op = bff_operations::get(&app, scope).await.unwrap();
                bff_operations::cancel(&app, scope, op.state_version, s.principal, "stop")
                    .await
                    .unwrap();
                model.release.notify_one();
            }
        )
    }).await.unwrap();
    let outcome = outcome.unwrap();
    assert_eq!(outcome.state, "CANCELLED");
    assert!(outcome.terminal);
    assert_eq!(outcome.completed_steps, 0);
    assert_eq!(last_step_event(&admin, scope, step_id).await.0, "STARTED");
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
}

#[derive(Default)]
struct PendingModel {
    started: tokio::sync::Notify,
}

impl ReasoningModel for PendingModel {
    fn provider(&self) -> &str {
        "pending-fixture"
    }

    fn model(&self) -> &str {
        "pending-response"
    }

    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        self.started.notify_one();
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn runtime_bounds_model_wait_by_the_operation_deadline() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: Uuid::new_v4(),
    };
    let deadline: String =
        sqlx::query_scalar("SELECT (clock_timestamp() + interval '4 seconds')::text")
            .fetch_one(&admin)
            .await
            .unwrap();
    bff_operations::submit(
        &app,
        scope,
        &NewOperation {
            kind: "AGENTIC_ANALYSIS".into(),
            target_analysis_id: Some(s.analysis),
            parent_operation_id: None,
            idempotency_key: None,
            input: json!({}),
            deadline_at: Some(deadline),
            step_budget: 10,
            requested_by: s.principal,
        },
    )
    .await
    .unwrap();
    let step_id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let model = PendingModel::default();
    let (result, pending_budget) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        async {
            tokio::join!(
                agentic_runtime::run_until_boundary(&app, scope, s.principal, &model),
                async {
                    model.started.notified().await;
                    operation_budget::view(&app, scope).await.unwrap()
                },
            )
        },
    )
    .await
    .expect("the operation deadline should bound the model wait");
    assert_eq!(pending_budget.used_invocations, 1);
    assert_eq!(pending_budget.unreported_token_usage_invocations, 1);
    assert!(!pending_budget.token_usage_complete);
    assert!(matches!(
        result,
        Err(agentic_runtime::RuntimeError::Model(ModelError::Timeout))
    ));
    let (timeout_ms, status, error_code): (i32, String, String) = sqlx::query_as(
        "SELECT timeout_ms,status,error_code FROM niccolo.model_invocations
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(step_id)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert!((1..=4_000).contains(&timeout_ms));
    assert_eq!(status, "FAILED");
    assert_eq!(error_code, "TIMEOUT");
    let budget = operation_budget::view(&app, scope).await.unwrap();
    assert_eq!(budget.used_invocations, 1);
    assert_eq!(budget.unreported_token_usage_invocations, 1);
    assert!(!budget.token_usage_complete);
    assert!(budget.remaining_total_tokens_is_upper_bound);
    assert_eq!(
        last_step_event(&admin, scope, step_id).await,
        ("FAILED".into(), "model invocation timed out".into())
    );
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
    assert_eq!(
        last_step_event(&admin, scope, step_id).await,
        ("FAILED".into(), "operation model budget is invalid".into())
    );
    request.input = json!({"model_budget":{"max_invocations":1}});
    assert!(matches!(
        bff_operations::submit(&app, scope, &request).await,
        Err(bff_operations::BffOperationError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn dense_plan_roundtrips_all_dependencies_and_ready_steps() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 64).await;
    let ids: Vec<_> = (0..64).map(|_| Uuid::new_v4()).collect();
    let steps: Vec<_> = ids.iter().enumerate().map(|(index, &id)| {
        step(id, (index + 1) as i32, "CHECK_ASSUMPTIONS", false, ids[..index].to_vec())
    }).collect();
    agentic_controller::create_plan(&app, scope, s.principal, &steps).await.unwrap();
    let view = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(view.steps.len(), ids.len());
    assert_eq!(view.ready_step_ids, [ids[0]]);
    for (index, persisted) in view.steps.iter().enumerate() {
        let mut expected = ids[..index].to_vec();
        expected.sort_unstable();
        assert_eq!(persisted.step_id, ids[index]);
        assert_eq!(persisted.depends_on, expected);
    }

    agentic_controller::start_step(&app, scope, ids[0], s.principal).await.unwrap();
    let view = agentic_controller::finish_step(
        &app, scope, ids[0], s.principal, "SUCCEEDED", "first condition checked",
    ).await.unwrap();
    assert_eq!(view.ready_step_ids, [ids[1]]);
    assert_eq!(view.budget_used, 1);
}

#[tokio::test]
async fn equivalent_plan_edge_order_produces_the_same_digest() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let mut prerequisites = [Uuid::new_v4(), Uuid::new_v4()];
    prerequisites.sort_unstable();
    let mut plan = vec![
        step(prerequisites[0], 1, "INSPECT_SOURCES", false, vec![]),
        step(prerequisites[1], 2, "CHECK_ASSUMPTIONS", false, vec![]),
        step(Uuid::new_v4(), 3, "SYNTHESIZE", false, prerequisites.to_vec()),
    ];
    let (_, first) = agentic_controller::create_plan(&app, scope, s.principal, &plan)
        .await
        .unwrap();
    plan[2].depends_on.reverse();
    plan.reverse();
    let (_, second) = agentic_controller::create_plan(&app, scope, s.principal, &plan)
        .await
        .unwrap();
    assert_eq!(first, second);
    let view = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(view.steps[2].depends_on, prerequisites);
}

#[tokio::test]
async fn concurrent_plans_allocate_distinct_consecutive_revisions() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let first_step = Uuid::new_v4();
    let second_step = Uuid::new_v4();
    let first = [step(first_step, 1, "INSPECT_SOURCES", false, vec![])];
    let second = [step(second_step, 1, "CHECK_ASSUMPTIONS", false, vec![])];
    let (first, second) = tokio::join!(
        agentic_controller::create_plan(&app, scope, s.principal, &first),
        agentic_controller::create_plan(&app, scope, s.principal, &second),
    );
    let (first_version, first_sha) = first.unwrap();
    let (second_version, second_sha) = second.unwrap();
    let mut revisions = [first_version, second_version];
    revisions.sort_unstable();
    assert_eq!(revisions, [1, 2]);
    let view = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(view.plan_revision, 2);
    let (latest_step, latest_sha) = if first_version == 2 {
        (first_step, first_sha)
    } else {
        (second_step, second_sha)
    };
    assert_eq!(view.plan_sha256, latest_sha);
    assert_eq!(view.ready_step_ids, [latest_step]);
}

#[tokio::test]
async fn an_active_step_prevents_replacement_until_its_outcome_is_recorded() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let id = Uuid::new_v4();
    let original = [step(id, 1, "INSPECT_SOURCES", false, vec![])];
    let replacement = [step(id, 1, "CHECK_ASSUMPTIONS", false, vec![])];
    agentic_controller::create_plan(&app, scope, s.principal, &original).await.unwrap();
    agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    assert!(matches!(
        agentic_controller::create_plan(&app, scope, s.principal, &replacement).await,
        Err(agentic_controller::ControllerError::StateConflict)
    ));
    let prior = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(prior.plan_revision, 1);
    assert_eq!(prior.steps[0].state, "STARTED");
    agentic_controller::finish_step(&app, scope, id, s.principal, "SUCCEEDED", "completed")
        .await
        .unwrap();
    let (revision, _) =
        agentic_controller::create_plan(&app, scope, s.principal, &replacement).await.unwrap();
    assert_eq!(revision, 2);
}

#[tokio::test]
async fn cancelled_operation_cannot_acquire_a_new_plan() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let prior = bff_operations::get(&app, scope).await.unwrap();
    bff_operations::cancel(&app, scope, prior.state_version, s.principal, "cancelled")
        .await
        .unwrap();
    let steps = [step(Uuid::new_v4(), 1, "INSPECT_SOURCES", false, vec![])];
    assert!(matches!(
        agentic_controller::create_plan(&app, scope, s.principal, &steps).await,
        Err(agentic_controller::ControllerError::StateConflict)
    ));
    assert!(matches!(
        agentic_runtime::ensure_default_plan(&app, scope, s.principal).await,
        Err(agentic_runtime::RuntimeError::Controller(
            agentic_controller::ControllerError::StateConflict
        ))
    ));
    assert!(matches!(
        agentic_controller::view(&app, scope).await,
        Err(agentic_controller::ControllerError::Invisible)
    ));
    let model = DeterministicModel::new("fixture", "unused").unwrap();
    let outcome = agentic_runtime::run_until_boundary(&app, scope, s.principal, &model)
        .await.unwrap();
    assert!(outcome.terminal);
    assert_eq!(outcome.state, "CANCELLED");
    assert_eq!(outcome.completed_steps, 0);
}

#[tokio::test]
async fn concurrent_followups_from_one_completed_plan_share_one_revision() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let id = Uuid::new_v4();
    let initial = [step(id, 1, "INSPECT_SOURCES", false, vec![])];
    let (revision, _) = agentic_controller::create_plan(&app, scope, s.principal, &initial)
        .await.unwrap();
    agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    agentic_controller::finish_step(&app, scope, id, s.principal, "SUCCEEDED", "done")
        .await.unwrap();
    let followup = [step(Uuid::new_v4(), 1, "REQUEST_HUMAN_DECISION", true, vec![])];
    let (first, second) = tokio::join!(
        agentic_controller::append_followup_plan(&app, scope, s.principal, &followup, revision),
        agentic_controller::append_followup_plan(&app, scope, s.principal, &followup, revision),
    );
    assert_ne!(first.unwrap(), second.unwrap());
    assert_eq!(agentic_controller::view(&app, scope).await.unwrap().plan_revision, 2);
}

#[tokio::test]
async fn superseded_plan_cannot_complete_the_operation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let id = Uuid::new_v4();
    let steps = [step(id, 1, "INSPECT_SOURCES", false, vec![])];
    let (old, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
        .await.unwrap();
    agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    agentic_controller::finish_step(&app, scope, id, s.principal, "SUCCEEDED", "done")
        .await.unwrap();
    let (current, _) = agentic_controller::create_plan(&app, scope, s.principal, &steps)
        .await.unwrap();
    let result = json!({"plan_revision": old});
    assert!(!agentic_controller::complete_plan(&app, scope, s.principal, old, &result)
        .await.unwrap());
    assert!(!agentic_controller::complete_plan(&app, scope, s.principal, current, &result)
        .await.unwrap());
    assert_eq!(bff_operations::get(&app, scope).await.unwrap().state, "RUNNING");

    agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    agentic_controller::finish_step(&app, scope, id, s.principal, "SUCCEEDED", "done")
        .await.unwrap();
    let result = json!({"plan_revision": current});
    assert!(agentic_controller::complete_plan(&app, scope, s.principal, current, &result)
        .await.unwrap());
    let operation = bff_operations::get(&app, scope).await.unwrap();
    assert_eq!(operation.state, "SUCCEEDED");
    assert_eq!(operation.result, Some(result));
}

#[tokio::test]
async fn stale_plan_selection_cannot_start_a_reused_step_id() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let id = Uuid::new_v4();
    let (old, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let (current, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(id, 1, "CHECK_ASSUMPTIONS", false, vec![])],
    )
    .await
    .unwrap();
    assert!(matches!(
        agentic_controller::start_step_at_revision(&app, scope, id, s.principal, old).await,
        Err(agentic_controller::ControllerError::PlanChanged)
    ));
    assert_eq!(agentic_controller::view(&app, scope).await.unwrap().budget_used, 0);
    let view = agentic_controller::start_step_at_revision(&app, scope, id, s.principal, current)
        .await
        .unwrap();
    assert_eq!(view.budget_used, 1);
    assert_eq!(view.steps[0].kind, "CHECK_ASSUMPTIONS");
}

#[tokio::test]
async fn concurrent_first_step_starts_transition_the_operation_once() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 1).await;
    let id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app, scope, s.principal, &[step(id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        agentic_controller::start_step(&app, scope, id, s.principal),
        agentic_controller::start_step(&app, scope, id, s.principal),
    );
    assert!(matches!(
        (a, b),
        (Ok(_), Err(agentic_controller::ControllerError::BudgetExhausted))
            | (Err(agentic_controller::ControllerError::BudgetExhausted), Ok(_))
    ));
    let events = bff_operations::events(&app, scope, 0, 20).await.unwrap();
    assert_eq!(
        events.events.iter().map(|event| event.kind.as_str()).collect::<Vec<_>>(),
        ["QUEUED", "RUNNING"]
    );
    let current = agentic_controller::view(&app, scope).await.unwrap();
    assert_eq!(current.budget_used, 1);
    assert_eq!(current.steps[0].state, "STARTED");
}

#[tokio::test]
async fn concurrent_step_starts_share_one_plan_budget_and_do_not_start_twice() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    for distinct_steps in [false, true] {
        let scope = op(&app, s, if distinct_steps { 1 } else { 2 }).await;
        let first = Uuid::new_v4();
        let other = Uuid::new_v4();
        let last = if distinct_steps { other } else { first };
        agentic_controller::create_plan(
            &app,
            scope,
            s.principal,
            &[
                step(first, 1, "INSPECT_SOURCES", false, vec![]),
                step(other, 2, "CHECK_ASSUMPTIONS", false, vec![]),
            ],
        )
        .await
        .unwrap();
        let queued = bff_operations::get(&app, scope).await.unwrap();
        bff_operations::start(&app, scope, queued.state_version, s.principal)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            agentic_controller::start_step(&app, scope, first, s.principal),
            agentic_controller::start_step(&app, scope, last, s.principal),
        );
        let err = match (a, b) {
            (Ok(_), Err(err)) | (Err(err), Ok(_)) => err,
            other => panic!("expected one successful start, got {other:?}"),
        };
        if distinct_steps {
            assert!(matches!(err, agentic_controller::ControllerError::BudgetExhausted));
        } else {
            assert!(matches!(err, agentic_controller::ControllerError::NotReady));
        }
        let view = agentic_controller::view(&app, scope).await.unwrap();
        assert_eq!(view.budget_used, 1);
        assert_eq!(view.steps.iter().filter(|step| step.state == "STARTED").count(), 1);
    }
}

#[tokio::test]
async fn concurrent_step_finishes_record_one_outcome() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let step_id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[step(step_id, 1, "INSPECT_SOURCES", false, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, step_id, s.principal)
        .await
        .unwrap();
    for invalid in ["invalid\0note".to_owned(), "á".repeat(4097)] {
        assert!(matches!(
            agentic_controller::finish_step(
                &app, scope, step_id, s.principal, "SUCCEEDED", &invalid,
            ).await,
            Err(agentic_controller::ControllerError::Invalid)
        ));
    }
    let (success, failure) = tokio::join!(
        agentic_controller::finish_step(&app, scope, step_id, s.principal, "SUCCEEDED", "completed"),
        agentic_controller::finish_step(&app, scope, step_id, s.principal, "FAILED", "failed"),
    );
    let outcome = match (success, failure) {
        (Ok(_), Err(agentic_controller::ControllerError::StateConflict)) => "SUCCEEDED",
        (Err(agentic_controller::ControllerError::StateConflict), Ok(_)) => "FAILED",
        other => panic!("expected one durable outcome, got {other:?}"),
    };
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM niccolo.agentic_step_events
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4
         ORDER BY event_seq",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(step_id)
    .fetch_all(&admin)
    .await
    .unwrap();
    assert_eq!(events, ["STARTED", outcome]);
}

#[tokio::test]
async fn concurrent_default_plan_requests_share_the_first_revision() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 32).await;
    let (a, b) = tokio::join!(
        agentic_runtime::ensure_default_plan(&app, scope, s.principal),
        agentic_runtime::ensure_default_plan(&app, scope, s.principal),
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.plan_revision, 1);
    assert_eq!(a, b);
    let plan_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_plans
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(plan_count, 1);
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
    let mut invalid_label = step(y, 2, "SYNTHESIZE", false, vec![x]);
    invalid_label.label = "invalid\0label".into();
    for invalid in [
        step(y, 2, "SYNTHESIZE", false, vec![x, x]),
        invalid_label,
    ] {
        assert!(matches!(
            agentic_controller::create_plan(
                &p,
                sc,
                s.principal,
                &[step(x, 1, "INSPECT_SOURCES", false, vec![]), invalid],
            )
            .await,
            Err(agentic_controller::ControllerError::Invalid)
        ));
    }
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
async fn recovery_requeues_only_stale_steps_in_plan_order() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let plan = [
        step(ids[0], 1, "INSPECT_SOURCES", false, vec![]),
        step(ids[1], 2, "CHECK_ASSUMPTIONS", false, vec![]),
        step(ids[2], 3, "CHECK_STOP_CONDITIONS", false, vec![]),
    ];
    let (revision, _) = agentic_controller::create_plan(&app, scope, s.principal, &plan)
        .await
        .unwrap();
    for id in ids {
        agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    }
    sqlx::query(
        "UPDATE niccolo.agentic_step_events
         SET created_at=clock_timestamp()-interval '10 minutes'
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
           AND plan_revision=$4 AND step_id=ANY($5) AND event_kind='STARTED'",
    )
    .bind(s.workspace).bind(s.situation).bind(scope.operation)
    .bind(revision).bind(&ids[..2]).execute(&admin).await.unwrap();
    let view = agentic_controller::recover_stale_started(&app, scope, s.principal, 120)
        .await
        .unwrap();
    assert_eq!(view.ready_step_ids, ids[..2]);
    assert_eq!(view.steps[2].state, "STARTED");
    let recovery_order: Vec<Uuid> = sqlx::query_scalar(
        "SELECT step_id FROM niccolo.agentic_step_events
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
           AND plan_revision=$4 AND event_kind='RETRY_READY'
         ORDER BY event_seq",
    )
    .bind(s.workspace).bind(s.situation).bind(scope.operation)
    .bind(revision).fetch_all(&admin).await.unwrap();
    assert_eq!(recovery_order, ids[..2]);
}

#[tokio::test]
async fn cancelled_operation_does_not_record_a_new_human_pause() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app, scope, s.principal, &[step(id, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    let current = bff_operations::get(&app, scope).await.unwrap();
    bff_operations::cancel(&app, scope, current.state_version, s.principal, "cancelled")
        .await
        .unwrap();
    assert!(matches!(
        agentic_controller::finish_step(
            &app, scope, id, s.principal, "WAITING_FOR_HUMAN", "review required",
        )
        .await,
        Err(agentic_controller::ControllerError::Operation(
            bff_operations::BffOperationError::StateConflict
        ))
    ));
    sqlx::query(
        "UPDATE niccolo.agentic_step_events
         SET created_at=clock_timestamp()-interval '10 minutes'
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .bind(id)
    .execute(&admin)
    .await
    .unwrap();
    agentic_controller::recover_stale_started(&app, scope, s.principal, 120)
        .await
        .unwrap();
    assert_eq!(last_step_event(&admin, scope, id).await.0, "STARTED");
    assert_eq!(bff_operations::get(&app, scope).await.unwrap().state, "CANCELLED");
}

#[tokio::test]
async fn failed_resume_keeps_the_operation_and_step_paused() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let id = Uuid::new_v4();
    agentic_controller::create_plan(
        &app, scope, s.principal, &[step(id, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    agentic_controller::finish_step(&app, scope, id, s.principal, "WAITING_FOR_HUMAN", "review")
        .await
        .unwrap();

    let err = agentic_controller::resume_step(&app, scope, id, s.principal, &"a".repeat(4097))
        .await;
    assert!(matches!(err, Err(agentic_controller::ControllerError::Database(_))));
    assert_eq!(last_step_event(&admin, scope, id).await.0, "WAITING_FOR_HUMAN");
    assert_eq!(bff_operations::get(&app, scope).await.unwrap().state, "WAITING_FOR_HUMAN");

    let resumed = agentic_controller::resume_step(&app, scope, id, s.principal, "approved")
        .await
        .unwrap();
    assert_eq!(resumed.steps[0].state, "STARTED");
    assert_eq!(bff_operations::get(&app, scope).await.unwrap().state, "RUNNING");
}

#[tokio::test]
async fn a_legacy_pending_human_response_can_still_finish() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let gate = Uuid::new_v4();
    let response = Uuid::new_v4();
    let (revision, _) = agentic_controller::create_plan(
        &app, scope, s.principal, &[step(gate, 1, "REQUEST_HUMAN_DECISION", true, vec![])],
    )
    .await
    .unwrap();
    agentic_controller::start_step(&app, scope, gate, s.principal).await.unwrap();
    agentic_controller::finish_step(&app, scope, gate, s.principal, "WAITING_FOR_HUMAN", "review")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.agentic_human_responses
             (workspace_id,situation_id,operation_id,plan_revision,step_id,response_id,
              decision,response_text,actor_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,'STOP','',$7)",
    )
    .bind(s.workspace).bind(s.situation).bind(scope.operation).bind(revision)
    .bind(gate).bind(response).bind(s.principal)
    .execute(&admin).await.unwrap();

    let outcome = agentic_controller::resolve_human_boundary(
        &app, scope, gate, response, s.principal, "STOP", "",
    )
    .await
    .unwrap();
    assert_eq!(outcome.operation_state, "CANCELLED");
    let recorded: Vec<(Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT result_operation_state,result_plan_revision
         FROM niccolo.agentic_human_responses
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace).bind(s.situation).bind(scope.operation)
    .fetch_all(&admin).await.unwrap();
    assert_eq!(recorded, [(None, None)]);
}

#[tokio::test]
async fn deferred_replan_leaves_the_human_response_and_pause_retryable() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = op(&app, s, 10).await;
    let gate = Uuid::new_v4();
    let active = Uuid::new_v4();
    agentic_controller::create_plan(
        &app,
        scope,
        s.principal,
        &[
            step(gate, 1, "REQUEST_HUMAN_DECISION", true, vec![]),
            step(active, 2, "INSPECT_SOURCES", false, vec![]),
        ],
    ).await.unwrap();
    for id in [gate, active] {
        agentic_controller::start_step(&app, scope, id, s.principal).await.unwrap();
    }
    agentic_controller::finish_step(&app, scope, gate, s.principal, "WAITING_FOR_HUMAN", "review")
        .await
        .unwrap();

    let response = Uuid::new_v4();
    let result = agentic_controller::resolve_human_boundary(
        &app, scope, gate, response, s.principal, "REPLAN", "use a different approach",
    ).await;
    assert!(matches!(result, Err(agentic_controller::ControllerError::StateConflict)));
    assert_eq!(bff_operations::get(&app, scope).await.unwrap().state, "WAITING_FOR_HUMAN");
    assert_eq!(last_step_event(&admin, scope, gate).await.0, "WAITING_FOR_HUMAN");
    let responses: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_human_responses
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(responses, 0);

    agentic_controller::finish_step(&app, scope, active, s.principal, "SUCCEEDED", "completed")
        .await
        .unwrap();
    let result = agentic_controller::resolve_human_boundary(
        &app, scope, gate, response, s.principal, "REPLAN", "use a different approach",
    ).await.unwrap();
    assert_eq!(result.plan_revision, 2);
    assert_eq!(result.operation_state, "RUNNING");
    assert!(result.human_response_persisted);
    assert!(!result.system_inferred_consent);
    let recorded: (String, i64) = sqlx::query_as(
        "SELECT result_operation_state,result_plan_revision
         FROM niccolo.agentic_human_responses
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND response_id=$4",
    )
    .bind(s.workspace).bind(s.situation).bind(scope.operation).bind(response)
    .fetch_one(&admin).await.unwrap();
    assert_eq!(recorded, ("RUNNING".into(), 2));
    let replay = agentic_controller::resolve_human_boundary(
        &app, scope, gate, response, s.principal, "REPLAN", "use a different approach",
    ).await.unwrap();
    assert_eq!(replay, result);
    assert!(matches!(
        agentic_controller::resolve_human_boundary(
            &app, scope, gate, response, s.principal, "REPLAN", "different response",
        ).await,
        Err(agentic_controller::ControllerError::StateConflict)
    ));
    assert_eq!(agentic_controller::view(&app, scope).await.unwrap().plan_revision, 2);

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
