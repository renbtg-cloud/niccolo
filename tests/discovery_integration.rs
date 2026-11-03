#![cfg(feature = "pg-integration")]
//! The proposal endpoint uses authenticated workspace graph data, not caller-supplied roots.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    owner: PgPool,
    w: Uuid,
    root: Uuid,
    a: Uuid,
    b: Uuid,
    token: String,
    router: Router,
}
async fn setup() -> Fixture {
    let owner = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let api_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let root = Uuid::new_v4();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'leads')")
        .bind(w)
        .execute(&owner)
        .await
        .unwrap();
    for id in [root, a, b] {
        sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'test://origin','synthetic','REPORTED_SPEECH')")
            .bind(w).bind(id).execute(&owner).await.unwrap();
    }
    for id in [a, b] {
        sqlx::query("INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) VALUES($1,$2,$3,'COPIED_FROM')")
            .bind(w).bind(id).bind(root).execute(&owner).await.unwrap();
    }
    let token = authz::provision_token(&owner, Uuid::new_v4(), w, true, false, 1)
        .await
        .unwrap();
    Fixture {
        owner,
        w,
        root,
        a,
        b,
        token,
        router: api::router(api_pool),
    }
}
fn req(w: Uuid, token: Option<&str>, reports: Vec<Uuid>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("/v1/workspaces/{w}/leads"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
        .body(Body::from(json!({"report_ids":reports}).to_string()))
        .unwrap()
}
async fn call(app: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn shared_origin_prompts_verification_without_claiming_independence() {
    let f = setup().await;
    let (status, value) = call(f.router, req(f.w, Some(&f.token), vec![f.a, f.b])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["proposals"][0]["kind"], "verify_shared_origin");
    assert_eq!(value["proposals"][0]["provenance_ids"][0], json!(f.root));
    assert_eq!(value["independence_established"], false);
    assert_eq!(value["human_review_required"], true);
    assert!(value["proposals"].as_array().unwrap().len() <= 3);
}
#[tokio::test]
async fn authorization_is_required_even_for_leads() {
    let f = setup().await;
    let (status, _) = call(f.router.clone(), req(f.w, None, vec![f.a, f.b])).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(
        f.router,
        req(Uuid::new_v4(), Some(&f.token), vec![f.a, f.b]),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn withdrawn_root_refuses_to_generate_leads() {
    let f = setup().await;
    sqlx::query(
        "UPDATE niccolo.evidence SET admissible=false WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(f.w)
    .bind(f.root)
    .execute(&f.owner)
    .await
    .unwrap();
    let (status, _) = call(f.router, req(f.w, Some(&f.token), vec![f.a, f.b])).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
#[tokio::test]
async fn invalid_or_invisible_report_ids_are_not_accepted() {
    let f = setup().await;
    for (ids, status) in [
        (vec![], StatusCode::BAD_REQUEST),
        (vec![f.a, f.a], StatusCode::BAD_REQUEST),
        (vec![Uuid::new_v4()], StatusCode::NOT_FOUND),
    ] {
        let (actual, _) = call(f.router.clone(), req(f.w, Some(&f.token), ids)).await;
        assert_eq!(actual, status);
    }
}
