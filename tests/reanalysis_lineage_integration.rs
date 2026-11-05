#![cfg(feature = "pg-integration")]
//! Reanalysis is a new immutable analysis linked to the earlier one it revisits.
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
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'reanalysis-test')")
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

async fn create_situation(f: &Setup, situation: Uuid) {
    let (status, _) = call(
        f.router.clone(),
        req(
            "POST",
            situation_path(f, situation),
            &f.writer,
            Some(json!({"focus":"Why did Alice object?"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn reanalysis_uses_new_context_and_preserves_explicit_parent_lineage() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let original = Uuid::new_v4();
    let reanalysis = Uuid::new_v4();
    create_situation(&f, situation).await;

    let (status, first) = call(
        f.router.clone(),
        req(
            "POST",
            analysis_path(&f, situation, original),
            &f.writer,
            Some(json!({"goal":"Explain Alice's objection"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["input_revision"], 1);
    assert!(first["reanalysis_of"].is_null());

    let focus_path = format!("{}/focus", situation_path(&f, situation));
    let (status, changed) = call(
        f.router.clone(),
        req(
            "POST",
            focus_path,
            &f.writer,
            Some(json!({"focus":"Reconsider after the user's correction"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(changed["revision"], 2);

    let (status, second) = call(
        f.router.clone(),
        req(
            "POST",
            analysis_path(&f, situation, reanalysis),
            &f.writer,
            Some(json!({
                "goal":"Re-evaluate Alice's objection",
                "reanalysis_of": original
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["input_revision"], 2);
    assert_eq!(second["reanalysis_of"], original.to_string());

    let (_, old_again) = call(
        f.router.clone(),
        req(
            "GET",
            analysis_path(&f, situation, original),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(old_again["input_revision"], 1);
    assert!(old_again["reanalysis_of"].is_null());
}

#[tokio::test]
async fn reanalysis_retry_is_idempotent_but_parent_cannot_change() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let parent_a = Uuid::new_v4();
    let parent_b = Uuid::new_v4();
    let child = Uuid::new_v4();
    create_situation(&f, situation).await;
    for parent in [parent_a, parent_b] {
        assert_eq!(
            call(
                f.router.clone(),
                req(
                    "POST",
                    analysis_path(&f, situation, parent),
                    &f.writer,
                    Some(json!({"goal":"parent"})),
                ),
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let path = analysis_path(&f, situation, child);
    let body = json!({"goal":"child", "reanalysis_of":parent_a});
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
    assert_eq!(replay["reanalysis_of"], parent_a.to_string());

    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path,
                &f.writer,
                Some(json!({"goal":"child", "reanalysis_of":parent_b})),
            ),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn self_unknown_and_other_situation_parent_are_rejected() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let other_situation = Uuid::new_v4();
    create_situation(&f, situation).await;
    create_situation(&f, other_situation).await;

    let self_id = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path(&f, situation, self_id),
                &f.writer,
                Some(json!({"goal":"bad", "reanalysis_of":self_id})),
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
                "POST",
                analysis_path(&f, situation, Uuid::new_v4()),
                &f.writer,
                Some(json!({"goal":"missing parent", "reanalysis_of":Uuid::new_v4()})),
            ),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    let other_parent = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path(&f, other_situation, other_parent),
                &f.writer,
                Some(json!({"goal":"other parent"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path(&f, situation, Uuid::new_v4()),
                &f.writer,
                Some(json!({"goal":"wrong situation parent", "reanalysis_of":other_parent})),
            ),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn reanalysis_does_not_rewrite_historical_output() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let original = Uuid::new_v4();
    let child = Uuid::new_v4();
    create_situation(&f, situation).await;
    let original_analysis = analysis_path(&f, situation, original);
    call(
        f.router.clone(),
        req(
            "POST",
            original_analysis.clone(),
            &f.writer,
            Some(json!({"goal":"original"})),
        ),
    )
    .await;
    let original_output = format!("{original_analysis}/output");
    let (_, published) = call(
        f.router.clone(),
        req(
            "POST",
            original_output.clone(),
            &f.writer,
            Some(json!({"answer_text":"Original answer", "coverage_note":"revision 1"})),
        ),
    )
    .await;

    let focus_path = format!("{}/focus", situation_path(&f, situation));
    call(
        f.router.clone(),
        req(
            "POST",
            focus_path,
            &f.writer,
            Some(json!({"focus":"corrected focus"})),
        ),
    )
    .await;
    call(
        f.router.clone(),
        req(
            "POST",
            analysis_path(&f, situation, child),
            &f.writer,
            Some(json!({"goal":"reanalyze", "reanalysis_of":original})),
        ),
    )
    .await;

    let (status, historical) = call(
        f.router.clone(),
        req("GET", original_output, &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(historical["answer_text"], published["answer_text"]);
    assert_eq!(historical["input_revision"], 1);
    assert_eq!(historical["input_is_current"], false);
    assert_eq!(historical["historical_output_preserved"], true);
}
