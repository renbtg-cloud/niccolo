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
    situation: Uuid,
    token: String,
}

async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'artifact-http')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    let router = api::router(service);
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &format!("/v1/workspaces/{workspace}/situations/{situation}"),
            &token,
            Some(json!({"focus":"Compare explicitly supplied descriptions"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    Setup {
        router,
        workspace,
        situation,
        token,
    }
}

fn request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let bytes = body.map_or_else(Vec::new, |value| serde_json::to_vec(&value).unwrap());
    if !bytes.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder.body(Body::from(bytes)).unwrap()
}

async fn invoke(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn http_flow_records_human_material_without_accepting_or_reading_artifact_bytes() {
    let setup = setup().await;
    let video = Uuid::new_v4();
    let transcript = Uuid::new_v4();
    let artifact_url = format!(
        "/v1/workspaces/{}/situations/{}/artifacts/{video}",
        setup.workspace, setup.situation
    );
    let artifact_body = json!({
        "artifact_type":"VIDEO",
        "label":"meeting.mp4",
        "declared_mime_type":"video/mp4",
        "declared_byte_length":18000000000_i64,
        "declared_sha256":null
    });
    let (status, created) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &artifact_url,
            &setup.token,
            Some(artifact_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["source_bytes_stored"], false);
    assert_eq!(created["artifact_was_inspected"], false);
    assert_eq!(created["parsers_invoked"], false);
    let (status, replay) = invoke(
        setup.router.clone(),
        request("POST", &artifact_url, &setup.token, Some(artifact_body)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let transcript_url = format!(
        "/v1/workspaces/{}/situations/{}/artifacts/{transcript}",
        setup.workspace, setup.situation
    );
    let (status, _) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &transcript_url,
            &setup.token,
            Some(json!({
                "artifact_type":"TRANSCRIPT_FILE",
                "label":"meeting-transcript.md",
                "declared_mime_type":"text/markdown",
                "declared_byte_length":null,
                "declared_sha256":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let reference = Uuid::new_v4();
    let storage_url = format!("{artifact_url}/storage-references/{reference}");
    let (status, storage) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &storage_url,
            &setup.token,
            Some(json!({
                "access_event_id":Uuid::new_v4(),
                "storage_type":"S3",
                "locator":"s3://client-video/meeting.mp4",
                "provider":"AWS",
                "environment_label":"client production",
                "resolver_profile":null,
                "notes":"declared by the user",
                "access_status":"NONE",
                "access_note":"Niccolò has no access"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(storage["locator_was_dereferenced"], false);
    assert_eq!(storage["access_status_is_human_declared"], true);

    let representation = Uuid::new_v4();
    let (status, represented) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &format!("{artifact_url}/representations/{representation}"),
            &setup.token,
            Some(json!({
                "representation_type":"HUMAN_DESCRIPTION",
                "producer_type":"USER",
                "producer_label":"Renato",
                "production_method":"described while watching externally",
                "scope_locator":"00:13:20-00:13:45",
                "content":"João appears to leave the room.",
                "completeness_status":"PARTIAL",
                "epistemic_qualification":"human report",
                "limitations":"bounded interval"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(represented["content_was_explicitly_supplied"], true);
    assert_eq!(represented["artifact_was_parsed"], false);
    assert_eq!(represented["representation_establishes_truth"], false);

    let relationship = Uuid::new_v4();
    let (status, related) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &format!(
                "/v1/workspaces/{}/situations/{}/artifact-relationships/{relationship}",
                setup.workspace, setup.situation
            ),
            &setup.token,
            Some(json!({
                "subject_artifact_id":transcript,
                "object_artifact_id":video,
                "relationship_type":"CLAIMED_TO_BE_TRANSCRIPTION_OF",
                "qualification":"declared by the user"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(related["relationship_is_verified"], false);

    let (status, list) = invoke(
        setup.router,
        request(
            "GET",
            &format!(
                "/v1/workspaces/{}/situations/{}/artifacts",
                setup.workspace, setup.situation
            ),
            &setup.token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(list["parser_free_mvp"], true);
    assert_eq!(list["source_bytes_accepted"], false);
    assert_eq!(list["locators_dereferenced"], false);
}

#[tokio::test]
async fn durable_presigned_credentials_are_rejected_at_the_http_boundary() {
    let setup = setup().await;
    let artifact = Uuid::new_v4();
    let artifact_url = format!(
        "/v1/workspaces/{}/situations/{}/artifacts/{artifact}",
        setup.workspace, setup.situation
    );
    let (status, _) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &artifact_url,
            &setup.token,
            Some(json!({
                "artifact_type":"DOCUMENT",
                "label":"private.pdf",
                "declared_mime_type":null,
                "declared_byte_length":null,
                "declared_sha256":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, error) = invoke(
        setup.router,
        request(
            "POST",
            &format!("{artifact_url}/storage-references/{}", Uuid::new_v4()),
            &setup.token,
            Some(json!({
                "access_event_id":Uuid::new_v4(),
                "storage_type":"HTTPS",
                "locator":"https://example.invalid/private.pdf?X-Amz-Signature=secret",
                "provider":null,
                "environment_label":null,
                "resolver_profile":null,
                "notes":null,
                "access_status":"UNVERIFIED",
                "access_note":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "invalid artifact metadata");
}
