#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz, question_refs};
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
    let service = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let p = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'refs-test')")
        .bind(w)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, p, w, true, true, 2)
        .await
        .unwrap();
    (admin, service.clone(), w, p, token, api::router(service))
}
async fn setup_data(admin: &PgPool, w: Uuid) -> (Uuid, Uuid) {
    let q = Uuid::new_v4();
    let e = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,'Why?')")
        .bind(w).bind(q).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'file://synthetic','attributed text','REPORTED_SPEECH')")
        .bind(w).bind(e).execute(admin).await.unwrap();
    (q, e)
}
fn uri(w: Uuid, q: Uuid, r: Uuid) -> String {
    format!("/v1/workspaces/{w}/questions/{q}/references/{r}")
}
fn list_uri(w: Uuid, q: Uuid) -> String {
    format!("/v1/workspaces/{w}/questions/{q}/references")
}
fn http(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    req.body(Body::from(body.to_string())).unwrap()
}
async fn call(r: Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = r.oneshot(req).await.unwrap();
    let code = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (code, serde_json::from_slice(&bytes).unwrap_or(json!({})))
}
fn data(e: Uuid, relation: &str) -> Value {
    json!({"evidence_id":e,"relation":relation,"human_reason":"Source cited by investigator"})
}
#[tokio::test]
async fn reference_is_a_pointer_not_a_truth_assertion() {
    let (a, _, w, p, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let (id, uri) = (Uuid::new_v4(), uri(w, q, Uuid::new_v4()));
    let (s, resp) = call(
        r.clone(),
        http("POST", &uri, Some(&t), data(e, "references")),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(resp["is_evidence"], false);
    assert_eq!(resp["proof_established"], false);
    assert_eq!(resp["actor_principal_id"], p.to_string());
    assert_eq!(
        call(r, http("GET", &list_uri(w, q), Some(&t), json!({})))
            .await
            .1["references"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let _ = id;
}
#[tokio::test]
async fn relations_are_typed_without_promoting_support_to_proof() {
    let (a, _, w, _, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    for relation in ["references", "challenges", "follow_up_on"] {
        let (s, body) = call(
            r.clone(),
            http(
                "POST",
                &uri(w, q, Uuid::new_v4()),
                Some(&t),
                data(e, relation),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(body["proof_established"], false);
    }
    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &uri(w, q, Uuid::new_v4()),
                Some(&t),
                data(e, "supports_truth")
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            r,
            http(
                "POST",
                &uri(w, q, Uuid::new_v4()),
                Some(&t),
                data(e, "proves")
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn idempotency_and_collision_are_enforced() {
    let (a, _, w, _, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let id = Uuid::new_v4();
    let url = uri(w, q, id);
    assert_eq!(
        call(
            r.clone(),
            http("POST", &url, Some(&t), data(e, "references"))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (s, body) = call(
        r.clone(),
        http("POST", &url, Some(&t), data(e, "references")),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["replayed"], true);
    assert_eq!(
        call(r, http("POST", &url, Some(&t), data(e, "challenges")))
            .await
            .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn foreign_workspace_evidence_and_questions_refused() {
    let (a, _, w, _, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let (b, _, other, _, othert, _) = fixture().await;
    let (oq, oe) = setup_data(&b, other).await;
    for (q, e) in [(q, oe), (oq, e)] {
        assert_eq!(
            call(
                r.clone(),
                http(
                    "POST",
                    &uri(w, q, Uuid::new_v4()),
                    Some(&t),
                    data(e, "references")
                )
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        call(r, http("GET", &list_uri(w, q), Some(&othert), json!({})))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn unprivileged_reads_and_writes_obey_membership() {
    let (a, _, w, _, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let read = authz::provision_token(&a, Uuid::new_v4(), w, true, false, 1)
        .await
        .unwrap();
    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &uri(w, q, Uuid::new_v4()),
                Some(&read),
                data(e, "references")
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &uri(w, q, Uuid::new_v4()),
                None,
                data(e, "references")
            )
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &uri(w, q, Uuid::new_v4()),
                Some(&t),
                data(e, "references")
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(r, http("GET", &list_uri(w, q), Some(&read), json!({})))
            .await
            .0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn pagination_bounded_and_correct() {
    let (a, _, w, _, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    for _ in 0..7 {
        assert_eq!(
            call(
                r.clone(),
                http(
                    "POST",
                    &uri(w, q, Uuid::new_v4()),
                    Some(&t),
                    data(e, "references")
                )
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut after = None;
    loop {
        let url = match after {
            Some(x) => format!("{}?limit=3&after={x}", list_uri(w, q)),
            None => format!("{}?limit=3", list_uri(w, q)),
        };
        let (s, page) = call(r.clone(), http("GET", &url, Some(&t), json!({}))).await;
        assert_eq!(s, StatusCode::OK);
        for x in page["references"].as_array().unwrap() {
            assert!(seen.insert(x["reference_id"].as_str().unwrap().to_owned()));
        }
        after = page["next_after"].as_str().map(str::to_owned);
        if after.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 7);
    assert_eq!(
        call(
            r,
            http(
                "GET",
                &format!("{}?limit=999", list_uri(w, q)),
                Some(&t),
                json!({})
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn writes_are_append_only_even_from_sql_service() {
    let (a, s, w, _, t, r) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let id = Uuid::new_v4();
    assert_eq!(
        call(
            r,
            http("POST", &uri(w, q, id), Some(&t), data(e, "references"))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let mut tx = s.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("DELETE FROM niccolo.question_evidence_references WHERE workspace_id=$1 AND reference_id=$2")
        .bind(w).bind(id).execute(&mut *tx).await.is_err());
    tx.rollback().await.unwrap();
}
#[tokio::test]
async fn concurrent_idempotency_creates_one_reference() {
    let (a, s, w, p, _, _) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let id = Uuid::new_v4();
    let mut jobs = Vec::new();
    for _ in 0..32 {
        let pool = s.clone();
        jobs.push(tokio::spawn(async move {
            question_refs::attach(&pool, w, p, id, q, e, "challenges", "Question the citation")
                .await
                .unwrap()
        }));
    }
    let mut first = 0;
    let mut again = 0;
    for j in jobs {
        if j.await.unwrap().replayed {
            again += 1
        } else {
            first += 1
        }
    }
    assert_eq!((first, again), (1, 31));
    let (rows, _) = question_refs::list(&s, w, q, None, 25).await.unwrap();
    assert_eq!(rows.len(), 1);
}
#[tokio::test]
async fn unknown_question_or_evidence_is_not_inserted() {
    let (a, s, w, p, _, _) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    for (q, e) in [(q, Uuid::new_v4()), (Uuid::new_v4(), e)] {
        assert!(matches!(
            question_refs::attach(
                &s,
                w,
                p,
                Uuid::new_v4(),
                q,
                e,
                "references",
                "human source pointer"
            )
            .await,
            Err(question_refs::ReferenceError::Invisible)
        ));
    }
}
#[tokio::test]
async fn provenance_reference_does_not_change_original_evidence() {
    let (a, s, w, p, _, _) = fixture().await;
    let (q, e) = setup_data(&a, w).await;
    let before: (String, bool) = sqlx::query_as(
        "SELECT content,admissible FROM niccolo.evidence WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(w)
    .bind(e)
    .fetch_one(&a)
    .await
    .unwrap();
    question_refs::attach(
        &s,
        w,
        p,
        Uuid::new_v4(),
        q,
        e,
        "challenges",
        "Could be copied",
    )
    .await
    .unwrap();
    let after: (String, bool) = sqlx::query_as(
        "SELECT content,admissible FROM niccolo.evidence WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(w)
    .bind(e)
    .fetch_one(&a)
    .await
    .unwrap();
    assert_eq!(before, after);
}
