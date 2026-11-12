#![cfg(feature = "pg-integration")]
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
    reader: String,
}

async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'analysis-history-test')",
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

fn history_path(f: &Setup, situation: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-history",
        f.workspace
    )
}

async fn create_situation(f: &Setup, situation: Uuid) {
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                situation_path(f, situation),
                &f.writer,
                Some(json!({"focus":"Initial focus"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn history_exposes_reanalysis_lineage_outputs_and_currentness() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    create_situation(&f, situation).await;

    call(
        f.router.clone(),
        req(
            "POST",
            analysis_path(&f, situation, first),
            &f.writer,
            Some(json!({"goal":"first pass"})),
        ),
    )
    .await;
    call(
        f.router.clone(),
        req(
            "POST",
            format!("{}/output", analysis_path(&f, situation, first)),
            &f.writer,
            Some(json!({"answer_text":"old answer","coverage_note":"revision 1"})),
        ),
    )
    .await;
    call(
        f.router.clone(),
        req(
            "POST",
            format!("{}/focus", situation_path(&f, situation)),
            &f.writer,
            Some(json!({"focus":"Corrected focus"})),
        ),
    )
    .await;
    call(
        f.router.clone(),
        req(
            "POST",
            analysis_path(&f, situation, second),
            &f.writer,
            Some(json!({"goal":"second pass","reanalysis_of":first})),
        ),
    )
    .await;

    let (status, body) = call(
        f.router.clone(),
        req("GET", history_path(&f, situation), &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let analyses = body["analyses"].as_array().unwrap();
    assert_eq!(analyses.len(), 2);
    assert_eq!(analyses[0]["analysis_id"], second.to_string());
    assert_eq!(analyses[0]["reanalysis_of"], first.to_string());
    assert_eq!(analyses[0]["input_is_current"], true);
    assert_eq!(analyses[0]["imported_context_count"], 0);
    assert!(analyses[0]["answer_text"].is_null());
    assert_eq!(analyses[1]["analysis_id"], first.to_string());
    assert_eq!(analyses[1]["answer_text"], "old answer");
    assert_eq!(analyses[1]["imported_context_count"], 0);
    assert_eq!(analyses[1]["input_is_current"], false);
    assert_eq!(analyses[1]["historical_output_preserved"], true);
}

#[tokio::test]
async fn history_is_cursor_paginated_without_duplicates() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create_situation(&f, situation).await;
    for n in 0..3 {
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path(&f, situation, Uuid::new_v4()),
                &f.writer,
                Some(json!({"goal":format!("pass {n}")})),
            ),
        )
        .await;
    }

    let (_, first_page) = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}?limit=2", history_path(&f, situation)),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(first_page["analyses"].as_array().unwrap().len(), 2);
    let cursor = first_page["next_before"].as_str().unwrap();
    let (_, second_page) = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}?limit=2&before={cursor}", history_path(&f, situation)),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(second_page["analyses"].as_array().unwrap().len(), 1);
    let first_ids = first_page["analyses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["analysis_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!first_ids.contains(&second_page["analyses"][0]["analysis_id"].as_str().unwrap()));
    assert!(second_page["next_before"].is_null());
}

#[tokio::test]
async fn history_rejects_invalid_limits_and_invisible_cursors() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create_situation(&f, situation).await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "GET",
                format!("{}?limit=0", history_path(&f, situation)),
                &f.reader,
                None,
            ),
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "GET",
                format!("{}?before={}", history_path(&f, situation), Uuid::new_v4()),
                &f.reader,
                None,
            ),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn history_requires_read_authorization() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (status, _) = call(
        f.router.clone(),
        Request::builder()
            .method("GET")
            .uri(history_path(&f, situation))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
