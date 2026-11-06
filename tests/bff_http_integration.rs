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
