#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    api, authz,
    identity::{self, IdentityAction},
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;
const CHAT: &[u8] =
    b"[02/11/2026, 12:00] Ana: I will attend\n[02/11/2026, 12:01] Beto: I'll call\n";
async fn fixture() -> (PgPool, PgPool, Uuid, Uuid, String, Router) {
    let a = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let s = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let p = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'identity-test')")
        .bind(w)
        .execute(&a)
        .await
        .unwrap();
    let t = authz::provision_token(&a, p, w, true, true, 7)
        .await
        .unwrap();
    let r = api::router(s.clone());
    (a, s, w, p, t, r)
}
fn http(method: &str, url: &str, token: Option<&str>, data: Value) -> Request<Body> {
    let mut q = Request::builder()
        .method(method)
        .uri(url)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        q = q.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    q.body(Body::from(data.to_string())).unwrap()
}
async fn call(r: Router, q: Request<Body>) -> (StatusCode, Value) {
    let resp = r.oneshot(q).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(json!({})))
}
async fn upload(r: Router, w: Uuid, token: &str) -> Uuid {
    let doc = Uuid::new_v4();
    let req = Request::builder()
        .uri(format!("/v1/workspaces/{w}/sources/{doc}"))
        .method("POST")
        .header(header::CONTENT_TYPE, "text/plain")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("x-source-locator", "fixture://identity")
        .body(Body::from(CHAT))
        .unwrap();
    assert_eq!(call(r, req).await.0, StatusCode::CREATED);
    doc
}
fn identity_url(w: Uuid, p: Uuid) -> String {
    format!("/v1/workspaces/{w}/identities/{p}")
}
fn event_url(w: Uuid, e: Uuid) -> String {
    format!("/v1/workspaces/{w}/identity-events/{e}")
}
fn effective_url(w: Uuid, d: Uuid) -> String {
    format!("/v1/workspaces/{w}/documents/{d}/identities")
}
fn history_url(w: Uuid, d: Uuid) -> String {
    format!("/v1/workspaces/{w}/documents/{d}/identity-history")
}
fn assign(d: Uuid, speaker: &str, p: Uuid, reason: &str) -> Value {
    json!({"document_id":d,"speaker_label":speaker,"operation":"assign","person_id":p,"human_reason":reason})
}
fn clear(d: Uuid, speaker: &str, reason: &str) -> Value {
    json!({"document_id":d,"speaker_label":speaker,"operation":"clear","person_id":null,"human_reason":reason})
}

#[tokio::test]
async fn explicit_assignment_then_correction_then_retraction_preserves_audit() {
    let (_, _, w, p, t, r) = fixture().await;
    let doc = upload(r.clone(), w, &t).await;
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &identity_url(w, first),
                Some(&t),
                json!({"display_label":"Person A"})
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &identity_url(w, second),
                Some(&t),
                json!({"display_label":"Person B"})
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let e1 = Uuid::new_v4();
    let e2 = Uuid::new_v4();
    let e3 = Uuid::new_v4();
    let (_, a) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, e1),
            Some(&t),
            assign(doc, "Ana", first, "human confirmation 1"),
        ),
    )
    .await;
    let (_, b) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, e2),
            Some(&t),
            assign(doc, "Ana", second, "correction by human"),
        ),
    )
    .await;
    let (_, c) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, e3),
            Some(&t),
            clear(doc, "Ana", "withdrawn by human"),
        ),
    )
    .await;
    assert_eq!(a["actor_principal_id"], p.to_string());
    assert!(a["event_no"].as_i64() < b["event_no"].as_i64());
    assert!(b["event_no"].as_i64() < c["event_no"].as_i64());
    let (status, eff) = call(
        r.clone(),
        http("GET", &effective_url(w, doc), Some(&t), json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(eff["bindings"].as_array().unwrap().len(), 1);
    assert!(eff["bindings"][0]["person_id"].is_null());
    let (status, hist) = call(r, http("GET", &history_url(w, doc), Some(&t), json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hist["events"].as_array().unwrap().len(), 3);
    assert_eq!(hist["events"][0]["person_id"], first.to_string());
    assert_eq!(hist["events"][1]["person_id"], second.to_string());
    assert_eq!(hist["events"][2]["operation"], "CLEAR");
}
#[tokio::test]
async fn same_speaker_label_across_documents_never_auto_merges() {
    let (_, _, w, _, t, r) = fixture().await;
    let d1 = upload(r.clone(), w, &t).await;
    let d2 = upload(r.clone(), w, &t).await;
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();
    for p in [p1, p2] {
        assert_eq!(
            call(
                r.clone(),
                http(
                    "POST",
                    &identity_url(w, p),
                    Some(&t),
                    json!({"display_label":"Ana"})
                )
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    for (d, p) in [(d1, p1), (d2, p2)] {
        assert_eq!(
            call(
                r.clone(),
                http(
                    "POST",
                    &event_url(w, Uuid::new_v4()),
                    Some(&t),
                    assign(d, "Ana", p, "human independently verified")
                )
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let (_, a) = call(
        r.clone(),
        http("GET", &effective_url(w, d1), Some(&t), json!({})),
    )
    .await;
    let (_, b) = call(r, http("GET", &effective_url(w, d2), Some(&t), json!({}))).await;
    assert_eq!(a["bindings"][0]["person_id"], p1.to_string());
    assert_eq!(b["bindings"][0]["person_id"], p2.to_string());
    assert_ne!(a["bindings"][0]["person_id"], b["bindings"][0]["person_id"]);
}
#[tokio::test]
async fn retry_event_is_idempotent_and_conflicting_id_is_rejected() {
    let (_, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let p = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, p),
            Some(&t),
            json!({"display_label":"A"}),
        ),
    )
    .await;
    let event = Uuid::new_v4();
    let data = assign(d, "Ana", p, "human evidence");
    let (s, a) = call(
        r.clone(),
        http("POST", &event_url(w, event), Some(&t), data.clone()),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, b) = call(
        r.clone(),
        http("POST", &event_url(w, event), Some(&t), data),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(a["event_no"], b["event_no"]);
    assert_eq!(b["replayed"], true);
    let (s, _) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, event),
            Some(&t),
            clear(d, "Ana", "not same action"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, hist) = call(r, http("GET", &history_url(w, d), Some(&t), json!({}))).await;
    assert_eq!(hist["events"].as_array().unwrap().len(), 1);
}
#[tokio::test]
async fn invented_speaker_and_nonexistent_person_are_refused() {
    let (_, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let p = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, p),
            Some(&t),
            json!({"display_label":"A"}),
        ),
    )
    .await;
    let (s, _) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&t),
            assign(d, "Made-up-speaker", p, "not present"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(
        r,
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&t),
            assign(d, "Ana", Uuid::new_v4(), "no such person"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
#[tokio::test]
async fn read_only_cannot_assign_or_create_identity_but_can_read_history() {
    let (admin, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let read = authz::provision_token(&admin, Uuid::new_v4(), w, true, false, 7)
        .await
        .unwrap();
    let (s, _) = call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, Uuid::new_v4()),
            Some(&read),
            json!({"display_label":"no"}),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&read),
            clear(d, "Ana", "no"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(
        r.clone(),
        http("GET", &history_url(w, d), Some(&read), json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(r, http("GET", &effective_url(w, d), None, json!({}))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn rls_and_db_permissions_protect_identity_tables() {
    let (admin, service, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let p = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, p),
            Some(&t),
            json!({"display_label":"Ana"}),
        ),
    )
    .await;
    call(
        r,
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&t),
            assign(d, "Ana", p, "human"),
        ),
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.identity_events")
        .fetch_one(&service)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let err = sqlx::query("UPDATE niccolo.identity_events SET human_reason='changed'")
        .execute(&service)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"));
    let err = sqlx::query("DELETE FROM niccolo.identity_events")
        .execute(&service)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.identity_events WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(count, 1);
}
#[tokio::test]
async fn history_is_paginated_without_leaking_another_document() {
    let (_, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let other = upload(r.clone(), w, &t).await;
    let p = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, p),
            Some(&t),
            json!({"display_label":"Ana"}),
        ),
    )
    .await;
    for idx in 0..5 {
        let event = Uuid::new_v4();
        let data = if idx % 2 == 0 {
            assign(d, "Ana", p, "positive")
        } else {
            clear(d, "Ana", "negative")
        };
        assert_eq!(
            call(
                r.clone(),
                http("POST", &event_url(w, event), Some(&t), data)
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let mut after: Option<i64> = None;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..3 {
        let url = match after {
            Some(a) => format!("{}?limit=2&after={a}", history_url(w, d)),
            None => format!("{}?limit=2", history_url(w, d)),
        };
        let (status, data) = call(r.clone(), http("GET", &url, Some(&t), json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        for e in data["events"].as_array().unwrap() {
            assert!(seen.insert(e["event_no"].as_i64().unwrap()));
        }
        after = data["next_after"].as_i64();
    }
    assert_eq!(seen.len(), 5);
    assert!(after.is_none());
    let (_, events) = call(r, http("GET", &history_url(w, other), Some(&t), json!({}))).await;
    assert_eq!(events["events"].as_array().unwrap().len(), 0);
}
#[tokio::test]
async fn concurrent_replay_produces_exactly_one_identity_event() {
    let (_, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let p = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, p),
            Some(&t),
            json!({"display_label":"Ana"}),
        ),
    )
    .await;
    let id = Uuid::new_v4();
    let mut jobs = vec![];
    for _ in 0..40 {
        let router = r.clone();
        let tok = t.clone();
        let url = event_url(w, id);
        let data = assign(d, "Ana", p, "human");
        jobs.push(tokio::spawn(async move {
            call(router, http("POST", &url, Some(&tok), data)).await.0
        }));
    }
    let mut created = 0;
    let mut replays = 0;
    for job in jobs {
        match job.await.unwrap() {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => replays += 1,
            s => panic!("{s}"),
        }
    }
    assert_eq!(created, 1);
    assert_eq!(replays, 39);
}
#[tokio::test]
async fn parallel_distinct_corrections_have_deterministic_last_event() {
    let (_, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let p = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, p),
            Some(&t),
            json!({"display_label":"Ana"}),
        ),
    )
    .await;
    let mut jobs = vec![];
    for idx in 0..24 {
        let router = r.clone();
        let tok = t.clone();
        let url = event_url(w, Uuid::new_v4());
        let data = if idx % 2 == 0 {
            assign(d, "Ana", p, "confirmed")
        } else {
            clear(d, "Ana", "withdrawn")
        };
        jobs.push(tokio::spawn(async move {
            call(router, http("POST", &url, Some(&tok), data)).await
        }));
    }
    let mut last_no = 0;
    let mut last_person = None;
    for job in jobs {
        let (status, event) = job.await.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        let no = event["event_no"].as_i64().unwrap();
        if no > last_no {
            last_no = no;
            last_person = event["person_id"].as_str().map(str::to_owned)
        }
    }
    let (_, effective) = call(r, http("GET", &effective_url(w, d), Some(&t), json!({}))).await;
    assert_eq!(effective["bindings"][0]["event_no"], last_no);
    assert_eq!(
        effective["bindings"][0]["person_id"].as_str(),
        last_person.as_deref()
    );
}
#[tokio::test]
async fn every_event_uses_authenticated_actor_not_body_claim() {
    let (admin, _, w, p, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let person = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, person),
            Some(&t),
            json!({"display_label":"A"}),
        ),
    )
    .await;
    let second = Uuid::new_v4();
    let second_token = authz::provision_token(&admin, second, w, true, true, 7)
        .await
        .unwrap();
    let (s, e1) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&t),
            assign(d, "Ana", person, "first"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(e1["actor_principal_id"], p.to_string());
    let (s, e2) = call(
        r.clone(),
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&second_token),
            clear(d, "Ana", "second"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(e2["actor_principal_id"], second.to_string());
    let mut forged = assign(d, "Ana", person, "forged");
    forged["actor_principal_id"] = json!(p.to_string());
    let (s, _) = call(
        r,
        http(
            "POST",
            &event_url(w, Uuid::new_v4()),
            Some(&second_token),
            forged,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY); // Axum rejects unknown JSON fields
}
#[tokio::test]
async fn identity_api_rejects_nil_ids_and_directional_label_spoofing() {
    let (_, _, w, _, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;

    let (status, _) = call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, Uuid::nil()),
            Some(&t),
            json!({"display_label":"Nobody"})
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let person = Uuid::new_v4();
    let (status, _) = call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, person),
            Some(&t),
            json!({"display_label":"Alice\u{202e}<mallory>"})
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(
        call(
            r.clone(),
            http(
                "POST",
                &identity_url(w, person),
                Some(&t),
                json!({"display_label":"Alice"})
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let (status, _) = call(
        r,
        http(
            "POST",
            &event_url(w, Uuid::nil()),
            Some(&t),
            assign(d, "Ana", person, "confirmed by human"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn malformed_bindings_fail_closed() {
    let (admin, service, w, p, t, r) = fixture().await;
    let d = upload(r.clone(), w, &t).await;
    let person = Uuid::new_v4();
    call(
        r.clone(),
        http(
            "POST",
            &identity_url(w, person),
            Some(&t),
            json!({"display_label":"A"}),
        ),
    )
    .await;
    let (s,_)=call(r.clone(),http("POST",&event_url(w,Uuid::new_v4()),Some(&t),
        json!({"document_id":d,"speaker_label":"Ana","operation":"assign","person_id":null,"human_reason":"bad"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s,_)=call(r.clone(),http("POST",&event_url(w,Uuid::new_v4()),Some(&t),
        json!({"document_id":d,"speaker_label":"Ana","operation":"clear","person_id":person,"human_reason":"bad"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(
        identity::record_event(
            &service,
            w,
            p,
            Uuid::new_v4(),
            d,
            "Ana",
            IdentityAction::Assign,
            None,
            "bad"
        )
        .await
        .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.identity_events WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(count, 0);
}
