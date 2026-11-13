#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    api,
    authz::{self, Capability},
    source_import::SourceFormat,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

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
const CHAT: &[u8] = b"[02/11/2026, 12:00] Ana: source A\n[02/11/2026, 12:01] Bob: source B\n";

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
    let (admin, app, w, _, _, router) = setup().await;
    let p = Uuid::new_v4();
    let read = authz::provision_token(&admin, p, w, true, false, 7)
        .await
        .unwrap();
    assert!(
        authz::authorize(&app, &read, w, Capability::Read)
            .await
            .unwrap()
    );
    assert!(
        !authz::authorize(&app, &read, w, Capability::Write)
            .await
            .unwrap()
    );
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &source(w, Uuid::new_v4()),
            Some(&read),
            Some("text/plain"),
            Some("fixture://chat"),
            CHAT,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let writer = authz::provision_token(&admin, Uuid::new_v4(), w, false, true, 7)
        .await
        .unwrap();
    let (status, _) = invoke(
        router,
        request("GET", &graph(w), Some(&writer), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
#[tokio::test]
async fn received_source_receipt_is_deterministic_and_contains_no_source_text() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let document = Uuid::new_v4();
    let locator = "fixture://private/client-chat.txt";
    let source_path = source(w, document);
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &source_path,
            Some(&token),
            Some("text/plain"),
            Some(locator),
            CHAT,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let receipt_path = format!("{source_path}/intake-receipt-package");
    let (status, first) = invoke(
        router.clone(),
        request("GET", &receipt_path, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, second) = invoke(
        router,
        request("GET", &receipt_path, Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(first["package_sha256"], second["package_sha256"]);
    assert_eq!(first["receipt"]["state"], "RECEIVED");
    assert_eq!(first["receipt"]["document_id"], document.to_string());
    assert_eq!(first["receipt"]["includes_original_upload"], false);
    assert_eq!(first["receipt"]["includes_source_text"], false);
    assert_eq!(first["receipt"]["includes_analysis"], false);
    assert_eq!(first["establishes_truth"], false);
    assert_eq!(first["establishes_authenticity"], false);
    assert_eq!(first["remote_publication_performed"], false);
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains(locator));
    assert!(!serialized.contains("source A"));
    assert!(!serialized.contains("source B"));
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
async fn invalid_source_errors_name_the_selected_import_contract() {
    let (_admin, _app, w, _, token, router) = setup().await;

    let (text_status, text_error) = invoke(
        router.clone(),
        request(
            "POST",
            &source(w, Uuid::new_v4()),
            Some(&token),
            Some("text/plain"),
            Some("notes.txt"),
            b"ordinary notes without WhatsApp attribution\n",
        ),
    )
    .await;
    assert_eq!(text_status, StatusCode::BAD_REQUEST);
    assert_eq!(text_error["error"], "invalid WhatsApp chat export");

    let (email_status, email_error) = invoke(
        router,
        request(
            "POST",
            &source(w, Uuid::new_v4()),
            Some(&token),
            Some("message/rfc822"),
            Some("mail.eml"),
            b"Subject: no sender\r\n\r\nbody\r\n",
        ),
    )
    .await;
    assert_eq!(email_status, StatusCode::BAD_REQUEST);
    assert_eq!(
        email_error["error"],
        "invalid or unsupported plain-text email source"
    );
}

#[tokio::test]
async fn source_is_atomic_replayable_and_provenance_is_returned_without_truth_claims() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let id = Uuid::new_v4();
    let (created, first) = invoke(
        router.clone(),
        request(
            "POST",
            &source(w, id),
            Some(&token),
            Some("text/plain"),
            Some("fixture://chat"),
            CHAT,
        ),
    )
    .await;
    assert_eq!(created, StatusCode::CREATED);
    assert_eq!(first["fragment_ids"].as_array().unwrap().len(), 2);
    let (replayed, again) = invoke(
        router.clone(),
        request(
            "POST",
            &source(w, id),
            Some(&token),
            Some("text/plain"),
            Some("fixture://chat"),
            CHAT,
        ),
    )
    .await;
    assert_eq!(replayed, StatusCode::OK);
    assert_eq!(again["replayed"], true);
    assert_eq!(again["fragment_ids"], first["fragment_ids"]);
    assert_eq!(again["revision"], first["revision"]);
    let (conflict, _) = invoke(
        router.clone(),
        request(
            "POST",
            &source(w, id),
            Some(&token),
            Some("text/plain"),
            Some("fixture://chat"),
            b"[02/11/2026, 12:00] Ana: altered A\n[02/11/2026, 12:01] Bob: source B\n",
        ),
    )
    .await;
    assert_eq!(conflict, StatusCode::CONFLICT);
    let (status, graph_json) = invoke(
        router.clone(),
        request("GET", &graph(w), Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(graph_json["node_count"], 2);
    let assess_path = format!("/v1/workspaces/{w}/assess");
    let ids = first["fragment_ids"].clone();
    let (status, assessment) = invoke(
        router,
        request(
            "POST",
            &assess_path,
            Some(&token),
            Some("application/json"),
            None,
            json!({"report_ids":ids}).to_string().as_bytes(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(assessment["independence_established"], false);
    assert_eq!(
        assessment["shared_source_documents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(assessment["shared_origins"].as_array().unwrap().len(), 0);
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
async fn source_upload_accepts_standard_content_type_parameters() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let document = Uuid::new_v4();
    let (status, data) = invoke(
        router,
        request(
            "POST",
            &source(w, document),
            Some(&token),
            Some("Text/Plain; charset=utf-8"),
            Some("fixture://content-type-parameter"),
            CHAT,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(data["document_id"], document.to_string());
}

#[tokio::test]
async fn wrong_content_type_and_missing_locator_do_not_write() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let doc = source(w, Uuid::new_v4());
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &doc,
            Some(&token),
            Some("application/json"),
            Some("file://f"),
            CHAT,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (status, _) = invoke(
        router.clone(),
        request("POST", &doc, Some(&token), Some("text/plain"), None, CHAT),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, data) = invoke(
        router,
        request("GET", &graph(w), Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["node_count"], 0);
}
#[tokio::test]
async fn payload_limit_is_enforced() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let huge = vec![b'X'; 4 * 1024 * 1024 + 1];
    let res = router
        .oneshot(request(
            "POST",
            &source(w, Uuid::new_v4()),
            Some(&token),
            Some("text/plain"),
            Some("file://huge"),
            &huge,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
#[tokio::test]
async fn concurrent_uploads_have_only_one_insert() {
    let (_admin, _app, w, _, token, router) = setup().await;
    let path = source(w, Uuid::new_v4());
    let mut joins = Vec::new();
    for _ in 0..32 {
        let cloned = router.clone();
        let t = token.clone();
        let p = path.clone();
        joins.push(tokio::spawn(async move {
            invoke(
                cloned,
                request(
                    "POST",
                    &p,
                    Some(&t),
                    Some("text/plain"),
                    Some("fixture://raced"),
                    CHAT,
                ),
            )
            .await
        }));
    }
    let mut inserted = 0;
    let mut replayed = 0;
    for join in joins {
        let (status, _) = join.await.unwrap();
        match status {
            StatusCode::CREATED => inserted += 1,
            StatusCode::OK => replayed += 1,
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!(inserted, 1);
    assert_eq!(replayed, 31);
}
#[tokio::test]
async fn cross_workspace_identical_document_uuid_never_crosses_tenants() {
    let (admin, app, w, _, token, router) = setup().await;
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES ($1,'second')")
        .bind(other)
        .execute(&admin)
        .await
        .unwrap();
    let other_token = authz::provision_token(&admin, Uuid::new_v4(), other, true, true, 7)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    assert_eq!(
        invoke(
            router.clone(),
            request(
                "POST",
                &source(w, id),
                Some(&token),
                Some("text/plain"),
                Some("fixture://one"),
                CHAT
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        invoke(
            router.clone(),
            request(
                "POST",
                &source(other, id),
                Some(&other_token),
                Some("text/plain"),
                Some("fixture://two"),
                CHAT
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, _) = invoke(
        router.clone(),
        request("GET", &graph(other), Some(&token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, visible) = invoke(
        router,
        request("GET", &graph(other), Some(&other_token), None, None, b""),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(visible["node_count"], 2);
    assert!(
        !authz::authorize(&app, &other_token, w, Capability::Read)
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
    let _ = SourceFormat::Eml;
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
    let foreign_token = authz::provision_token(&admin, Uuid::new_v4(), foreign, true, true, 7)
        .await
        .unwrap();
    for idx in 0..6 {
        let loc = format!("fixture://source-{idx}");
        let (status, _) = invoke(
            router.clone(),
            request(
                "POST",
                &source(w, Uuid::new_v4()),
                Some(&token),
                Some("text/plain"),
                Some(&loc),
                CHAT,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let foreign_id = Uuid::new_v4();
    assert_eq!(
        invoke(
            router.clone(),
            request(
                "POST",
                &source(foreign, foreign_id),
                Some(&foreign_token),
                Some("text/plain"),
                Some("fixture://private"),
                CHAT
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
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
    let (_admin, _app, w, _, token, router) = setup().await;
    let document = Uuid::new_v4();
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &source(w, document),
            Some(&token),
            Some("text/plain"),
            Some("fixture://identity-review"),
            CHAT,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

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
