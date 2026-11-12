#![cfg(feature = "pg-integration")]
use axum::{Router, body::Body, http::{Request, StatusCode, header}};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    router: Router,
    admin: sqlx::PgPool,
    workspace: Uuid,
    writer: String,
    reader: String,
}

async fn setup() -> Fixture {
    let admin = PgPoolOptions::new().max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let secure = PgPoolOptions::new().max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'git-publication')")
        .bind(workspace).execute(&admin).await.unwrap();
    let writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1).await.unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1).await.unwrap();
    Fixture { router: api::router(secure), admin, workspace, writer, reader }
}

async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

fn request(method: &str, uri: String, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder().method(method).uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string()))).unwrap()
}

async fn published_analysis(f: &Fixture) -> (Uuid, Uuid) {
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let base = format!("/v1/workspaces/{}/situations/{situation}", f.workspace);
    assert_eq!(call(f.router.clone(), request("POST", base.clone(), &f.writer,
        Some(json!({"focus":"Plan portable publication without claiming it happened"})))).await.0, StatusCode::CREATED);
    let analysis_path = format!("{base}/analysis-snapshots/{analysis}");
    assert_eq!(call(f.router.clone(), request("POST", analysis_path.clone(), &f.writer,
        Some(json!({"goal":"Produce a portable published analysis"})))).await.0, StatusCode::CREATED);
    assert_eq!(call(f.router.clone(), request("POST", format!("{analysis_path}/output"), &f.writer,
        Some(json!({"answer_text":"Recorded answer","coverage_note":"Bounded fixture"})))).await.0, StatusCode::CREATED);
    (situation, analysis)
}

#[tokio::test]
async fn publication_planning_is_explicit_idempotent_and_never_claims_remote_success() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();

    let target_uri = format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace);
    let target_body = json!({
        "repository_owner":"example-owner",
        "repository_name":"niccolo-history",
        "ref_name":"main",
        "repository_visibility":"PRIVATE",
        "sensitive_exports_allowed":false
    });
    let (status, registered) = call(f.router.clone(), request("POST", target_uri.clone(), &f.writer, Some(target_body.clone()))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(registered["provider_connection_verified"], false);
    assert_eq!(registered["provider_secret_stored"], false);
    assert_eq!(registered["registration_publishes_nothing"], true);

    let (status, replay) = call(f.router.clone(), request("POST", target_uri, &f.writer, Some(target_body))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let analysis_path = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    let (status, preview) = call(
        f.router.clone(),
        request("GET", format!("{analysis_path}/git-publication-preview?target_id={target}"), &f.reader, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["scope_kind"], "STANDARD_PRESENTATION");
    assert_eq!(preview["includes_examined_source_text"], false);
    assert_eq!(preview["includes_original_uploads"], false);
    assert_eq!(preview["remote_publish_performed"], false);
    assert_eq!(preview["requires_transient_provider_authorization"], true);
    assert_eq!(preview["ready_for_remote_publish"], false);
    assert_eq!(preview["package_sha256"].as_str().unwrap().len(), 64);
    assert!(preview["file_count"].as_u64().unwrap() > 0);
    assert!(preview["total_text_bytes"].as_u64().unwrap() > 0);

    let intent_uri = format!("{analysis_path}/git-publication-intents/{publication}");
    let (status, recorded) = call(
        f.router.clone(),
        request("POST", intent_uri.clone(), &f.writer, Some(json!({"target_id":target}))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recorded["remote_publish_performed"], false);
    assert_eq!(recorded["requires_provider_authorization_before_publish"], true);
    assert_eq!(recorded["intent"]["publication_state"], "AWAITING_REMOTE_AUTHORIZATION");
    assert_eq!(recorded["intent"]["package_sha256"], preview["package_sha256"]);

    let (status, replayed) = call(
        f.router.clone(),
        request("POST", intent_uri, &f.writer, Some(json!({"target_id":target}))),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);

    let (status, listed) = call(
        f.router.clone(),
        request("GET", format!("{analysis_path}/git-publication-intents?limit=25"), &f.reader, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["remote_publish_performed"], false);
    assert_eq!(listed["intents"].as_array().unwrap().len(), 1);
    assert_eq!(listed["intents"][0]["publication_id"], publication.to_string());
}

#[tokio::test]
async fn target_registration_rejects_urls_and_public_sensitive_scope() {
    let f = setup().await;
    let target = Uuid::new_v4();
    let uri = format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace);
    for body in [
        json!({"repository_owner":"https://example.com","repository_name":"repo","ref_name":"main","repository_visibility":"PRIVATE","sensitive_exports_allowed":false}),
        json!({"repository_owner":"owner","repository_name":"repo","ref_name":"main","repository_visibility":"PUBLIC","sensitive_exports_allowed":true}),
        json!({"repository_owner":"owner","repository_name":"../repo","ref_name":"main","repository_visibility":"PRIVATE","sensitive_exports_allowed":false}),
    ] {
        assert_eq!(call(f.router.clone(), request("POST", uri.clone(), &f.writer, Some(body))).await.0, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn reader_cannot_create_targets_or_publication_intents() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let uri = format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace);
    assert_eq!(
        call(f.router.clone(), request("POST", uri, &f.reader, Some(json!({
            "repository_owner":"owner","repository_name":"repo","ref_name":"main",
            "repository_visibility":"PRIVATE","sensitive_exports_allowed":false
        })))).await.0,
        StatusCode::FORBIDDEN
    );
    let intent = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{intent}",
        f.workspace
    );
    assert_eq!(
        call(f.router.clone(), request("POST", uri, &f.reader, Some(json!({"target_id":target})))).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn write_only_credential_cannot_publish_an_analysis_it_cannot_read() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let target_uri = format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace);
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                target_uri,
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"history",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                })),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let write_only = authz::provision_token(&f.admin, Uuid::new_v4(), f.workspace, false, true, 1)
        .await
        .unwrap();
    let publication = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{publication}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request("POST", uri, &write_only, Some(json!({"target_id":target}))),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn publication_metadata_is_workspace_isolated() {
    let f = setup().await;
    let target = Uuid::new_v4();
    let uri = format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace);
    assert_eq!(call(f.router.clone(), request("POST", uri, &f.writer, Some(json!({
        "repository_owner":"owner","repository_name":"repo","ref_name":"main",
        "repository_visibility":"PRIVATE","sensitive_exports_allowed":false
    })))).await.0, StatusCode::CREATED);

    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'other-git-workspace')")
        .bind(other).execute(&f.admin).await.unwrap();
    let other_reader = authz::provision_token(&f.admin, Uuid::new_v4(), other, true, false, 1).await.unwrap();
    let (status, body) = call(
        f.router.clone(),
        request("GET", format!("/v1/workspaces/{other}/git-publication-targets"), &other_reader, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["targets"].as_array().unwrap().is_empty());
}
