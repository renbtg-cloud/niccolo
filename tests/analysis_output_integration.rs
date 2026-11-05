#![cfg(feature = "pg-integration")]
//! Completed answers are immutable historical artifacts bound to frozen input.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

struct Setup {
    router: Router,
    workspace: Uuid,
    writer: String,
    second_writer: String,
    reader: String,
}

async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'analysis-output-test')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1)
        .await
        .unwrap();
    let second_writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Setup {
        router: api::router(secure),
        workspace,
        writer,
        second_writer,
        reader,
    }
}

fn req(method: &str, path: String, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}

async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}

async fn create_and_begin(f: &Setup, situation: Uuid, analysis: Uuid) {
    let create = format!("/v1/workspaces/{}/situations/{situation}", f.workspace);
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                create,
                Some(&f.writer),
                Some(json!({"focus":"Why did Alice object?"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let analysis_path = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}",
        f.workspace, situation, analysis
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path,
                Some(&f.writer),
                Some(json!({"goal":"Explain the objection with current evidence"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

fn output_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/output",
        f.workspace, situation, analysis
    )
}

fn output_body() -> Value {
    json!({
        "answer_text":"The current evidence supports several plausible explanations; the record is incomplete.",
        "coverage_note":"Used Situation revision 1 only; no later user context was examined."
    })
}

#[tokio::test]
async fn historical_answer_survives_later_context_without_retroactive_rewrite() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_and_begin(&f, situation, analysis).await;
    let path = output_path(&f, situation, analysis);
    let (status, published) = call(
        f.router.clone(),
        req("POST", path.clone(), Some(&f.writer), Some(output_body())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(published["input_revision"], 1);
    assert_eq!(published["input_is_current"], true);
    let original_answer = published["answer_text"].clone();

    let focus_path = format!(
        "/v1/workspaces/{}/situations/{}/focus",
        f.workspace, situation
    );
    let (status, current) = call(
        f.router.clone(),
        req(
            "POST",
            focus_path,
            Some(&f.writer),
            Some(json!({"focus":"A later question after a human correction"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["revision"], 2);

    let (status, historical) =
        call(f.router.clone(), req("GET", path, Some(&f.reader), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(historical["answer_text"], original_answer);
    assert_eq!(historical["input_revision"], 1);
    assert_eq!(historical["current_situation_revision"], 2);
    assert_eq!(historical["input_is_current"], false);
    assert_eq!(historical["historical_output_preserved"], true);
}

#[tokio::test]
async fn exact_retry_is_idempotent_but_answer_or_actor_change_is_conflict() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_and_begin(&f, situation, analysis).await;
    let path = output_path(&f, situation, analysis);
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path.clone(), Some(&f.writer), Some(output_body()))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, replay) = call(
        f.router.clone(),
        req("POST", path.clone(), Some(&f.writer), Some(output_body())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let mut changed = output_body();
    changed["answer_text"] = json!("Different answer must be a new analysis, not a rewrite.");
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path.clone(), Some(&f.writer), Some(changed))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path, Some(&f.second_writer), Some(output_body()))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn read_only_member_can_read_but_cannot_publish_output() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_and_begin(&f, situation, analysis).await;
    let path = output_path(&f, situation, analysis);
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path.clone(), Some(&f.reader), Some(output_body()))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    call(
        f.router.clone(),
        req("POST", path.clone(), Some(&f.writer), Some(output_body())),
    )
    .await;
    assert_eq!(
        call(f.router.clone(), req("GET", path, Some(&f.reader), None))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn output_requires_a_visible_frozen_analysis_snapshot() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let create = format!("/v1/workspaces/{}/situations/{situation}", f.workspace);
    call(
        f.router.clone(),
        req(
            "POST",
            create,
            Some(&f.writer),
            Some(json!({"focus":"Question"})),
        ),
    )
    .await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                output_path(&f, situation, analysis),
                Some(&f.writer),
                Some(output_body())
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn concurrent_identical_publish_creates_one_immutable_answer() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_and_begin(&f, situation, analysis).await;
    let path = output_path(&f, situation, analysis);
    let mut tasks = Vec::new();
    for _ in 0..24 {
        let router = f.router.clone();
        let token = f.writer.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            call(router, req("POST", path, Some(&token), Some(output_body()))).await
        }));
    }
    let mut created = 0;
    let mut replayed = 0;
    for task in tasks {
        let (status, value) = task.await.unwrap();
        match status {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => {
                replayed += 1;
                assert_eq!(value["replayed"], true);
            }
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(created, 1);
    assert_eq!(replayed, 23);
}
