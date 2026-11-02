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
