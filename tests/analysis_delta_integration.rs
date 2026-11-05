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
struct F {
    router: Router,
    w: Uuid,
    writer: String,
    reader: String,
}
async fn setup() -> F {
    let a = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let p = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'delta')")
        .bind(w)
        .execute(&a)
        .await
        .unwrap();
    let writer = authz::provision_token(&a, Uuid::new_v4(), w, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&a, Uuid::new_v4(), w, true, false, 1)
        .await
        .unwrap();
    F {
        router: api::router(p),
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
fn raw(m: &str, p: String, t: &str, ct: &str, locator: &str, b: &'static [u8]) -> Request<Body> {
    Request::builder()
        .method(m)
        .uri(p)
        .header(header::CONTENT_TYPE, ct)
        .header(header::AUTHORIZATION, format!("Bearer {t}"))
        .header("x-source-locator", locator)
        .body(Body::from(b))
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
fn ana(f: &F, s: Uuid, a: Uuid) -> String {
    format!("{}/analysis-snapshots/{a}", sit(f, s))
}
async fn seed(f: &F) -> (Uuid, Uuid, Uuid) {
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
                Some(json!({"focus":"Original focus"}))
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
                format!("{}/contributions/{c}", sit(f, s)),
                &f.writer,
                Some(json!({"kind":"user_reported","text":"Original context"}))
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
                ana(f, s, a),
                &f.writer,
                Some(json!({"goal":"understand"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    (s, c, a)
}
#[tokio::test]
async fn unchanged_input_has_empty_delta() {
    let f = setup().await;
    let (s, _c, a) = seed(&f).await;
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/delta", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["input_is_current"], true);
    assert_eq!(v["focus_changed"], false);
    assert_eq!(v["added_contributions"].as_array().unwrap().len(), 0);
}
#[tokio::test]
async fn focus_source_and_new_context_are_visible() {
    let f = setup().await;
    let (s, _c, a) = seed(&f).await;
    let d = Uuid::new_v4();
    let late = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            raw(
                "POST",
                format!("/v1/workspaces/{}/sources/{d}", f.w),
                &f.writer,
                "text/plain",
                "fixture://delta",
                b"[02/11/2026, 12:00] Ana: hello\n"
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
                format!("{}/sources/{d}", sit(&f, s)),
                &f.writer,
                None
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
                format!("{}/focus", sit(&f, s)),
                &f.writer,
                Some(json!({"focus":"New focus"}))
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/contributions/{late}", sit(&f, s)),
                &f.writer,
                Some(json!({"kind":"hypothetical","text":"WHAT IF later"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/delta", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["focus_changed"], true);
    assert_eq!(v["added_source_document_ids"][0], d.to_string());
    assert_eq!(v["added_contributions"][0]["kind"], "HYPOTHETICAL");
    assert_eq!(v["delta_is_not_a_truth_judgement"], true);
}
#[tokio::test]
async fn correction_is_removed_plus_replacement() {
    let f = setup().await;
    let (s, c, a) = seed(&f).await;
    let r = Uuid::new_v4();
    let p = format!("{}/contributions/{c}/corrections/{r}", sit(&f, s));
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                p,
                &f.writer,
                Some(json!({"text":"Corrected context"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/delta", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["removed_contributions"][0]["current_status"], "CORRECTED");
    assert_eq!(v["removed_contributions"][0]["corrected_by"], r.to_string());
    assert_eq!(
        v["added_contributions"][0]["contribution_id"],
        r.to_string()
    );
}
#[tokio::test]
async fn retraction_is_visible_without_deleting_history() {
    let f = setup().await;
    let (s, c, a) = seed(&f).await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/contributions/{c}/retract", sit(&f, s)),
                &f.writer,
                None
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/delta", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["removed_contributions"][0]["current_status"], "RETRACTED");
    assert_eq!(
        v["removed_contributions"][0]["contribution_id"],
        c.to_string()
    );
}
