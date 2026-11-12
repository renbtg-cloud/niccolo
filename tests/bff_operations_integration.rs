#![cfg(feature = "pg-integration")]
use niccolo_evidence::bff_operations::{self, NewOperation, OperationScope};
use niccolo_evidence::operation_budget;
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
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let app = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
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
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'bff')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus',$3,1)").bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'focus',$3,$4,'CREATED')").bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    s
}
fn op(s: Seed, id: Uuid) -> OperationScope {
    OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: id,
    }
}
fn input(s: Seed, kind: &str, key: Option<&str>) -> NewOperation {
    NewOperation {
        kind: kind.into(),
        target_analysis_id: Some(s.analysis),
        parent_operation_id: None,
        idempotency_key: key.map(str::to_string),
        input: json!({"analysis_id":s.analysis}),
        deadline_at: None,
        step_budget: 32,
        requested_by: s.principal,
    }
}

#[tokio::test]
async fn v182_v186_operation_is_durable_idempotent_and_conflict_detecting() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let a = Uuid::new_v4();
    let first = bff_operations::submit(&app, op(s, a), &input(s, "BUILD_BRIEFING", Some("same")))
        .await
        .unwrap();
    assert_eq!(first.state, "QUEUED");
    assert_eq!(first.state_version, 1);
    assert_eq!(first.retry_after_ms, Some(750));
    assert!(!first.terminal);
    let replay = bff_operations::submit(
        &app,
        op(s, Uuid::new_v4()),
        &input(s, "BUILD_BRIEFING", Some("same")),
    )
    .await
    .unwrap();
    assert_eq!(replay.operation_id, a);
    let mut changed = input(s, "BUILD_BRIEFING", Some("same"));
    changed.input = json!({"analysis_id":s.analysis,"changed":true});
    assert!(matches!(
        bff_operations::submit(&app, op(s, Uuid::new_v4()), &changed).await,
        Err(bff_operations::BffOperationError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn concurrent_same_key_submissions_reuse_one_operation_and_queue_event() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let request = input(s, "AGENTIC_ANALYSIS", Some("parallel-replay"));
    let scopes = (0..10).map(|_| op(s, Uuid::new_v4())).collect::<Vec<_>>();
    let mut tasks = tokio::task::JoinSet::new();
    for scope in scopes {
        let pool = app.clone();
        let submitted = request.clone();
        tasks.spawn(async move { bff_operations::submit(&pool, scope, &submitted).await });
    }
    let mut winner = None;
    while let Some(joined) = tasks.join_next().await {
        let operation = joined.unwrap().unwrap();
        if let Some(id) = winner {
            assert_eq!(operation.operation_id, id);
        } else {
            winner = Some(operation.operation_id);
        }
        assert_eq!(operation.state, "QUEUED");
    }
    let winner = winner.unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.bff_operations          WHERE workspace_id=$1 AND situation_id=$2 AND idempotency_key='parallel-replay'"
    )
    .bind(s.workspace)
    .bind(s.situation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.bff_operation_events          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND event_kind='QUEUED'"
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(winner)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(events, 1);

    let mut changed = request.clone();
    changed.input = json!({"different":"intent"});
    assert!(matches!(
        bff_operations::submit(&app, op(s, Uuid::new_v4()), &changed).await,
        Err(bff_operations::BffOperationError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn idempotency_rejects_different_immutable_operation_metadata() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let id = Uuid::new_v4();
    let original = input(s, "AGENTIC_ANALYSIS", Some("immutable"));
    bff_operations::submit(&app, op(s, id), &original)
        .await
        .unwrap();

    let mut variants = Vec::new();
    let mut steps = original.clone();
    steps.step_budget = 33;
    variants.push(steps);
    let mut analysis = original.clone();
    analysis.target_analysis_id = Some(Uuid::new_v4());
    variants.push(analysis);
    let mut parent = original.clone();
    parent.parent_operation_id = Some(Uuid::new_v4());
    variants.push(parent);
    let mut actor = original.clone();
    actor.requested_by = Uuid::new_v4();
    variants.push(actor);
    let mut deadline = original.clone();
    deadline.deadline_at = Some("2099-01-01T00:00:00Z".into());
    variants.push(deadline);

    for variant in variants {
        assert!(matches!(
            bff_operations::submit(&app, op(s, Uuid::new_v4()), &variant).await,
            Err(bff_operations::BffOperationError::IdempotencyConflict)
        ));
    }

    let mut changed_id = input(s, "AGENTIC_ANALYSIS", None);
    changed_id.step_budget = 64;
    assert!(matches!(
        bff_operations::submit(&app, op(s, id), &changed_id).await,
        Err(bff_operations::BffOperationError::IdempotencyConflict)
    ));
    let same_id_wrong_key = input(s, "AGENTIC_ANALYSIS", Some("other-key"));
    assert!(matches!(
        bff_operations::submit(&app, op(s, id), &same_id_wrong_key).await,
        Err(bff_operations::BffOperationError::IdempotencyConflict)
    ));
}

#[tokio::test]
async fn v187_v192_sync_execution_and_pull_cursor_share_one_operation() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let id = Uuid::new_v4();
    let sc = op(s, id);
    bff_operations::submit(&app, sc, &input(s, "BUILD_BRIEFING", None))
        .await
        .unwrap();
    let done = bff_operations::execute_deterministic(&app, sc, s.principal)
        .await
        .unwrap();
    assert_eq!(done.state, "SUCCEEDED");
    assert!(done.terminal);
    assert!(done.result.is_some());
    assert_eq!(done.retry_after_ms, None);
    assert!(done.etag.contains(&done.state_version.to_string()));
    let p1 = bff_operations::events(&app, sc, 0, 1).await.unwrap();
    assert_eq!(p1.events.len(), 1);
    assert!(p1.next_after.is_some());
    let p2 = bff_operations::events(&app, sc, p1.next_after.unwrap(), 10)
        .await
        .unwrap();
    assert!(p2.events.iter().any(|e| e.kind == "RUNNING"));
    assert!(p2.events.iter().any(|e| e.kind == "SUCCEEDED"));
}

#[tokio::test]
async fn v193_v195_cancel_and_human_pause_resume_are_explicit_states() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cancelled = Uuid::new_v4();
    let csc = op(s, cancelled);
    let q = bff_operations::submit(&app, csc, &input(s, "AGENTIC_ANALYSIS", None))
        .await
        .unwrap();
    let c = bff_operations::cancel(&app, csc, q.state_version, s.principal, "human stopped it")
        .await
        .unwrap();
    assert_eq!(c.state, "CANCELLED");
    assert!(c.terminal);
    let id = Uuid::new_v4();
    let sc = op(s, id);
    bff_operations::submit(&app, sc, &input(s, "AGENTIC_ANALYSIS", None))
        .await
        .unwrap();
    assert!(matches!(
        bff_operations::execute_deterministic(&app, sc, s.principal).await,
        Err(bff_operations::BffOperationError::UnsupportedKind)
    ));
    let running = bff_operations::get(&app, sc).await.unwrap();
    assert_eq!(running.state, "RUNNING");
    let waiting = bff_operations::wait_for_human(
        &app,
        sc,
        running.state_version,
        s.principal,
        "need a high-information answer",
    )
    .await
    .unwrap();
    assert_eq!(waiting.state, "WAITING_FOR_HUMAN");
    assert_eq!(waiting.retry_after_ms, Some(3000));
    let resumed = bff_operations::resume(&app, sc, waiting.state_version, s.principal)
        .await
        .unwrap();
    assert_eq!(resumed.state, "RUNNING");
}

#[tokio::test]
async fn v196_v200_parent_budget_and_operation_digest_contracts_are_preserved() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let parent_id = Uuid::new_v4();
    let parent = bff_operations::submit(
        &app,
        op(s, parent_id),
        &input(s, "AGENTIC_ANALYSIS", Some("parent")),
    )
    .await
    .unwrap();
    let child_id = Uuid::new_v4();
    let mut child = input(s, "BUILD_HANDOFF", Some("child"));
    child.parent_operation_id = Some(parent.operation_id);
    child.step_budget = 7;
    let child = bff_operations::submit(&app, op(s, child_id), &child)
        .await
        .unwrap();
    assert_eq!(child.parent_operation_id, Some(parent_id));
    assert_eq!(child.step_budget, 7);
    let events = bff_operations::events(&app, op(s, child_id), 0, 200)
        .await
        .unwrap();
    assert_eq!(events.events[0].kind, "QUEUED");
}

#[tokio::test]
async fn malformed_deadline_is_rejected_before_postgres_cast_or_persistence() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let mut bad = input(s, "AGENTIC_ANALYSIS", None);
    bad.deadline_at = Some("tomorrow-ish".into());
    let scope = op(s, Uuid::new_v4());
    assert!(matches!(
        bff_operations::submit(&app, scope, &bad).await,
        Err(bff_operations::BffOperationError::Invalid)
    ));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.bff_operations
          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(scope.operation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn operation_budget_reports_actual_postgres_deadline_and_expiry() {
    let (admin, app) = pools().await;
    let seed = seed(&admin).await;

    let live_scope = op(seed, Uuid::new_v4());
    let mut live = input(seed, "BUILD_BRIEFING", None);
    live.deadline_at = Some("2099-01-01T00:00:00Z".into());
    live.input = json!({
        "analysis_id": seed.analysis,
        "model_budget": {"max_invocations": 2, "max_total_tokens": 80}
    });
    bff_operations::submit(&app, live_scope, &live)
        .await
        .unwrap();
    let current = operation_budget::view(&app, live_scope).await.unwrap();
    assert!(current.deadline_active);
    assert!(current.within_deadline);
    assert!(current.remaining_deadline_ms.unwrap() > 0);
    assert_eq!(current.remaining_invocations, 2);
    assert_eq!(current.remaining_total_tokens, 80);
    assert!(current.stop_reasons.is_empty());
    assert!(current.can_start_model_call);

    let expired_scope = op(seed, Uuid::new_v4());
    let mut expired_input = input(seed, "BUILD_BRIEFING", None);
    expired_input.deadline_at = Some("2000-01-01T00:00:00Z".into());
    bff_operations::submit(&app, expired_scope, &expired_input)
        .await
        .unwrap();
    let past = operation_budget::view(&app, expired_scope).await.unwrap();
    assert_eq!(past.remaining_deadline_ms, Some(0));
    assert!(!past.within_deadline);
    assert!(!past.can_start_model_call);
    assert_eq!(past.stop_reasons, vec!["DEADLINE_EXPIRED"]);
    assert!(matches!(
        operation_budget::require_available(&app, expired_scope).await,
        Err(operation_budget::BudgetError::Deadline)
    ));
}

#[tokio::test]
async fn adaptive_admission_serializes_competing_workers_at_workspace_limit() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    for _ in 0..6 {
        let scope = op(s, Uuid::new_v4());
        bff_operations::submit(&app, scope, &input(s, "BUILD_BRIEFING", None))
            .await
            .unwrap();
    }

    let mut joins = Vec::new();
    for idx in 0..8 {
        let pool = app.clone();
        let workspace = s.workspace;
        joins.push(tokio::spawn(async move {
            let owner = format!("adaptive-{idx}");
            let claimed = bff_operations::claim_next_in_workspace_limited(
                &pool, workspace, &owner, 60_000, 2,
            )
            .await
            .unwrap();
            (owner, claimed)
        }));
    }

    let mut claimed = Vec::new();
    let mut rejected = 0;
    for join in joins {
        let (owner, scope) = join.await.unwrap();
        match scope {
            Some(scope) => claimed.push((owner, scope)),
            None => rejected += 1,
        }
    }
    assert_eq!(claimed.len(), 2);
    assert_eq!(rejected, 6);

    let (owner, released) = claimed.pop().unwrap();
    assert!(
        bff_operations::release_execution(&app, released, &owner)
            .await
            .unwrap()
    );
    let replacement = bff_operations::claim_next_in_workspace_limited(
        &app,
        s.workspace,
        "adaptive-replacement",
        60_000,
        2,
    )
    .await
    .unwrap();
    assert!(replacement.is_some());
}

#[tokio::test]
async fn adaptive_admission_rejects_zero_or_excessive_limit() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    for limit in [0, 513] {
        assert!(matches!(
            bff_operations::claim_next_in_workspace_limited(
                &app,
                s.workspace,
                "invalid-limit",
                60_000,
                limit,
            )
            .await,
            Err(bff_operations::BffOperationError::Invalid)
        ));
    }
}
