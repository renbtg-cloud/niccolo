#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    api, authz,
    human_feedback::{self, HumanResponse},
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

async fn fixture() -> (PgPool, PgPool, Uuid, Uuid, String, Router) {
    let a = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let s = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let w = Uuid::new_v4();
    let p = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'feedback-fixture')")
        .bind(w)
        .execute(&a)
        .await
        .unwrap();
    let t = authz::provision_token(&a, p, w, true, true, 7)
        .await
        .unwrap();
    let router = api::router(s.clone());
    (a, s, w, p, t, router)
}
fn req(method: &str, path: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut q = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        q = q.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    q.body(Body::from(body.to_string())).unwrap()
}
async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(json!({})))
}
fn question(w: Uuid, q: Uuid) -> String {
    format!("/v1/workspaces/{w}/questions/{q}")
}
fn feedback(w: Uuid, q: Uuid, e: Uuid) -> String {
    format!("/v1/workspaces/{w}/questions/{q}/feedback/{e}")
}
fn history(w: Uuid, q: Uuid) -> String {
    format!("/v1/workspaces/{w}/questions/{q}/feedback")
}
async fn make_question(router: Router, w: Uuid, t: &str) -> Uuid {
    let q = Uuid::new_v4();
    assert_eq!(
        call(
            router,
            req(
                "POST",
                &question(w, q),
                Some(t),
                json!({"text":"What independent source could confirm this account?"})
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    q
}
fn response(name: &str, reason: &str) -> Value {
    json!({"response":name,"human_reason":reason})
}
#[tokio::test]
async fn question_create_replay_and_conflict() {
    let (_, _, w, _, t, r) = fixture().await;
    let q = Uuid::new_v4();
    let url = question(w, q);
    let body = json!({"text":"Which primary source should be sought?"});
    assert_eq!(
        call(r.clone(), req("POST", &url, Some(&t), body.clone()))
            .await
            .0,
        StatusCode::CREATED
    );
    let (status, again) = call(r.clone(), req("POST", &url, Some(&t), body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["replayed"], true);
    assert_eq!(
        call(
            r,
            req("POST", &url, Some(&t), json!({"text":"Different question"}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn explicitly_distinguishes_three_responses_and_does_not_claim_evidence() {
    let (_, _, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let choices = ["rejected", "insufficient_evidence", "unknown"];
    for choice in choices {
        let e = Uuid::new_v4();
        let (status, body) = call(
            r.clone(),
            req(
                "POST",
                &feedback(w, q, e),
                Some(&t),
                response(choice, "explicit human response"),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["is_evidence"], false);
    }
    let (_, body) = call(r, req("GET", &history(w, q), Some(&t), json!({}))).await;
    assert_eq!(body["events"].as_array().unwrap().len(), 3);
    assert_eq!(body["events"][0]["response"], "REJECTED");
    assert_eq!(body["events"][1]["response"], "INSUFFICIENT_EVIDENCE");
    assert_eq!(body["events"][2]["response"], "UNKNOWN");
}
#[tokio::test]
async fn repeated_event_replays_only_if_actor_and_payload_match() {
    let (_, _, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let e = Uuid::new_v4();
    let url = feedback(w, q, e);
    let first = response("unknown", "Need user clarification");
    assert_eq!(
        call(r.clone(), req("POST", &url, Some(&t), first.clone()))
            .await
            .0,
        StatusCode::CREATED
    );
    let (s, replayed) = call(r.clone(), req("POST", &url, Some(&t), first)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(
        call(
            r.clone(),
            req(
                "POST",
                &url,
                Some(&t),
                response("rejected", "Need user clarification")
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            r,
            req(
                "POST",
                &url,
                Some(&t),
                response("unknown", "Another reason")
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn write_needs_membership_and_readonly_token_cannot_add_feedback() {
    let (a, _, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let other = Uuid::new_v4();
    let read = authz::provision_token(&a, other, w, true, false, 7)
        .await
        .unwrap();
    assert_eq!(
        call(
            r.clone(),
            req(
                "POST",
                &feedback(w, q, Uuid::new_v4()),
                Some(&read),
                response("unknown", "not authorized")
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            r.clone(),
            req(
                "POST",
                &feedback(w, q, Uuid::new_v4()),
                None,
                response("unknown", "not authenticated")
            )
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(r, req("GET", &history(w, q), Some(&read), json!({})))
            .await
            .0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn author_is_authenticated_principal_not_user_supplied() {
    let (_, _, w, p, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let e = Uuid::new_v4();
    let injected =
        json!({"response":"rejected","human_reason":"no","actor_principal_id":Uuid::new_v4()});
    assert_eq!(
        call(
            r.clone(),
            req("POST", &feedback(w, q, e), Some(&t), injected)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (s, data) = call(
        r,
        req(
            "POST",
            &feedback(w, q, e),
            Some(&t),
            response("rejected", "human rejected"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(data["actor_principal_id"], p.to_string());
}
#[tokio::test]
async fn cross_workspace_questions_and_feedback_are_invisible() {
    let (_, _, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let (_, _, other, _, othert, _) = fixture().await;
    assert_eq!(
        call(
            r.clone(),
            req(
                "POST",
                &feedback(other, q, Uuid::new_v4()),
                Some(&othert),
                response("unknown", "wrong workspace")
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            r.clone(),
            req("GET", &history(other, q), Some(&othert), json!({}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            r.clone(),
            req("GET", &history(w, q), Some(&othert), json!({}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(r, req("GET", &history(w, q), Some(&t), json!({})))
            .await
            .0,
        StatusCode::OK
    );
}
#[tokio::test]
async fn invalid_responses_and_controls_are_rejected_without_persistence() {
    let (_, _, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    for choice in ["", "uncertain", "INSUFFICIENT_EVIDENCE", "yes"] {
        assert_eq!(
            call(
                r.clone(),
                req(
                    "POST",
                    &feedback(w, q, Uuid::new_v4()),
                    Some(&t),
                    response(choice, "reason")
                )
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for reason in ["", "\n", "hello\ntext"] {
        assert_eq!(
            call(
                r.clone(),
                req(
                    "POST",
                    &feedback(w, q, Uuid::new_v4()),
                    Some(&t),
                    response("unknown", reason)
                )
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        call(r, req("GET", &history(w, q), Some(&t), json!({})))
            .await
            .1["events"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
#[tokio::test]
async fn question_list_keyset_pagination_has_no_duplicates() {
    let (_, _, w, _, t, r) = fixture().await;
    for _ in 0..5 {
        make_question(r.clone(), w, &t).await;
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut after = None;
    loop {
        let url = match after {
            Some(id) => format!("/v1/workspaces/{w}/questions?limit=2&after={id}"),
            None => format!("/v1/workspaces/{w}/questions?limit=2"),
        };
        let (s, page) = call(r.clone(), req("GET", &url, Some(&t), json!({}))).await;
        assert_eq!(s, StatusCode::OK);
        for q in page["questions"].as_array().unwrap() {
            assert!(seen.insert(q["question_id"].as_str().unwrap().to_owned()));
            assert_eq!(q["is_evidence"], false);
        }
        after = page["next_after"].as_str().map(|x| x.to_owned());
        if after.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 5);
}
#[tokio::test]
async fn feedback_history_keyset_pages_in_event_sequence() {
    let (_, _, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    for n in 0..5 {
        assert_eq!(
            call(
                r.clone(),
                req(
                    "POST",
                    &feedback(w, q, Uuid::new_v4()),
                    Some(&t),
                    response("unknown", &format!("reason {n}"))
                )
            )
            .await
            .0,
            StatusCode::CREATED
        );
    }
    let mut seen = Vec::new();
    let mut after = None;
    loop {
        let url = match after {
            Some(n) => format!("{}?limit=2&after={n}", history(w, q)),
            None => format!("{}?limit=2", history(w, q)),
        };
        let (s, page) = call(r.clone(), req("GET", &url, Some(&t), json!({}))).await;
        assert_eq!(s, StatusCode::OK);
        for event in page["events"].as_array().unwrap() {
            seen.push(event["event_no"].as_i64().unwrap());
        }
        after = page["next_after"].as_i64();
        if after.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 5);
    assert!(seen.windows(2).all(|pair| pair[0] < pair[1]));
}
#[tokio::test]
async fn direct_sql_cannot_update_or_delete_feedback() {
    let (_, service, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let e = Uuid::new_v4();
    assert_eq!(
        call(
            r,
            req(
                "POST",
                &feedback(w, q, e),
                Some(&t),
                response("rejected", "no evidence")
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let mut tx = service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let result=sqlx::query("UPDATE niccolo.question_feedback_events SET response='UNKNOWN' WHERE workspace_id=$1 AND event_id=$2")
        .bind(w).bind(e).execute(&mut *tx).await;
    assert!(result.is_err());
    tx.rollback().await.unwrap();
    let mut tx = service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let result = sqlx::query(
        "DELETE FROM niccolo.question_feedback_events WHERE workspace_id=$1 AND event_id=$2",
    )
    .bind(w)
    .bind(e)
    .execute(&mut *tx)
    .await;
    assert!(result.is_err());
    tx.rollback().await.unwrap();
}
#[tokio::test]
async fn fifty_parallel_replays_produce_one_entry() {
    let (_, s, w, p, t, r) = fixture().await;
    let q = make_question(r, w, &t).await;
    let e = Uuid::new_v4();
    let mut handles = Vec::new();
    for _ in 0..50 {
        let pool = s.clone();
        handles.push(tokio::spawn(async move {
            human_feedback::record_feedback(
                &pool,
                w,
                p,
                e,
                q,
                HumanResponse::InsufficientEvidence,
                "await original source",
            )
            .await
            .unwrap()
        }));
    }
    let mut new = 0;
    let mut replayed = 0;
    for h in handles {
        if h.await.unwrap().replayed {
            replayed += 1
        } else {
            new += 1
        }
    }
    assert_eq!((new, replayed), (1, 49));
    let (entries, _) = human_feedback::feedback_history(&s, w, q, 0, 100)
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
}
#[tokio::test]
async fn foreign_actor_replaying_another_actor_event_is_conflict() {
    let (a, s, w, p, t, r) = fixture().await;
    let q = make_question(r, w, &t).await;
    let e = Uuid::new_v4();
    let other = Uuid::new_v4();
    let _other_token = authz::provision_token(&a, other, w, true, true, 7)
        .await
        .unwrap();
    assert!(
        !human_feedback::record_feedback(&s, w, p, e, q, HumanResponse::Unknown, "open question")
            .await
            .unwrap()
            .replayed
    );
    assert!(matches!(
        human_feedback::record_feedback(
            &s,
            w,
            other,
            e,
            q,
            HumanResponse::Unknown,
            "open question"
        )
        .await,
        Err(human_feedback::FeedbackError::Conflict)
    ));
}
#[tokio::test]
async fn db_rejects_cross_workspace_foreign_key_even_when_both_visible_to_admin() {
    let (a, s, w, _, t, r) = fixture().await;
    let q = make_question(r, w, &t).await;
    let (_, _, other, _, _, _) = fixture().await;
    // The restricted role cannot forge workspace-authoritative constraints by inserting another workspace's question.
    let mut tx = s.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(other.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let inserted=sqlx::query("INSERT INTO niccolo.question_feedback_events(workspace_id,event_id,question_id,response,human_reason,actor_principal_id) VALUES($1,$2,$3,'UNKNOWN','test',$4)")
        .bind(other).bind(Uuid::new_v4()).bind(q).bind(Uuid::new_v4()).execute(&mut *tx).await;
    assert!(inserted.is_err());
    tx.rollback().await.unwrap();
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.question_feedback_events WHERE workspace_id=$1 AND question_id=$2")
        .bind(other).bind(q).fetch_one(&a).await.unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn privileged_db_connection_is_rejected_for_api_repository_methods() {
    let (admin, _, w, p, _, _) = fixture().await;
    assert!(matches!(
        human_feedback::create_question(&admin, w, Uuid::new_v4(), "New question").await,
        Err(human_feedback::FeedbackError::Privileged)
    ));
    assert!(matches!(
        human_feedback::record_feedback(
            &admin,
            w,
            p,
            Uuid::new_v4(),
            Uuid::new_v4(),
            HumanResponse::Rejected,
            "never"
        )
        .await,
        Err(human_feedback::FeedbackError::Privileged)
    ));
}
#[tokio::test]
async fn independent_people_disagree_without_false_consensus() {
    let (admin, service, w, p, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    let other = Uuid::new_v4();
    let _other_t = authz::provision_token(&admin, other, w, true, true, 7)
        .await
        .unwrap();
    for (actor, reply) in [
        (p, HumanResponse::Rejected),
        (other, HumanResponse::Unknown),
    ] {
        human_feedback::record_feedback(
            &service,
            w,
            actor,
            Uuid::new_v4(),
            q,
            reply,
            "personal response",
        )
        .await
        .unwrap();
    }
    let url = format!("/v1/workspaces/{w}/questions/{q}/positions");
    let (status, body) = call(r.clone(), req("GET", &url, Some(&t), json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["consensus_established"], false);
    let choices: std::collections::BTreeSet<_> = body["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["response"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        choices,
        std::collections::BTreeSet::from(["REJECTED".to_owned(), "UNKNOWN".to_owned()])
    );
}
#[tokio::test]
async fn actor_can_change_position_without_erasing_previous_feedback() {
    let (_, service, w, p, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    for resp in [
        HumanResponse::Rejected,
        HumanResponse::InsufficientEvidence,
        HumanResponse::Unknown,
    ] {
        human_feedback::record_feedback(
            &service,
            w,
            p,
            Uuid::new_v4(),
            q,
            resp,
            "new evidence came in",
        )
        .await
        .unwrap();
    }
    let (_, positions) = call(
        r.clone(),
        req(
            "GET",
            &format!("/v1/workspaces/{w}/questions/{q}/positions"),
            Some(&t),
            json!({}),
        ),
    )
    .await;
    assert_eq!(positions["positions"].as_array().unwrap().len(), 1);
    assert_eq!(positions["positions"][0]["response"], "UNKNOWN");
    let (_, events) = call(r, req("GET", &history(w, q), Some(&t), json!({}))).await;
    assert_eq!(events["events"].as_array().unwrap().len(), 3);
}
#[tokio::test]
async fn latest_positions_are_paginated_by_actor() {
    let (admin, service, w, _, t, r) = fixture().await;
    let q = make_question(r.clone(), w, &t).await;
    for _ in 0..5 {
        let p = Uuid::new_v4();
        authz::provision_token(&admin, p, w, true, true, 1)
            .await
            .unwrap();
        human_feedback::record_feedback(
            &service,
            w,
            p,
            Uuid::new_v4(),
            q,
            HumanResponse::Unknown,
            "no answer",
        )
        .await
        .unwrap();
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut after = None;
    loop {
        let uri = match after {
            Some(x) => format!("/v1/workspaces/{w}/questions/{q}/positions?limit=2&after={x}"),
            None => format!("/v1/workspaces/{w}/questions/{q}/positions?limit=2"),
        };
        let (status, data) = call(r.clone(), req("GET", &uri, Some(&t), json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        for value in data["positions"].as_array().unwrap() {
            assert!(seen.insert(value["actor_principal_id"].as_str().unwrap().to_owned()));
        }
        after = data["next_after"].as_str().map(str::to_owned);
        if after.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 5);
}
