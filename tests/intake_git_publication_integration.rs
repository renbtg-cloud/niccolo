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

struct Fixture {
    router: Router,
    workspace: Uuid,
    writer: String,
    reader: String,
}

async fn setup() -> Fixture {
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
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'intake-git')")
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
    Fixture {
        router: api::router(secure),
        workspace,
        writer,
        reader,
    }
}

fn request(
    method: &str,
    uri: String,
    token: &str,
    content_type: Option<&str>,
    locator: Option<&str>,
    body: Body,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(locator) = locator {
        builder = builder.header("x-source-locator", locator);
    }
    builder.body(body).unwrap()
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

async fn register_target(f: &Fixture, target: Uuid, repository: &str) {
    let uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    let body = json!({
        "repository_owner":"example-owner",
        "repository_name":repository,
        "ref_name":"intake",
        "repository_visibility":"PRIVATE",
        "sensitive_exports_allowed":false
    });
    let (status, _) = call(
        f.router.clone(),
        request(
            "POST",
            uri,
            &f.writer,
            Some("application/json"),
            None,
            Body::from(body.to_string()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn ingest(f: &Fixture, document: Uuid) {
    let uri = format!("/v1/workspaces/{}/sources/{document}", f.workspace);
    let raw = "[12/11/2026, 10:00] Ana: private source text\n";
    let (status, _) = call(
        f.router.clone(),
        request(
            "POST",
            uri,
            &f.writer,
            Some("text/plain"),
            Some("fixture://private/intake.txt"),
            Body::from(raw),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn intake_publication_planning_is_received_only_idempotent_and_not_remote_success() {
    let f = setup().await;
    let document = Uuid::new_v4();
    let target = Uuid::new_v4();
    let other_target = Uuid::new_v4();
    let publication = Uuid::new_v4();
    ingest(&f, document).await;
    register_target(&f, target, "history").await;
    register_target(&f, other_target, "history-2").await;

    let base = format!("/v1/workspaces/{}/sources/{document}", f.workspace);
    let (status, preview) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{base}/git-publication-preview?target_id={target}"),
            &f.reader,
            None,
            None,
            Body::empty(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["scope_kind"], "INTAKE_RECEIPT");
    assert_eq!(preview["includes_original_uploads"], false);
    assert_eq!(preview["includes_source_text"], false);
    assert_eq!(preview["includes_analysis"], false);
    assert_eq!(preview["remote_publish_performed"], false);
    assert_eq!(preview["requires_transient_provider_authorization"], true);
    assert_eq!(preview["establishes_authenticity"], false);
    assert_eq!(preview["establishes_truth"], false);
    assert_eq!(preview["proves_completeness"], false);

    let intent_uri = format!("{base}/git-publication-intents/{publication}");
    let body = json!({"target_id":target});
    let (status, created) = call(
        f.router.clone(),
        request(
            "POST",
            intent_uri.clone(),
            &f.writer,
            Some("application/json"),
            None,
            Body::from(body.to_string()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["intent"]["scope_kind"], "INTAKE_RECEIPT");
    assert_eq!(
        created["intent"]["package_sha256"],
        preview["package_sha256"]
    );
    assert_eq!(created["remote_publish_performed"], false);
    assert_eq!(created["source_state_preserved"], "RECEIVED");

    let (status, replay) = call(
        f.router.clone(),
        request(
            "POST",
            intent_uri.clone(),
            &f.writer,
            Some("application/json"),
            None,
            Body::from(body.to_string()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        replay["intent"]["package_sha256"],
        preview["package_sha256"]
    );

    let (status, _) = call(
        f.router.clone(),
        request(
            "POST",
            intent_uri.clone(),
            &f.writer,
            Some("application/json"),
            None,
            Body::from(json!({"target_id":other_target}).to_string()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, remote) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{intent_uri}/remote-status"),
            &f.reader,
            None,
            None,
            Body::empty(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(remote["state"], "AWAITING_REMOTE_AUTHORIZATION");
    assert_eq!(remote["remote_publish_verified"], false);
    assert_eq!(remote["event_count"], 0);

    let (status, events) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{intent_uri}/remote-events"),
            &f.reader,
            None,
            None,
            Body::empty(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events["events"].as_array().unwrap().len(), 0);
    assert_eq!(events["source_state_preserved"], "RECEIVED");

    let (status, _) = call(
        f.router,
        request(
            "POST",
            format!("{intent_uri}/publish"),
            &f.writer,
            Some("application/json"),
            None,
            Body::from(
                json!({
                    "attempt_id":Uuid::new_v4(),
                    "confirmation":"PUBLISH_INTAKE_RECEIPT_TO_GITHUB"
                })
                .to_string(),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
