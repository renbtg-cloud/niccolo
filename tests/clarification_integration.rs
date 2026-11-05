#![cfg(feature = "pg-integration")]
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

struct Setup {
    admin: PgPool,
    router: Router,
    workspace: Uuid,
    writer: String,
    reader: String,
}

async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(16)
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
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'clarification-test')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Setup {
        admin,
        router: api::router(secure),
        workspace,
        writer,
        reader,
    }
}

fn req(method: &str, path: String, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}

async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}

fn situation_path(f: &Setup, situation: Uuid) -> String {
    format!("/v1/workspaces/{}/situations/{situation}", f.workspace)
}

fn analysis_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    )
}

fn clarifications_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!("{}/clarifications", analysis_path(f, situation, analysis))
}

async fn create_situation(f: &Setup, situation: Uuid) {
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                situation_path(f, situation),
                &f.writer,
                Some(json!({"focus":"Understand the disagreement"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

async fn begin_analysis(f: &Setup, situation: Uuid, analysis: Uuid) {
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path(f, situation, analysis),
                &f.writer,
                Some(json!({"goal":"understand what matters"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

fn clarification_body(text: &str, reason: &str) -> Value {
    json!({"text":text,"reason":reason})
}

#[tokio::test]
async fn clarification_keeps_the_frozen_input_that_motivated_it() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let question = Uuid::new_v4();
    create_situation(&f, situation).await;
    begin_analysis(&f, situation, analysis).await;
    let path = format!(
        "{}/{}",
        clarifications_path(&f, situation, analysis),
        question
    );
    let (status, created) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            &f.writer,
            Some(clarification_body(
                "Had Alice already seen the final decision?",
                "The answer would distinguish anticipation from prior knowledge.",
            )),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["input_is_current"], true);
    assert_eq!(created["is_evidence"], false);
    assert_eq!(created["purpose"], "TARGETED_CLARIFICATION");

    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/focus", situation_path(&f, situation)),
                &f.writer,
                Some(json!({"focus":"Now inspect the Tuesday meeting"})),
            ),
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, listed) = call(
        f.router.clone(),
        req(
            "GET",
            clarifications_path(&f, situation, analysis),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        listed["clarifications"][0]["question_id"],
        question.to_string()
    );
    assert_eq!(listed["clarifications"][0]["input_revision"], 1);
    assert_eq!(listed["clarifications"][0]["current_situation_revision"], 2);
    assert_eq!(listed["clarifications"][0]["input_is_current"], false);
    assert_eq!(
        listed["later_context_does_not_rewrite_question_origin"],
        true
    );
}

#[tokio::test]
async fn clarification_retry_is_idempotent_but_text_or_reason_cannot_mutate() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let question = Uuid::new_v4();
    create_situation(&f, situation).await;
    begin_analysis(&f, situation, analysis).await;
    let path = format!(
        "{}/{}",
        clarifications_path(&f, situation, analysis),
        question
    );
    let body = clarification_body(
        "Did Roberto receive the email?",
        "This changes the exposure timeline.",
    );
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path.clone(), &f.writer, Some(body.clone()))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, replay) = call(
        f.router.clone(),
        req("POST", path.clone(), &f.writer, Some(body)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path.clone(),
                &f.writer,
                Some(clarification_body(
                    "Did Roberto receive the email?",
                    "A different reason cannot rewrite history.",
                )),
            ),
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
                &f.writer,
                Some(clarification_body(
                    "Did Roberto read the email?",
                    "This changes the exposure timeline.",
                )),
            ),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn clarification_list_is_bounded_and_reader_cannot_propose() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    begin_analysis(&f, situation, analysis).await;
    let base = clarifications_path(&f, situation, analysis);
    for n in 0..3 {
        let question = Uuid::new_v4();
        assert_eq!(
            call(
                f.router.clone(),
                req(
                    "POST",
                    format!("{base}/{question}"),
                    &f.writer,
                    Some(clarification_body(
                        &format!("Question {n}?"),
                        "It could materially distinguish two explanations.",
                    )),
                ),
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let (_, first) = call(
        f.router.clone(),
        req("GET", format!("{base}?limit=2"), &f.reader, None),
    )
    .await;
    assert_eq!(first["clarifications"].as_array().unwrap().len(), 2);
    let cursor = first["next_after"].as_str().unwrap();
    let (_, second) = call(
        f.router.clone(),
        req(
            "GET",
            format!("{base}?limit=2&after={cursor}"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(second["clarifications"].as_array().unwrap().len(), 1);
    assert!(second["next_after"].is_null());
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{base}/{}", Uuid::new_v4()),
                &f.reader,
                Some(clarification_body("Reader write?", "Must be denied.")),
            ),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            f.router.clone(),
            req("GET", format!("{base}?limit=0"), &f.reader, None),
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn unknown_analysis_does_not_create_an_orphan_question() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let question = Uuid::new_v4();
    create_situation(&f, situation).await;
    let status = call(
        f.router.clone(),
        req(
            "POST",
            format!(
                "{}/{}",
                clarifications_path(&f, situation, analysis),
                question
            ),
            &f.writer,
            Some(clarification_body("Orphan?", "Should never persist.")),
        ),
    )
    .await
    .0;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2",
    )
    .bind(f.workspace)
    .bind(question)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_identical_clarification_creates_one_binding() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let question = Uuid::new_v4();
    create_situation(&f, situation).await;
    begin_analysis(&f, situation, analysis).await;
    let path = format!(
        "{}/{}",
        clarifications_path(&f, situation, analysis),
        question
    );
    let body = clarification_body(
        "Was Alice present when the decision changed?",
        "Presence would materially change the plausible knowledge timeline.",
    );
    let mut tasks = Vec::new();
    for _ in 0..24 {
        let router = f.router.clone();
        let token = f.writer.clone();
        let path = path.clone();
        let body = body.clone();
        tasks.push(tokio::spawn(async move {
            call(router, req("POST", path, &token, Some(body))).await.0
        }));
    }
    let mut created = 0;
    for task in tasks {
        match task.await.unwrap() {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => {}
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(created, 1);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_clarification_questions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND question_id=$4",
    )
    .bind(f.workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
