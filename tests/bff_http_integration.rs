#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz, model_adapter::DeterministicModel};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct Seed {
    admin: PgPool,
    router: Router,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    token: String,
}
async fn setup() -> Seed {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'bff-http')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus',$3,1)").bind(workspace).bind(situation).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'focus',$3,$4,'CREATED')").bind(workspace).bind(situation).bind("a".repeat(64)).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)").bind(workspace).bind(situation).bind(analysis).bind("a".repeat(64)).bind(principal).execute(&admin).await.unwrap();
    Seed {
        admin,
        router: api::router_with_model(
            secure,
            Arc::new(DeterministicModel::new("test", "deterministic-v1").unwrap()),
        ),
        workspace,
        situation,
        analysis,
        token,
    }
}
fn path(s: &Seed, id: Uuid, suffix: &str) -> String {
    format!(
        "/v1/workspaces/{}/situations/{}/bff/operations/{}{}",
        s.workspace, s.situation, id, suffix
    )
}
fn intent_path(s: &Seed, id: Uuid, suffix: &str) -> String {
    format!(
        "/v1/workspaces/{}/situations/{}/bff/intents/{}{}",
        s.workspace, s.situation, id, suffix
    )
}
fn req(
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
    if_none_match: Option<&str>,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json");
    if let Some(tag) = if_none_match {
        b = b.header(header::IF_NONE_MATCH, tag);
    }
    let data = body
        .map(|v| serde_json::to_vec(&v).unwrap())
        .unwrap_or_default();
    if !data.is_empty() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    b.body(Body::from(data)).unwrap()
}
async fn invoke(router: Router, r: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let resp = router.oneshot(r).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, headers, v)
}
fn body(s: &Seed, kind: &str) -> Value {
    json!({"kind":kind,"target_analysis_id":s.analysis,"idempotency_key":format!("{}-{kind}-{}",s.analysis,Uuid::new_v4()),"input":{},"step_budget":12})
}

#[tokio::test]
async fn v201_v204_sync_and_async_http_surfaces_share_operation_shape() {
    let s = setup().await;
    let sync = Uuid::new_v4();
    let (status, h, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, sync, "/sync"),
            &s.token,
            Some(body(&s, "BUILD_BRIEFING")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], "SUCCEEDED");
    assert!(h.get(header::ETAG).is_some());
    assert_eq!(v["operation_id"], sync.to_string());
    let async_id = Uuid::new_v4();
    let (status, h, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, async_id, "/async"),
            &s.token,
            Some(body(&s, "BUILD_HANDOFF")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(v["operation_id"], async_id.to_string());
    assert!(h.get(header::RETRY_AFTER).is_some());
    for _ in 0..30 {
        let (_, _, now) = invoke(
            s.router.clone(),
            req("GET", &path(&s, async_id, ""), &s.token, None, None),
        )
        .await;
        if now["state"] == "SUCCEEDED" {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("async deterministic operation did not finish");
}

#[tokio::test]
async fn v205_v208_pull_etag_poll_hint_events_and_cancel_are_first_class() {
    let s = setup().await;
    let id = Uuid::new_v4();
    let (status, h, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/async"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(v["state"], "QUEUED");
    assert_eq!(h["x-poll-after-ms"], "750");
    let tag = h[header::ETAG].to_str().unwrap().to_string();
    let (status, _, _) = invoke(
        s.router.clone(),
        req("GET", &path(&s, id, ""), &s.token, None, Some(&tag)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    let (status, _, events) = invoke(
        s.router.clone(),
        req(
            "GET",
            &format!("{}?after=0&limit=1", path(&s, id, "/events")),
            &s.token,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events["events"].as_array().unwrap().len(), 1);
    let version = v["state_version"].as_i64().unwrap();
    let (status, _, cancelled) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/cancel"),
            &s.token,
            Some(json!({"expected_version":version,"reason":"human says stop"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled["state"], "CANCELLED");
    assert_eq!(cancelled["terminal"], true);
    drop(s.admin);
}

#[tokio::test]
async fn v209_v210_read_token_cannot_submit_or_cancel() {
    let s = setup().await;
    let read_principal = Uuid::new_v4();
    let read = authz::provision_token(&s.admin, read_principal, s.workspace, true, false, 1)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/async"),
            &read,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn v301_v315_agentic_sync_async_and_pull_share_one_operation_semantics() {
    let s = setup().await;
    let sync = Uuid::new_v4();
    let (status, _, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, sync, "/sync"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], "SUCCEEDED");
    let async_id = Uuid::new_v4();
    let (status, _, queued) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, async_id, "/async"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let version = queued["state_version"].as_i64().unwrap();
    let wait_path = format!(
        "{}?after_version={version}&timeout_ms=3000",
        path(&s, async_id, "/wait")
    );
    let (status, _, changed) = invoke(
        s.router.clone(),
        req("GET", &wait_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(changed["state_version"].as_i64().unwrap() > version || changed["terminal"] == true);
    for _ in 0..40 {
        let (_, _, now) = invoke(
            s.router.clone(),
            req("GET", &path(&s, async_id, ""), &s.token, None, None),
        )
        .await;
        if now["terminal"] == true {
            assert_eq!(now["state"], "SUCCEEDED");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("async agentic operation did not finish");
}

#[tokio::test]
async fn v316_v325_agentic_plan_is_inspectable_through_bff() {
    let s = setup().await;
    let id = Uuid::new_v4();
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/async"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    for _ in 0..40 {
        let (status, _, plan) = invoke(
            s.router.clone(),
            req("GET", &path(&s, id, "/agentic"), &s.token, None, None),
        )
        .await;
        if status == StatusCode::OK {
            assert!(plan["steps"].as_array().unwrap().len() >= 6);
            assert!(plan["plan_sha256"].as_str().unwrap().len() == 64);
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("agentic plan was not visible");
}

#[tokio::test]
async fn v391_v430_human_intent_bff_creates_frozen_analysis_and_uses_same_agentic_runtime() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = json!({
        "analysis_id": analysis,
        "intent": "UNDERSTAND",
        "goal": null,
        "reanalysis_of": null,
        "idempotency_key": format!("intent-{operation}"),
        "step_budget": 32,
        "context": {"surface":"http-integration","human_owns_project":true}
    });
    let (status, headers, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(request),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["analysis_id"], analysis.to_string());
    assert_eq!(value["intent"], "UNDERSTAND");
    assert_eq!(value["operation"]["operation_id"], operation.to_string());
    assert_eq!(value["operation"]["state"], "SUCCEEDED");
    assert!(headers.get(header::ETAG).is_some());

    let analysis_path = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}",
        s.workspace, s.situation, analysis
    );
    let (status, _, frozen) = invoke(
        s.router.clone(),
        req("GET", &analysis_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(frozen["analysis_id"], analysis.to_string());
    assert_eq!(frozen["frozen_input"], true);
    assert_eq!(frozen["incorporates_future_context"], false);
    assert!(frozen["goal"].as_str().unwrap().contains("evidence"));
}

fn intent_request(operation: Uuid, analysis: Uuid, intent: &str) -> Value {
    json!({
        "analysis_id": analysis,
        "intent": intent,
        "goal": null,
        "reanalysis_of": null,
        "idempotency_key": format!("intent-aware-{operation}"),
        "step_budget": 32,
        "context": {"surface":"intent-aware-planning-test","human_owns_project":true}
    })
}

async fn agentic_plan(s: &Seed, operation: Uuid) -> Value {
    let (status, _, plan) = invoke(
        s.router.clone(),
        req("GET", &path(s, operation, "/agentic"), &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    plan
}

fn step<'a>(plan: &'a Value, kind: &str) -> Option<&'a Value> {
    plan["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["kind"] == kind)
}

#[tokio::test]
async fn v431_v440_about_to_act_or_say_forces_explicit_human_boundary() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let (status, _, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(intent_request(operation, analysis, "ABOUT_TO_ACT_OR_SAY")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["operation"]["state"], "WAITING_FOR_HUMAN");
    assert_eq!(value["operation"]["terminal"], false);

    let plan = agentic_plan(&s, operation).await;
    let synthesis = step(&plan, "SYNTHESIZE").unwrap();
    assert!(
        synthesis["label"]
            .as_str()
            .unwrap()
            .contains("before the human acts")
    );
    let human = step(&plan, "REQUEST_HUMAN_DECISION").unwrap();
    assert_eq!(human["requires_human"], true);
    assert_eq!(human["state"], "WAITING_FOR_HUMAN");
    assert!(
        human["label"]
            .as_str()
            .unwrap()
            .contains("human must choose")
    );
    assert_eq!(step(&plan, "PREPARE_HANDOFF").unwrap()["state"], "PENDING");
}

#[tokio::test]
async fn v441_v450_understand_changes_synthesis_without_forcing_human_choice() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let (status, _, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(intent_request(operation, analysis, "UNDERSTAND")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["operation"]["state"], "SUCCEEDED");

    let plan = agentic_plan(&s, operation).await;
    assert!(step(&plan, "REQUEST_HUMAN_DECISION").is_none());
    assert!(
        step(&plan, "SYNTHESIZE").unwrap()["label"]
            .as_str()
            .unwrap()
            .contains("provisional understanding")
    );
    assert_eq!(plan["complete"], true);
}

#[tokio::test]
async fn v451_v460_figure_out_what_matters_keeps_attention_distinct_from_truth() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let (status, _, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(intent_request(
                operation,
                analysis,
                "FIGURE_OUT_WHAT_MATTERS",
            )),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["operation"]["state"], "SUCCEEDED");

    let plan = agentic_plan(&s, operation).await;
    let synthesis = step(&plan, "SYNTHESIZE").unwrap()["label"]
        .as_str()
        .unwrap();
    assert!(synthesis.contains("what appears material"));
    assert!(synthesis.contains("attention-worthy unresolved questions"));
    assert!(step(&plan, "REQUEST_HUMAN_DECISION").is_none());
}

#[tokio::test]
async fn v461_v470_human_intent_is_durable_input_with_non_execution_boundary() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = intent_request(operation, analysis, "ABOUT_TO_ACT_OR_SAY");
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(request),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let raw: String = sqlx::query_scalar(
        "SELECT input_json FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(operation)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let input: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(input["human_intent"], "ABOUT_TO_ACT_OR_SAY");
    assert_eq!(input["human_owns_project"], true);
    assert_eq!(input["automatic_external_action_allowed"], false);
}

#[tokio::test]
async fn v471_v480_exact_intent_replay_reuses_operation_and_plan() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = intent_request(operation, analysis, "ABOUT_TO_ACT_OR_SAY");

    for expected_replay in [false, true] {
        let (status, _, value) = invoke(
            s.router.clone(),
            req(
                "POST",
                &intent_path(&s, operation, "/sync"),
                &s.token,
                Some(request.clone()),
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["analysis_replayed"], expected_replay);
        assert_eq!(value["operation"]["operation_id"], operation.to_string());
        assert_eq!(value["operation"]["state"], "WAITING_FOR_HUMAN");
    }

    let plan = agentic_plan(&s, operation).await;
    assert_eq!(plan["plan_revision"], 1);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_plans WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(operation)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

