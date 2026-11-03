#![cfg(feature = "pg-integration")]
//! Actual PostgreSQL RLS policy tests using a separate API database role,
//! never the unrestricted legacy test-service role.
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

async fn fixture() -> (PgPool, PgPool, Uuid, Uuid, String, Router) {
    let admin = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let p = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'secure-test')")
        .bind(w)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, p, w, true, true, 1)
        .await
        .unwrap();
    (admin, secure.clone(), w, p, token, api::router(secure))
}
async fn scoped_rows(
    pool: &PgPool,
    workspace: Uuid,
    digest: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .execute(&mut *tx)
        .await?;
    if let Some(d) = digest {
        sqlx::query("SELECT set_config('niccolo.auth_digest',$1,true)")
            .bind(d)
            .execute(&mut *tx)
            .await?;
    }
    let found = sqlx::query_scalar("SELECT count(*) FROM niccolo.workspaces")
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(found)
}
fn request(method: &str, url: &str, token: Option<&str>, data: Option<Value>) -> Request<Body> {
    let mut q = Request::builder()
        .method(method)
        .uri(url)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        q = q.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    q.body(data.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}
async fn call(router: Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(json!({})))
}
#[tokio::test]
async fn secure_role_denies_without_digest_even_after_workspace_guc_is_set() {
    let (_, pool, w, _, _, _) = fixture().await;
    assert_eq!(scoped_rows(&pool, w, None).await.unwrap(), 0);
    assert_eq!(
        scoped_rows(&pool, w, Some(&"f".repeat(64))).await.unwrap(),
        0
    );
}
#[tokio::test]
async fn valid_digest_opens_exactly_one_workspace_not_any_other() {
    let (admin, pool, w, _, token, _) = fixture().await;
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'other')")
        .bind(other)
        .execute(&admin)
        .await
        .unwrap();
    let digest = authz::credential_digest(&token).unwrap();
    assert_eq!(scoped_rows(&pool, w, Some(&digest)).await.unwrap(), 1);
    assert_eq!(scoped_rows(&pool, other, Some(&digest)).await.unwrap(), 0);
}
#[tokio::test]
async fn cannot_set_role_to_legacy_unrestricted_service_role() {
    let (_, pool, _, _, _, _) = fixture().await;
    assert!(
        sqlx::query("SET ROLE niccolo_app")
            .execute(&pool)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn denied_token_reuse_after_revocation_even_with_saved_digest() {
    let (admin, pool, w, _, token, _) = fixture().await;
    let digest = authz::credential_digest(&token).unwrap();
    assert_eq!(scoped_rows(&pool, w, Some(&digest)).await.unwrap(), 1);
    sqlx::query("UPDATE niccolo_auth.tokens SET revoked_at=now() WHERE token_sha256=$1")
        .bind(&digest)
        .execute(&admin)
        .await
        .unwrap();
    assert_eq!(scoped_rows(&pool, w, Some(&digest)).await.unwrap(), 0);
}
#[tokio::test]
async fn read_only_token_cannot_write_even_if_workspace_is_set() {
    let (admin, pool, w, _, _, _) = fixture().await;
    let principal = Uuid::new_v4();
    let read = authz::provision_token(&admin, principal, w, true, false, 1)
        .await
        .unwrap();
    let digest = authz::credential_digest(&read).unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('niccolo.auth_digest',$1,true)")
        .bind(&digest)
        .execute(&mut *tx)
        .await
        .unwrap();
    let _found: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.workspaces")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let changed=sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,'not allowed')")
        .bind(w).bind(Uuid::new_v4()).execute(&mut *tx).await;
    assert!(changed.is_err());
    tx.rollback().await.unwrap();
}
#[tokio::test]
async fn session_scope_does_not_survive_connection_reuse() {
    let (_, pool, w, _, token, _) = fixture().await;
    let digest = authz::credential_digest(&token).unwrap();
    assert_eq!(scoped_rows(&pool, w, Some(&digest)).await.unwrap(), 1);
    for _ in 0..15 {
        assert_eq!(scoped_rows(&pool, w, None).await.unwrap(), 0);
    }
}
#[tokio::test]
async fn secure_http_api_supports_authorized_question_and_feedback() {
    let (_, _, w, p, token, router) = fixture().await;
    let q = Uuid::new_v4();
    let e = Uuid::new_v4();
    let question_url = format!("/v1/workspaces/{w}/questions/{q}");
    assert_eq!(
        call(
            router.clone(),
            request(
                "POST",
                &question_url,
                Some(&token),
                Some(json!({"text":"Which original export?"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let feedback_url = format!("/v1/workspaces/{w}/questions/{q}/feedback/{e}");
    let (status, result) = call(
        router.clone(),
        request(
            "POST",
            &feedback_url,
            Some(&token),
            Some(json!({"response":"unknown","human_reason":"needs verification"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(result["actor_principal_id"], p.to_string());
    let (status, history) = call(
        router,
        request(
            "GET",
            &format!("/v1/workspaces/{w}/questions/{q}/feedback"),
            Some(&token),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history["events"].as_array().unwrap().len(), 1);
}
#[tokio::test]
async fn secure_http_cannot_follow_valid_token_into_unrelated_workspace() {
    let (admin, _, w, _, token, r) = fixture().await;
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'unrelated')")
        .bind(other)
        .execute(&admin)
        .await
        .unwrap();
    assert_eq!(
        call(
            r.clone(),
            request(
                "GET",
                &format!("/v1/workspaces/{w}/questions"),
                Some(&token),
                None
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            r,
            request(
                "GET",
                &format!("/v1/workspaces/{other}/questions"),
                Some(&token),
                None
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn secure_http_auth_digest_does_not_leak_between_concurrent_requests() {
    let (admin, _, w, _, token, r) = fixture().await;
    let other = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'concurrent-other')")
        .bind(other)
        .execute(&admin)
        .await
        .unwrap();
    let other_token = authz::provision_token(&admin, Uuid::new_v4(), other, true, true, 1)
        .await
        .unwrap();
    let mut jobs = Vec::new();
    for n in 0..128 {
        let r = r.clone();
        let (t, workspace) = if n % 2 == 0 {
            (token.clone(), w)
        } else {
            (other_token.clone(), other)
        };
        jobs.push(tokio::spawn(async move {
            let allowed = call(
                r.clone(),
                request(
                    "GET",
                    &format!("/v1/workspaces/{workspace}/questions"),
                    Some(&t),
                    None,
                ),
            )
            .await
            .0;
            let other_workspace = if workspace == w { other } else { w };
            let forbidden = call(
                r,
                request(
                    "GET",
                    &format!("/v1/workspaces/{other_workspace}/questions"),
                    Some(&t),
                    None,
                ),
            )
            .await
            .0;
            (allowed, forbidden)
        }));
    }
    for task in jobs {
        let (allowed, forbidden) = task.await.unwrap();
        assert_eq!(allowed, StatusCode::OK);
        assert_eq!(forbidden, StatusCode::FORBIDDEN);
    }
}
#[tokio::test]
async fn every_workspace_table_has_all_four_restrictive_policies() {
    let (admin, _, _, _, _, _) = fixture().await;
    let bad:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT c.relname,count(p.policyname) as n \
        FROM pg_class c JOIN pg_namespace ns ON ns.oid=c.relnamespace \
        JOIN pg_attribute a ON a.attrelid=c.oid AND a.attname='workspace_id' \
        LEFT JOIN pg_policies p ON p.schemaname='niccolo' AND p.tablename=c.relname AND p.policyname LIKE 'api_%_gate' \
        WHERE ns.nspname='niccolo' AND c.relkind='r' GROUP BY c.relname) q WHERE n<>4")
        .fetch_one(&admin).await.unwrap();
    assert_eq!(bad, 0);
}
