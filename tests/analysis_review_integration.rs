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
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'review')")
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
            req("POST", sit(f, s), &f.writer, Some(json!({"focus":"Why?"})))
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
                Some(json!({"kind":"user_reported","text":"Context"}))
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
                Some(json!({"goal":"Understand"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    (s, c, a)
}
#[tokio::test]
async fn review_starts_with_honest_zero_usage() {
    let f = setup().await;
    let (s, _c, a) = seed(&f).await;
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/review", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["human_context"]["present_in_input"], 1);
    assert_eq!(v["human_context"]["recorded_as_used"], 0);
    assert_eq!(v["output_present"], false);
    assert_eq!(v["counts_do_not_establish_completeness_or_truth"], true);
}
#[tokio::test]
async fn review_aggregates_use_output_and_clarification() {
    let f = setup().await;
    let (s, c, a) = seed(&f).await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/used-contributions/{c}", ana(&f, s, a)),
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
                format!("{}/output", ana(&f, s, a)),
                &f.writer,
                Some(
                    json!({"answer_text":"Provisional answer","coverage_note":"Bounded input only"})
                )
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let q = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/clarifications/{q}", ana(&f, s, a)),
                &f.writer,
                Some(json!({"text":"When?","reason":"Timing matters"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let ans = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/clarifications/{q}/answers/{ans}", ana(&f, s, a)),
                &f.writer,
                Some(json!({"text":"Tuesday"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/review", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["human_context"]["recorded_as_used"], 1);
    assert_eq!(v["output_present"], true);
    assert_eq!(v["clarifications"]["questions"], 1);
    assert_eq!(v["clarifications"]["answers"], 1);
    assert_eq!(v["delta"]["context_added"], 1);
}
#[tokio::test]
async fn review_detects_focus_delta_without_judging_it() {
    let f = setup().await;
    let (s, _c, a) = seed(&f).await;
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
    let v = call(
        f.router.clone(),
        req("GET", format!("{}/review", ana(&f, s, a)), &f.reader, None),
    )
    .await
    .1;
    assert_eq!(v["input_is_current"], false);
    assert_eq!(v["delta"]["focus_changed"], true);
    assert_eq!(v["human_review_required"], true);
}
