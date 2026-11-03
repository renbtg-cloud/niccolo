#![cfg(feature = "pg-integration")]
//! Real-PostgreSQL operational boundaries: a live socket is not readiness.
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use niccolo_evidence::api;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

async fn check(url_env: &str, expected: StatusCode) {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var(url_env).expect("explicit disposable test URL"))
        .await
        .expect("connect test db");
    let response = api::router(pool)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), expected, "role named by {url_env}");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["x-frame-options"], "DENY");
}
#[tokio::test]
async fn dedicated_api_role_is_ready() {
    check("NICCOLO_PG_SECURE_URL", StatusCode::OK).await;
}
#[tokio::test]
async fn privileged_db_pool_is_never_ready() {
    check("NICCOLO_PG_ADMIN_URL", StatusCode::SERVICE_UNAVAILABLE).await;
}
#[tokio::test]
async fn legacy_db_pool_is_not_ready_for_public_api() {
    check("NICCOLO_PG_SERVICE_URL", StatusCode::SERVICE_UNAVAILABLE).await;
}
