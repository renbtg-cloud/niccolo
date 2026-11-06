#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    agentic_controller::{self, StepInput},
    bff_operations::{self, NewOperation, OperationScope},
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
