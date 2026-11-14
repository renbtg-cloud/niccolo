#![cfg(feature = "pg-integration")]
//! Analysis snapshots freeze the Situation revision seen by an in-flight reasoning step.
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
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'analysis-snapshot-test')",
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

async fn create_situation(f: &Setup, situation: Uuid) {
    let path = format!("/v1/workspaces/{}/situations/{situation}", f.workspace);
    let (status, _) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            Some(&f.writer),
            Some(json!({"focus":"Why did Alice object?"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

fn snapshot_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}",
        f.workspace, situation, analysis
    )
}

#[tokio::test]
async fn later_human_context_never_changes_frozen_analysis_input() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let path = snapshot_path(&f, situation, analysis);
    let (status, frozen) = call(
        f.router.clone(),
        req(
            "POST",
            path.clone(),
            Some(&f.writer),
            Some(json!({"goal":"Explain Alice's objection"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(frozen["input_revision"], 1);
    assert_eq!(frozen["frozen_input"], true);
    assert_eq!(frozen["incorporates_future_context"], false);
    let fingerprint = frozen["input_fingerprint"].clone();

    let contribution = Uuid::new_v4();
    let contribution_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, contribution
    );
    let (status, current) = call(
        f.router.clone(),
        req(
            "POST",
            contribution_path,
            Some(&f.writer),
            Some(json!({
                "kind":"user_reported",
                "text":"Alice and the director had agreed the previous day",
                "related_evidence_id":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(current["revision"], 2);

    let (status, still_frozen) =
        call(f.router.clone(), req("GET", path, Some(&f.writer), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_frozen["input_revision"], 1);
    assert_eq!(still_frozen["input_fingerprint"], fingerprint);
}

#[tokio::test]
async fn retry_after_situation_changes_returns_original_snapshot_not_new_revision() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let path = snapshot_path(&f, situation, analysis);
    let (_, first) = call(
        f.router.clone(),
        req(
            "POST",
            path.clone(),
            Some(&f.writer),
            Some(json!({"goal":"Check timing"})),
        ),
    )
    .await;
    let focus_path = format!(
        "/v1/workspaces/{}/situations/{}/focus",
        f.workspace, situation
    );
    call(
        f.router.clone(),
        req(
            "POST",
            focus_path,
            Some(&f.writer),
            Some(json!({"focus":"Check attribution instead"})),
        ),
    )
    .await;
    let (status, replay) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            Some(&f.writer),
            Some(json!({"goal":"Check timing"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["input_revision"], 1);
    assert_eq!(replay["input_fingerprint"], first["input_fingerprint"]);
}

#[tokio::test]
async fn same_analysis_id_cannot_be_rewritten_with_different_goal_or_actor() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let path = snapshot_path(&f, situation, analysis);
    call(
        f.router.clone(),
        req(
            "POST",
            path.clone(),
            Some(&f.writer),
            Some(json!({"goal":"Original goal"})),
        ),
    )
    .await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path.clone(),
                Some(&f.writer),
                Some(json!({"goal":"Different goal"}))
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path,
                Some(&f.second_writer),
                Some(json!({"goal":"Original goal"}))
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn read_only_member_can_read_snapshot_but_cannot_start_one() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let path = snapshot_path(&f, situation, analysis);
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path.clone(),
                Some(&f.reader),
                Some(json!({"goal":"Read-only cannot start"}))
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    call(
        f.router.clone(),
        req(
            "POST",
            path.clone(),
            Some(&f.writer),
            Some(json!({"goal":"Writer starts"})),
        ),
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
async fn different_analysis_ids_cannot_create_two_snapshots_of_same_revision() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create_situation(&f, situation).await;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let (a, b) = tokio::join!(
        call(
            f.router.clone(),
            req(
                "POST",
                snapshot_path(&f, situation, first),
                Some(&f.writer),
                Some(json!({"goal":"One revision"}))
            )
        ),
        call(
            f.router.clone(),
            req(
                "POST",
                snapshot_path(&f, situation, second),
                Some(&f.writer),
                Some(json!({"goal":"One revision"}))
            )
        ),
    );
    let codes = [a.0, b.0];
    assert_eq!(
        codes.iter().filter(|c| **c == StatusCode::CREATED).count(),
        1
    );
    assert_eq!(
        codes.iter().filter(|c| **c == StatusCode::CONFLICT).count(),
        1
    );
    assert!(!codes.contains(&StatusCode::INTERNAL_SERVER_ERROR));
}

#[tokio::test]
async fn concurrent_identical_starts_create_one_immutable_snapshot() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let path = snapshot_path(&f, situation, analysis);
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let router = f.router.clone();
        let token = f.writer.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            call(
                router,
                req(
                    "POST",
                    path,
                    Some(&token),
                    Some(json!({"goal":"One frozen input"})),
                ),
            )
            .await
        }));
    }
    let mut created = 0;
    let mut replayed = 0;
    let mut fingerprint = None;
    for task in tasks {
        let (status, value) = task.await.unwrap();
        match status {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => replayed += 1,
            other => panic!("unexpected {other}"),
        }
        assert_eq!(value["input_revision"], 1);
        let fp = value["input_fingerprint"].as_str().unwrap().to_owned();
        if let Some(expected) = &fingerprint {
            assert_eq!(expected, &fp);
        } else {
            fingerprint = Some(fp);
        }
    }
    assert_eq!(created, 1);
    assert_eq!(replayed, 15);
}

#[tokio::test]
async fn materialized_input_is_the_exact_frozen_revision_not_current_context() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;

    let contribution = Uuid::new_v4();
    let contribution_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, contribution
    );
    let (status, before) = call(
        f.router.clone(),
        req(
            "POST",
            contribution_path,
            Some(&f.writer),
            Some(json!({
                "kind":"hypothetical",
                "text":"WHAT IF Roberto never saw the email?",
                "related_evidence_id":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(before["revision"], 2);

    let analysis_path = snapshot_path(&f, situation, analysis);
    let (status, frozen) = call(
        f.router.clone(),
        req(
            "POST",
            analysis_path.clone(),
            Some(&f.writer),
            Some(json!({"goal":"Evaluate the WHAT IF without treating it as fact"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(frozen["input_revision"], 2);

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
            Some(json!({"focus":"A later, different question"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["revision"], 3);

    let input_path = format!("{analysis_path}/input");
    let (status, materialized) = call(
        f.router.clone(),
        req("GET", input_path, Some(&f.reader), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(materialized["analysis"]["input_revision"], 2);
    assert_eq!(materialized["input"]["revision"], 2);
    assert_eq!(materialized["current_situation_revision"], 3);
    assert_eq!(materialized["input_is_current"], false);
    assert_eq!(materialized["future_context_included"], false);
    assert_eq!(
        materialized["input"]["contributions"][0]["kind"],
        "HYPOTHETICAL"
    );
    assert_eq!(
        materialized["input"]["contributions"][0]["is_verified_evidence"],
        false
    );
    assert_ne!(materialized["input"]["focus"], current["focus"]);
    assert_eq!(
        materialized["input"]["fingerprint"],
        materialized["analysis"]["input_fingerprint"]
    );
}
