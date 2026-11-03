#![cfg(feature = "pg-integration")]
//! Human hypotheses remain immutable manual annotations tied to two visible sources.
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
struct Setup {
    admin: PgPool,
    router: Router,
    w: Uuid,
    other: Uuid,
    a: Uuid,
    b: Uuid,
    foreign: Uuid,
    token: String,
}
async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let other = Uuid::new_v4();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    for workspace in [w, other] {
        sqlx::query(
            "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'hypothesis-test')",
        )
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    }
    for (ws, id) in [(w, a), (w, b), (other, foreign)] {
        sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'test://source','synthetic','REPORTED_SPEECH')")
   .bind(ws).bind(id).execute(&admin).await.unwrap();
    }
    let token = authz::provision_token(&admin, Uuid::new_v4(), w, true, true, 1)
        .await
        .unwrap();
    Setup {
        admin,
        router: api::router(secure),
        w,
        other,
        a,
        b,
        foreign,
        token,
    }
}
fn req(
    method: &str,
    workspace: Uuid,
    id: Option<Uuid>,
    token: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let url = match id {
        Some(id) => format!("/v1/workspaces/{workspace}/hypotheses/{id}"),
        None => format!("/v1/workspaces/{workspace}/hypotheses"),
    };
    let mut builder = Request::builder()
        .method(method)
        .uri(url)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    builder
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}
fn payload(a: Uuid, b: Uuid, note: &str) -> Value {
    json!({"kind":"apparent_contradiction","left_evidence_id":a,"right_evidence_id":b,"human_note":note})
}
async fn call(router: Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.oneshot(req).await.unwrap();
    let code = resp.status();
    assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (code, serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn idempotent_replay_and_conflicting_replay_do_not_rewrite() {
    let f = setup().await;
    let id = Uuid::new_v4();
    let (code, value) = call(
        f.router.clone(),
        req(
            "POST",
            f.w,
            Some(id),
            Some(&f.token),
            Some(payload(f.a, f.b, "Compare original timestamps")),
        ),
    )
    .await;
    assert_eq!(code, StatusCode::CREATED);
    assert_eq!(value["is_verified_fact"], false);
    let (code, value) = call(
        f.router.clone(),
        req(
            "POST",
            f.w,
            Some(id),
            Some(&f.token),
            Some(payload(f.a, f.b, "Compare original timestamps")),
        ),
    )
    .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(value["replayed"], true);
    let (code, _) = call(
        f.router.clone(),
        req(
            "POST",
            f.w,
            Some(id),
            Some(&f.token),
            Some(payload(f.a, f.b, "Different note")),
        ),
    )
    .await;
    assert_eq!(code, StatusCode::CONFLICT);
    let (code, page) = call(f.router, req("GET", f.w, None, Some(&f.token), None)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(page["hypotheses"].as_array().unwrap().len(), 1);
    assert_eq!(
        page["hypotheses"][0]["human_note"],
        "Compare original timestamps"
    );
}
#[tokio::test]
async fn missing_authorization_and_cross_workspace_evidence_are_rejected() {
    let f = setup().await;
    let id = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                f.w,
                Some(id),
                None,
                Some(payload(f.a, f.b, "Check"))
            )
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                f.other,
                Some(id),
                Some(&f.token),
                Some(payload(f.a, f.b, "Check"))
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                f.w,
                Some(id),
                Some(&f.token),
                Some(payload(f.a, f.foreign, "Check"))
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            f.router,
            req(
                "POST",
                f.w,
                Some(id),
                Some(&f.token),
                Some(payload(f.a, f.a, "Check"))
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn read_only_member_cannot_submit_and_sql_cannot_mutate_records() {
    let f = setup().await;
    let id = Uuid::new_v4();
    let reader = authz::provision_token(&f.admin, Uuid::new_v4(), f.w, true, false, 1)
        .await
        .unwrap();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                f.w,
                Some(id),
                Some(&reader),
                Some(payload(f.a, f.b, "Review"))
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                f.w,
                Some(id),
                Some(&f.token),
                Some(payload(f.a, f.b, "Review"))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert!(sqlx::query("UPDATE niccolo.manual_hypotheses SET human_note='forged' WHERE workspace_id=$1 AND hypothesis_id=$2")
 .bind(f.w).bind(id).execute(&f.admin).await.is_err());
    assert!(
        sqlx::query(
            "DELETE FROM niccolo.manual_hypotheses WHERE workspace_id=$1 AND hypothesis_id=$2"
        )
        .bind(f.w)
        .bind(id)
        .execute(&f.admin)
        .await
        .is_err()
    );
    assert_eq!(
        call(f.router, req("GET", f.w, None, Some(&reader), None))
            .await
            .0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn twenty_four_concurrent_replays_insert_exactly_one_row() {
    let f = setup().await;
    let id = Uuid::new_v4();
    let mut join = tokio::task::JoinSet::new();
    for _ in 0..24 {
        let router = f.router.clone();
        let token = f.token.clone();
        let body = payload(f.a, f.b, "human observation only");
        let w = f.w;
        join.spawn(async move {
            call(router, req("POST", w, Some(id), Some(&token), Some(body)))
                .await
                .0
        });
    }
    let mut created = 0;
    let mut replayed = 0;
    while let Some(done) = join.join_next().await {
        match done.unwrap() {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => replayed += 1,
            other => panic!("unexpected {other}"),
        }
    }
    assert_eq!(created, 1);
    assert_eq!(replayed, 23);
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.manual_hypotheses WHERE workspace_id=$1 AND hypothesis_id=$2",
    )
    .bind(f.w)
    .bind(id)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn new_table_has_composite_foreign_keys_and_digest_gated_rls() {
    let f = setup().await;
    let actor: Uuid = sqlx::query_scalar(
        "SELECT principal_id FROM niccolo_auth.memberships WHERE workspace_id=$1 LIMIT 1",
    )
    .bind(f.w)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    let wrong=sqlx::query("INSERT INTO niccolo.manual_hypotheses(workspace_id,hypothesis_id,kind,left_evidence_id,right_evidence_id,human_note,actor_principal_id) VALUES($1,$2,'ATTRIBUTION_QUESTION',$3,$4,'forged', $5)")
        .bind(f.w).bind(Uuid::new_v4()).bind(f.a).bind(f.foreign).bind(actor).execute(&f.admin).await;
    assert!(
        wrong.is_err(),
        "cross-workspace evidence must fail at the FK"
    );
    let id = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                f.w,
                Some(id),
                Some(&f.token),
                Some(payload(f.a, f.b, "human"))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let secure = PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let mut tx = secure.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(f.w.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.manual_hypotheses WHERE workspace_id=$1")
            .bind(f.w)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        count, 0,
        "workspace GUC without authenticated digest is insufficient"
    );
    tx.rollback().await.unwrap();
    let mut tx = secure.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(f.w.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let digest = authz::credential_digest(&f.token).unwrap();
    sqlx::query("SELECT set_config('niccolo.auth_digest',$1,true)")
        .bind(digest)
        .execute(&mut *tx)
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.manual_hypotheses WHERE workspace_id=$1")
            .bind(f.w)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
async fn pagination_is_bounded_stable_and_nonoverlapping() {
    let f = setup().await;
    for _ in 0..9 {
        assert_eq!(
            call(
                f.router.clone(),
                req(
                    "POST",
                    f.w,
                    Some(Uuid::new_v4()),
                    Some(&f.token),
                    Some(payload(f.a, f.b, "review"))
                )
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut cursor: Option<Uuid> = None;
    loop {
        let url = match cursor {
            Some(c) => format!("/v1/workspaces/{}/hypotheses?limit=3&after={c}", f.w),
            None => format!("/v1/workspaces/{}/hypotheses?limit=3", f.w),
        };
        let response = f
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(url)
                    .header(header::AUTHORIZATION, format!("Bearer {}", f.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        for row in payload["hypotheses"].as_array().unwrap() {
            assert!(
                seen.insert(row["hypothesis_id"].as_str().unwrap().to_string()),
                "duplicate between pages"
            );
        }
        cursor = payload["next_after"]
            .as_str()
            .map(|s| s.parse::<Uuid>().unwrap());
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 9);
}
