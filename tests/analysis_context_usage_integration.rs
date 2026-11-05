#![cfg(feature = "pg-integration")]
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
struct F {
    admin: PgPool,
    router: Router,
    w: Uuid,
    writer: String,
    reader: String,
}
async fn setup() -> F {
    let admin = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'usage-test')")
        .bind(w)
        .execute(&admin)
        .await
        .unwrap();
    let writer = authz::provision_token(&admin, Uuid::new_v4(), w, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), w, true, false, 1)
        .await
        .unwrap();
    F {
        admin,
        router: api::router(secure),
        w,
        writer,
        reader,
    }
}
fn req(m: &str, p: String, t: &str, b: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(m)
        .uri(p)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {t}"))
        .body(b.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}
async fn call(r: Router, q: Request<Body>) -> (StatusCode, Value) {
    let x = r.oneshot(q).await.unwrap();
    let s = x.status();
    let b = x.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&b).unwrap_or_else(|_| json!({})))
}
fn sit(f: &F, s: Uuid) -> String {
    format!("/v1/workspaces/{}/situations/{s}", f.w)
}
fn contrib(f: &F, s: Uuid, c: Uuid) -> String {
    format!("{}/contributions/{c}", sit(f, s))
}
fn analysis(f: &F, s: Uuid, a: Uuid) -> String {
    format!("{}/analysis-snapshots/{a}", sit(f, s))
}
fn used(f: &F, s: Uuid, a: Uuid) -> String {
    format!("{}/used-contributions", analysis(f, s, a))
}
async fn seed(f: &F, kind: &str) -> (Uuid, Uuid, Uuid) {
    let s = Uuid::new_v4();
    let c = Uuid::new_v4();
    let a = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                sit(f, s),
                &f.writer,
                Some(json!({"focus":"Understand"}))
            )
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
                contrib(f, s, c),
                &f.writer,
                Some(json!({"kind":kind,"text":"Human supplied premise"}))
            )
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
                analysis(f, s, a),
                &f.writer,
                Some(json!({"goal":"understand what matters"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    (s, c, a)
}
#[tokio::test]
async fn present_contribution_can_be_marked_used_without_becoming_truth() {
    let f = setup().await;
    let (s, c, a) = seed(&f, "user_reported").await;
    let (status, v) = call(
        f.router.clone(),
        req("POST", format!("{}/{}", used(&f, s, a), c), &f.writer, None),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(v["kind"], "USER_REPORTED");
    assert_eq!(v["used_does_not_imply_true"], true);
    assert_eq!(v["is_verified_evidence"], false);
    let list = call(
        f.router.clone(),
        req("GET", used(&f, s, a), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(
        list["used_contributions"][0]["contribution_id"],
        c.to_string()
    );
    assert_eq!(list["presence_in_input_does_not_imply_use"], true);
}
#[tokio::test]
async fn hypothetical_stays_hypothetical_when_used() {
    let f = setup().await;
    let (s, c, a) = seed(&f, "hypothetical").await;
    let v = call(
        f.router.clone(),
        req("POST", format!("{}/{}", used(&f, s, a), c), &f.writer, None),
    )
    .await
    .1;
    assert_eq!(v["kind"], "HYPOTHETICAL");
    assert_eq!(v["used_does_not_imply_true"], true);
}
#[tokio::test]
async fn later_contribution_cannot_be_backfilled_into_old_analysis() {
    let f = setup().await;
    let (s, _c, a) = seed(&f, "user_reported").await;
    let late = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                contrib(&f, s, late),
                &f.writer,
                Some(json!({"kind":"user_reported","text":"Arrived later"}))
            )
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
                format!("{}/{}", used(&f, s, a), late),
                &f.writer,
                None
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_used_contributions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(f.w).bind(s).bind(a).fetch_one(&f.admin).await.unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn reader_cannot_record_and_identical_retry_is_idempotent() {
    let f = setup().await;
    let (s, c, a) = seed(&f, "investigative_preference").await;
    let p = format!("{}/{}", used(&f, s, a), c);
    assert_eq!(
        call(f.router.clone(), req("POST", p.clone(), &f.reader, None))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(f.router.clone(), req("POST", p.clone(), &f.writer, None))
            .await
            .0,
        StatusCode::CREATED
    );
    let (status, v) = call(f.router.clone(), req("POST", p, &f.writer, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["replayed"], true);
}
