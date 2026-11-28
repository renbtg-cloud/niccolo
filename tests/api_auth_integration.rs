#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz::{self, Capability}};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

#[path = "support/historical_sources.rs"]
mod historical_sources;

async fn setup() -> (PgPool, PgPool, Uuid, Uuid, String, Router) {
    let admin = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let app = PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'v007-test')")
        .bind(w)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, principal, w, true, true, 7)
        .await
        .unwrap();
    let router = api::router(app.clone());
    (admin, app, w, principal, token, router)
}
fn request(
    method: &str,
    path: &str,
    token: Option<&str>,
    content_type: Option<&str>,
    locator: Option<&str>,
    body: &[u8],
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(ct) = content_type {
        builder = builder.header(header::CONTENT_TYPE, ct);
    }
    if let Some(loc) = locator {
        builder = builder.header("x-source-locator", loc);
    }
    builder.body(Body::from(body.to_vec())).unwrap()
}
async fn invoke(router: Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let val =
        serde_json::from_slice(&bytes).unwrap_or(json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, val)
}
fn graph(w: Uuid) -> String {
    format!("/v1/workspaces/{w}/graph")
}
fn source(w: Uuid, id: Uuid) -> String {
    format!("/v1/workspaces/{w}/sources/{id}")
}
const CHAT_FRAGMENTS: &[(&str, &str)] = &[("Ana", "source A"), ("Bob", "source B")];
const CHAT: &[u8] = b"[02/11/2026, 12:00] Ana: source A\n[02/11/2026, 12:01] Bob: source B\n";

fn situation_path(workspace: Uuid, situation: Uuid) -> String {
    format!("/v1/workspaces/{workspace}/situations/{situation}")
}

fn json_post(path: &str, token: &str, body: Value) -> Request<Body> {
    request(
        "POST",
        path,
        Some(token),
        Some("application/json"),
        None,
        body.to_string().as_bytes(),
    )
}

fn artifact_metadata(label: &str) -> Value {
    json!({"artifact_type":"EXTERNAL_RECORD", "label":label})
}

async fn create_situation(router: &Router, path: &str, token: &str) {
    let (status, body) = invoke(
        router.clone(),
        json_post(path, token, json!({"focus":"Inspect attributed records"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

async fn list_artifact_view(router: &Router, situation: &str, token: &str) -> Value {
    let (status, body) = invoke(
        router.clone(),
        request(
            "GET",
            &format!("{situation}/artifacts"),
            Some(token),
            None,
            None,
            b"",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn only_signed_in_members_may_read_workspace() {
    let (admin, app, w, _, token, router) = setup().await;
    let (status, _) = invoke(
        router.clone(),
        request("GET", &graph(w), None, None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = invoke(
        router.clone(),
        request("GET", &graph(w), Some("not-a-token"), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'no-membership')")
        .bind(other)
        .execute(&admin)
        .await
        .unwrap();
    let (status, _) = invoke(
        router.clone(),
        request("GET", &graph(other), Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, data) = invoke(
        router,
        request("GET", &graph(w), Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["node_count"], 0);
    assert!(
        authz::authorize(&app, &token, w, Capability::Read)
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn read_only_members_cannot_write_but_writers_can_read() {
    let (admin, app, workspace, _, token, router) = setup().await;
    let situation = situation_path(workspace, Uuid::new_v4());
    create_situation(&router, &situation, &token).await;
    let read = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 7)
        .await
        .unwrap();
    assert!(
        authz::authorize(&app, &read, workspace, Capability::Read)
            .await
            .unwrap()
    );
    assert!(
        !authz::authorize(&app, &read, workspace, Capability::Write)
            .await
            .unwrap()
    );
    let uri = format!("{situation}/artifacts/{}", Uuid::new_v4());
    let (status, _) = invoke(
        router.clone(),
        json_post(&uri, &read, artifact_metadata("Proposed record")),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, false, true, 7)
        .await
        .unwrap();
    let (status, _) = invoke(
        router.clone(),
        json_post(&uri, &writer, artifact_metadata("Writer's record")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let view = list_artifact_view(&router, &situation, &writer).await;
    assert_eq!(view["artifacts"][0]["label"], "Writer's record");
}

#[tokio::test]
async fn revocation_expiry_and_disabled_principal_are_enforced() {
    let (admin, app, w, p, token, _) = setup().await;
    assert!(
        authz::authorize(&app, &token, w, Capability::Read)
            .await
            .unwrap()
    );
    sqlx::query("UPDATE niccolo_auth.memberships SET revoked_at=now() WHERE principal_id=$1 AND workspace_id=$2")
        .bind(p).bind(w).execute(&admin).await.unwrap();
    assert!(
        !authz::authorize(&app, &token, w, Capability::Read)
            .await
            .unwrap()
    );
    sqlx::query("UPDATE niccolo_auth.memberships SET revoked_at=NULL WHERE principal_id=$1 AND workspace_id=$2")
        .bind(p).bind(w).execute(&admin).await.unwrap();
    sqlx::query("UPDATE niccolo_auth.tokens SET revoked_at=now() WHERE principal_id=$1")
        .bind(p)
        .execute(&admin)
        .await
        .unwrap();
    assert!(
        !authz::authorize(&app, &token, w, Capability::Read)
            .await
            .unwrap()
    );
    let token2 = authz::provision_token(&admin, p, w, true, true, 7)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE niccolo_auth.tokens SET expires_at=created_at+interval '1 microsecond' WHERE token_sha256=$1",
    )
    .bind(authz::credential_digest(&token2).unwrap())
    .execute(&admin)
    .await
    .unwrap();
    assert!(
        !authz::authorize(&app, &token2, w, Capability::Read)
            .await
            .unwrap()
    );
    let token3 = authz::provision_token(&admin, p, w, true, true, 7)
        .await
        .unwrap();
    sqlx::query("UPDATE niccolo_auth.principals SET active=false WHERE principal_id=$1")
        .bind(p)
        .execute(&admin)
        .await
        .unwrap();
    assert!(
        !authz::authorize(&app, &token3, w, Capability::Read)
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn service_role_cannot_query_auth_tables_or_modify_access() {
    let (_, app, w, _, token, _) = setup().await;
    let err = sqlx::query("SELECT token_sha256 FROM niccolo_auth.tokens")
        .fetch_all(&app)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"));
    let err = sqlx::query("UPDATE niccolo_auth.memberships SET can_write=true")
        .execute(&app)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"));
    assert!(
        authz::authorize(&app, &token, w, Capability::Read)
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn retired_upload_rejects_email_bytes_without_echoing_or_storing_them() {
    let (admin, _, workspace, _, token, router) = setup().await;
    let (status, response) = invoke(
        router,
        request(
            "POST",
            &source(workspace, Uuid::new_v4()),
            Some(&token),
            Some("message/rfc822"),
            Some("report.eml"),
            b"From: Ana\nContent-Type: text/html\n\n<p>Private evidence</p>",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!response.to_string().contains("Private evidence"));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_documents WHERE workspace_id=$1")
            .bind(workspace)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn external_artifact_locator_preserves_utf8_file_names() {
    let (_, _, workspace, _, token, router) = setup().await;
    let situation = situation_path(workspace, Uuid::new_v4());
    create_situation(&router, &situation, &token).await;
    let artifact = format!("{situation}/artifacts/{}", Uuid::new_v4());
    assert_eq!(
        invoke(
            router.clone(),
            json_post(&artifact, &token, artifact_metadata("Meeting notes"))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let uri = format!("{artifact}/storage-references/{}", Uuid::new_v4());
    let locator = "Recepção — reunião.txt";
    let input = json!({
        "access_event_id":Uuid::new_v4(), "storage_type":"FILE_PATH", "locator":locator,
        "access_status":"UNVERIFIED"
    });
    assert_eq!(
        invoke(router.clone(), json_post(&uri, &token, input.clone()))
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        invoke(router.clone(), json_post(&uri, &token, input))
            .await
            .0,
        StatusCode::OK
    );
    let view = list_artifact_view(&router, &situation, &token).await;
    assert_eq!(view["storage_references"].as_array().unwrap().len(), 1);
    assert_eq!(view["storage_references"][0]["locator"], locator);
}

#[tokio::test]
async fn artifact_is_atomic_replayable_and_preserves_attributed_provenance() {
    let (_, _, workspace, _, token, router) = setup().await;
    let situation = situation_path(workspace, Uuid::new_v4());
    create_situation(&router, &situation, &token).await;
    let id = Uuid::new_v4();
    let path = format!("{situation}/artifacts/{id}");
    let request = |label| json_post(&path, &token, artifact_metadata(label));
    let (created, first) = invoke(router.clone(), request("Audit notes")).await;
    assert_eq!(created, StatusCode::CREATED);
    let (replayed, again) = invoke(router.clone(), request("Audit notes")).await;
    assert_eq!(replayed, StatusCode::OK);
    assert_eq!(again["replayed"], true);
    assert_eq!(again["artifact_id"], first["artifact_id"]);
    assert_eq!(first["source_bytes_stored"], false);
    assert_eq!(first["artifact_was_inspected"], false);
    assert_eq!(
        invoke(router.clone(), request("Altered label")).await.0,
        StatusCode::CONFLICT
    );
    let representation = Uuid::new_v4();
    let representation_path = format!("{path}/representations/{representation}");
    let body = json!({
        "representation_type":"HUMAN_TRANSCRIPTION", "producer_type":"THIRD_PARTY_HUMAN",
        "producer_label":"Ana", "scope_locator":"Second answer", "content":"Described timing only",
        "completeness_status":"PARTIAL"
    });
    assert_eq!(
        invoke(
            router.clone(),
            json_post(&representation_path, &token, body)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let view = list_artifact_view(&router, &situation, &token).await;
    assert_eq!(view["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(view["artifacts"][0]["label"], "Audit notes");
    let excerpt = &view["representations"][0];
    assert_eq!(excerpt["representation_id"], json!(representation));
    assert_eq!(excerpt["producer_label"], "Ana");
    assert_eq!(excerpt["scope_locator"], "Second answer");
    assert_eq!(excerpt["representation_establishes_truth"], false);
}

#[tokio::test]
async fn unauthorized_assessment_is_rejected_before_graph_loading() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let path = format!("/v1/workspaces/{w}/assess");
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &path,
            None,
            Some("application/json"),
            None,
            b"{\"report_ids\":[]}",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &path,
            Some(&token),
            Some("application/json"),
            None,
            b"{\"report_ids\":[]}",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = invoke(
        router,
        request(
            "POST",
            &path,
            Some(&token),
            Some("application/json"),
            None,
            b"{\"report_ids\":[\"00000000-0000-0000-0000-000000000001\"]}",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
#[tokio::test]
async fn invalid_storage_reference_requests_do_not_write() {
    let (_, _, workspace, _, token, router) = setup().await;
    let situation = situation_path(workspace, Uuid::new_v4());
    create_situation(&router, &situation, &token).await;
    let artifact = format!("{situation}/artifacts/{}", Uuid::new_v4());
    assert_eq!(
        invoke(
            router.clone(),
            json_post(&artifact, &token, artifact_metadata("Audit notes"))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let path = format!("{artifact}/storage-references/{}", Uuid::new_v4());
    let (status, _) = invoke(
        router.clone(),
        request("POST", &path, Some(&token), Some("text/plain"), None, CHAT),
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let input = json!({"access_event_id":Uuid::new_v4(), "storage_type":"FILE_PATH", "access_status":"UNVERIFIED"});
    let (status, _) = invoke(router.clone(), json_post(&path, &token, input)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        list_artifact_view(&router, &situation, &token).await["storage_references"],
        json!([])
    );
}

#[tokio::test]
async fn artifact_payload_limit_is_enforced() {
    let (_, _, workspace, _, token, router) = setup().await;
    let situation = situation_path(workspace, Uuid::new_v4());
    create_situation(&router, &situation, &token).await;
    let path = format!("{situation}/artifacts/{}", Uuid::new_v4());
    let huge = artifact_metadata(&"X".repeat(4 * 1024 * 1024 + 1));
    let (status, _) = invoke(router.clone(), json_post(&path, &token, huge)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        list_artifact_view(&router, &situation, &token).await["artifacts"],
        json!([])
    );
}

#[tokio::test]
async fn concurrent_artifact_replays_have_only_one_insert() {
    let (_, _, workspace, _, token, router) = setup().await;
    let situation = situation_path(workspace, Uuid::new_v4());
    create_situation(&router, &situation, &token).await;
    let path = format!("{situation}/artifacts/{}", Uuid::new_v4());
    let joins = (0..32)
        .map(|_| {
            let router = router.clone();
            let req = json_post(&path, &token, artifact_metadata("Raced record"));
            tokio::spawn(invoke(router, req))
        })
        .collect::<Vec<_>>();
    let mut inserted = 0;
    let mut replayed = 0;
    for join in joins {
        let (status, body) = join.await.unwrap();
        match status {
            StatusCode::CREATED => inserted += 1,
            StatusCode::OK => replayed += 1,
            other => panic!("unexpected {other}: {body}"),
        }
    }
    assert_eq!((inserted, replayed), (1, 31));
    assert_eq!(
        list_artifact_view(&router, &situation, &token).await["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn cross_workspace_identical_artifact_uuid_never_crosses_tenants() {
    let (admin, app, workspace, _, token, router) = setup().await;
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES ($1,'second')")
        .bind(other)
        .execute(&admin)
        .await
        .unwrap();
    let other_token = authz::provision_token(&admin, Uuid::new_v4(), other, true, true, 7)
        .await
        .unwrap();
    let situation = Uuid::new_v4();
    let local = situation_path(workspace, situation);
    let foreign = situation_path(other, situation);
    let id = Uuid::new_v4();
    for (path, credential, label) in [
        (&local, &token, "Local record"),
        (&foreign, &other_token, "Foreign record"),
    ] {
        create_situation(&router, path, credential).await;
        let (status, body) = invoke(
            router.clone(),
            json_post(
                &format!("{path}/artifacts/{id}"),
                credential,
                artifact_metadata(label),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(
            list_artifact_view(&router, path, credential).await["artifacts"][0]["label"],
            label
        );
    }
    let (status, _) = invoke(
        router,
        request(
            "GET",
            &format!("{foreign}/artifacts"),
            Some(&token),
            None,
            None,
            b"",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        !authz::authorize(&app, &other_token, workspace, Capability::Read)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn unsupported_provisioning_options_are_rejected() {
    let (admin, _app, w, _, _, _) = setup().await;
    for days in [-1, 0, 366] {
        assert!(
            authz::provision_token(&admin, Uuid::new_v4(), w, true, true, days)
                .await
                .is_err()
        );
    }
    assert!(
        authz::provision_token(&admin, Uuid::new_v4(), w, false, false, 7)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn list_sources_is_paginated_and_tenant_scoped() {
    let (admin, _, w, _, token, router) = setup().await;
    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'foreign')")
        .bind(foreign)
        .execute(&admin)
        .await
        .unwrap();
    for idx in 0..6 {
        historical_sources::seed(&admin, w, Uuid::new_v4(), &format!("fixture://source-{idx}"), CHAT_FRAGMENTS).await;
    }
    let foreign_id = Uuid::new_v4();
    historical_sources::seed(&admin, foreign, foreign_id, "fixture://private", CHAT_FRAGMENTS).await;
    let mut cursor: Option<String> = None;
    let mut seen = std::collections::BTreeSet::new();
    for page_no in 0..3 {
        let url = match cursor {
            Some(ref c) => format!("/v1/workspaces/{w}/sources?limit=2&after={c}"),
            None => format!("/v1/workspaces/{w}/sources?limit=2"),
        };
        let (status, result) = invoke(
            router.clone(),
            request("GET", &url, Some(&token), None, None, b""),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let docs = result["documents"].as_array().unwrap();
        assert_eq!(docs.len(), 2);
        for item in docs {
            assert_eq!(item["fragment_count"], 2);
            let id = item["document_id"].as_str().unwrap().to_string();
            assert_ne!(id, foreign_id.to_string());
            assert!(seen.insert(id));
        }
        cursor = result["next_after"].as_str().map(str::to_owned);
        if page_no < 2 {
            assert!(cursor.is_some())
        } else {
            assert!(cursor.is_none())
        }
    }
    assert_eq!(seen.len(), 6);
    let url = format!("/v1/workspaces/{w}/sources?limit=101");
    assert_eq!(
        invoke(
            router.clone(),
            request("GET", &url, Some(&token), None, None, b"")
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let url = format!("/v1/workspaces/{foreign}/sources");
    assert_eq!(
        invoke(router, request("GET", &url, Some(&token), None, None, b""))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn duplicate_assessment_ids_are_rejected_before_analysis() {
    let (_, _, w, _, token, router) = setup().await;
    let same = Uuid::new_v4();
    let path = format!("/v1/workspaces/{w}/assess");
    let body = json!({"report_ids":[same,same]}).to_string();
    let (status, _) = invoke(
        router,
        request(
            "POST",
            &path,
            Some(&token),
            Some("application/json"),
            None,
            body.as_bytes(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rotation_is_atomic_and_preserves_expiration() {
    let (admin, app, w, _, original, _) = setup().await;
    let rotated = authz::rotate_token(&admin, &original).await.unwrap();
    assert_ne!(original, rotated);
    assert!(
        !authz::authorize(&app, &original, w, Capability::Read)
            .await
            .unwrap()
    );
    assert!(
        authz::authorize(&app, &rotated, w, Capability::Read)
            .await
            .unwrap()
    );
    assert!(authz::rotate_token(&admin, &original).await.is_err());
    assert!(authz::rotate_token(&app, &rotated).await.is_err());
}

#[tokio::test]
async fn source_speaker_labels_are_visible_without_becoming_identities() {
    let (admin, _, w, _, token, router) = setup().await;
    let document = Uuid::new_v4();
    historical_sources::seed(&admin, w, document, "fixture://identity-review", CHAT_FRAGMENTS).await;

    let path = format!("{}/speakers", source(w, document));
    let (status, view) = invoke(
        router.clone(),
        request("GET", &path, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["speaker_labels_are_verified_identities"], false);
    let speakers = view["speakers"].as_array().unwrap();
    assert_eq!(speakers.len(), 2);
    assert_eq!(speakers[0]["speaker_label"], "Ana");
    assert_eq!(speakers[0]["fragment_count"], 1);
    assert_eq!(speakers[1]["speaker_label"], "Bob");
    assert_eq!(speakers[1]["fragment_count"], 1);

    let (status, _) = invoke(router.clone(), request("GET", &path, None, None, None, b"")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let missing = format!("{}/speakers", source(w, Uuid::new_v4()));
    let (status, _) = invoke(
        router,
        request("GET", &missing, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn identity_catalog_is_paginated_and_human_curated() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);

    for (person, label) in [(first, "Ana"), (second, "Roberto")] {
        let path = format!("/v1/workspaces/{w}/identities/{person}");
        let (status, _) = invoke(
            router.clone(),
            request(
                "POST",
                &path,
                Some(&token),
                Some("application/json"),
                None,
                json!({"display_label":label}).to_string().as_bytes(),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let path = format!("/v1/workspaces/{w}/identities?limit=1");
    let (status, page1) = invoke(
        router.clone(),
        request("GET", &path, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["catalog_is_human_curated"], true);
    assert_eq!(page1["identities"].as_array().unwrap().len(), 1);
    assert_eq!(page1["identities"][0]["person_id"], first.to_string());
    assert_eq!(page1["identities"][0]["display_label"], "Ana");
    assert_eq!(page1["next_after"], first.to_string());

    let path = format!("/v1/workspaces/{w}/identities?after={first}&limit=1");
    let (status, page2) =
        invoke(router, request("GET", &path, Some(&token), None, None, b"")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page2["identities"].as_array().unwrap().len(), 1);
    assert_eq!(page2["identities"][0]["person_id"], second.to_string());
    assert_eq!(page2["identities"][0]["display_label"], "Roberto");
    assert!(page2["next_after"].is_null());
}

#[tokio::test]
async fn presentation_package_requires_authorization_and_published_analysis() {
    let (admin, _app, workspace, _, token, router) = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/presentation-package"
    );
    let (anonymous, _) = invoke(
        router.clone(),
        request("GET", &path, None, None, None, b""),
    )
    .await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    let (not_published, _) = invoke(
        router.clone(),
        request("GET", &path, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(not_published, StatusCode::NOT_FOUND);
    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'foreign-export')")
        .bind(foreign)
        .execute(&admin)
        .await
        .unwrap();
    let foreign_path = format!(
        "/v1/workspaces/{foreign}/situations/{situation}/analysis-snapshots/{analysis}/presentation-package"
    );
    let (forbidden, _) = invoke(
        router,
        request("GET", &foreign_path, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
}


#[tokio::test]
async fn operation_budget_status_is_private_and_missing_operations_stay_hidden() {
    let (admin, _app, workspace, _, token, router) = setup().await;
    let situation = Uuid::new_v4();
    let operation = Uuid::new_v4();
    let route = format!(
        "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/budget"
    );
    assert_eq!(
        invoke(
            router.clone(),
            request("GET", &route, None, None, None, b"")
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        invoke(
            router.clone(),
            request("GET", &route, Some(&token), None, None, b"")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'foreign-budget')")
        .bind(foreign)
        .execute(&admin)
        .await
        .unwrap();
    let foreign_route = format!(
        "/v1/workspaces/{foreign}/situations/{situation}/bff/operations/{operation}/budget"
    );
    assert_eq!(
        invoke(
            router,
            request("GET", &foreign_route, Some(&token), None, None, b"")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
