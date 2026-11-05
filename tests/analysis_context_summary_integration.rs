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
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'context-summary')")
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
fn con(f: &F, s: Uuid, c: Uuid) -> String {
    format!("{}/contributions/{c}", sit(f, s))
}
fn ana(f: &F, s: Uuid, a: Uuid) -> String {
    format!("{}/analysis-snapshots/{a}", sit(f, s))
}
async fn seed(f: &F) -> (Uuid, Uuid, Uuid, Uuid) {
    let s = Uuid::new_v4();
    let c1 = Uuid::new_v4();
    let c2 = Uuid::new_v4();
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
    for (c, k, t) in [
        (c1, "user_reported", "Alice said X"),
        (c2, "hypothetical", "WHAT IF Bob never saw it?"),
    ] {
        assert_eq!(
            call(
                f.router.clone(),
                req(
                    "POST",
                    con(f, s, c),
                    &f.writer,
                    Some(json!({"kind":k,"text":t}))
                )
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
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
    (s, c1, c2, a)
}
#[tokio::test]
async fn present_and_used_are_distinct() {
    let f = setup().await;
    let (s, c1, c2, a) = seed(&f).await;
    let used = format!("{}/used-contributions/{c1}", ana(&f, s, a));
    assert_eq!(
        call(f.router.clone(), req("POST", used, &f.writer, None))
            .await
            .0,
        StatusCode::CREATED
    );
    let v = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}/context-usage", ana(&f, s, a)),
            &f.reader,
            None,
        ),
    )
    .await
    .1;
    let xs = v["contributions"].as_array().unwrap();
    assert_eq!(xs.len(), 2);
    assert_eq!(
        xs.iter()
            .find(|x| x["contribution_id"] == c1.to_string())
            .unwrap()["used_by_analysis"],
        true
    );
    assert_eq!(
        xs.iter()
            .find(|x| x["contribution_id"] == c2.to_string())
            .unwrap()["used_by_analysis"],
        false
    );
    assert_eq!(v["presence_in_input_does_not_imply_use"], true);
}
#[tokio::test]
async fn hypothetical_kind_survives_summary() {
    let f = setup().await;
    let (s, _c1, c2, a) = seed(&f).await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/used-contributions/{c2}", ana(&f, s, a)),
                &f.writer,
                None
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let v = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}/context-usage", ana(&f, s, a)),
            &f.reader,
            None,
        ),
    )
    .await
    .1;
    let x = v["contributions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["contribution_id"] == c2.to_string())
        .unwrap();
    assert_eq!(x["kind"], "HYPOTHETICAL");
    assert_eq!(x["is_verified_evidence"], false);
}
#[tokio::test]
async fn later_contribution_is_not_present_in_old_summary() {
    let f = setup().await;
    let (s, _c1, _c2, a) = seed(&f).await;
    let late = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                con(&f, s, late),
                &f.writer,
                Some(json!({"kind":"user_reported","text":"later"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let v = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}/context-usage", ana(&f, s, a)),
            &f.reader,
            None,
        ),
    )
    .await
    .1;
    assert!(
        !v["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["contribution_id"] == late.to_string())
    );
}
#[tokio::test]
async fn reader_can_read_but_invalid_limit_is_rejected() {
    let f = setup().await;
    let (s, _c1, _c2, a) = seed(&f).await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "GET",
                format!("{}/context-usage?limit=1", ana(&f, s, a)),
                &f.reader,
                None
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
                "GET",
                format!("{}/context-usage?limit=0", ana(&f, s, a)),
                &f.reader,
                None
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
