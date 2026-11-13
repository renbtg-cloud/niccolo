#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    analysis_structure::AnalysisScope, api, authz, human_feedback, human_handoff,
    investigation_policy,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Setup {
    admin: PgPool,
    router: Router,
    workspace: Uuid,
    writer_principal: Uuid,
    writer: String,
    reader: String,
}

async fn setup() -> Setup {
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
    let workspace = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'clarification-answer-test')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let writer_principal = Uuid::new_v4();
    let writer = authz::provision_token(&admin, writer_principal, workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Setup {
        admin,
        router: api::router(secure),
        workspace,
        writer_principal,
        writer,
        reader,
    }
}
fn req(method: &str, path: String, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}
async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}
fn sit(f: &Setup, s: Uuid) -> String {
    format!("/v1/workspaces/{}/situations/{s}", f.workspace)
}
fn analysis(f: &Setup, s: Uuid, a: Uuid) -> String {
    format!("{}/analysis-snapshots/{a}", sit(f, s))
}
fn question(f: &Setup, s: Uuid, a: Uuid, q: Uuid) -> String {
    format!("{}/clarifications/{q}", analysis(f, s, a))
}
async fn seed(f: &Setup, s: Uuid, a: Uuid, q: Uuid) {
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                sit(f, s),
                &f.writer,
                Some(json!({"focus":"Understand the disagreement"}))
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
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                question(f, s, a, q),
                &f.writer,
                Some(json!({"text":"Had Alice seen it?","reason":"Timeline matters"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
}
#[tokio::test]
async fn answer_becomes_new_user_reported_context_not_old_input_or_evidence() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    let q = Uuid::new_v4();
    let ans = Uuid::new_v4();
    seed(&f, s, a, q).await;
    let path = format!("{}/answers/{ans}", question(&f, s, a, q));
    let (status, v) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            &f.writer,
            Some(json!({"text":"Yes, she had already seen it."})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(v["kind"], "USER_REPORTED");
    assert_eq!(v["is_evidence"], false);
    assert_eq!(v["question_input_revision"], 1);
    assert_eq!(v["answer_revision"], 2);
    assert_eq!(v["answer_is_in_original_analysis_input"], false);
    let snapshot = call(f.router.clone(), req("GET", sit(&f, s), &f.reader, None))
        .await
        .1;
    assert_eq!(snapshot["revision"], 2);
    assert_eq!(
        snapshot["contributions"][0]["contribution_id"],
        ans.to_string()
    );
    assert_eq!(snapshot["contributions"][0]["kind"], "USER_REPORTED");
    let frozen = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}/input", analysis(&f, s, a)),
            &f.reader,
            None,
        ),
    )
    .await
    .1;
    assert_eq!(frozen["input"]["revision"], 1);
    assert_eq!(
        frozen["input"]["contributions"].as_array().unwrap().len(),
        0
    );
}
#[tokio::test]
async fn answered_clarification_disappears_from_proactive_queue() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    let q = Uuid::new_v4();
    seed(&f, s, a, q).await;
    let secure = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let scope = AnalysisScope {
        workspace: f.workspace,
        situation: s,
        analysis: a,
    };
    let digest = authz::credential_digest(&f.reader).unwrap();
    let before = authz::with_auth_digest(
        digest.clone(),
        investigation_policy::queue(&secure, scope, 3),
    )
    .await
    .unwrap();
    assert!(before.items.iter().any(|lead| lead.id == q));

    let answer_path = format!("{}/answers/{}", question(&f, s, a, q), Uuid::new_v4());
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                answer_path,
                &f.writer,
                Some(json!({"text":"I do not know yet."}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let after =
        authz::with_auth_digest(digest, investigation_policy::queue(&secure, scope, 3))
            .await
            .unwrap();
    assert!(!after.items.iter().any(|lead| lead.id == q));
    assert_eq!(after.candidate_count, 0);
}

#[tokio::test]
async fn deferred_source_request_is_suppressed_until_explicitly_reopened() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    seed(&f, s, a, Uuid::new_v4()).await;
    let secure = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let scope = AnalysisScope {
        workspace: f.workspace,
        situation: s,
        analysis: a,
    };
    let request = Uuid::new_v4();
    let digest = authz::credential_digest(&f.writer).unwrap();
    authz::with_auth_digest(digest, async {
        human_handoff::propose_source_request(
            &secure,
            scope,
            &human_handoff::SourceRequestInput {
                request_id: request,
                text: "Ask for the contemporaneous source".into(),
                source_kind: "EMAIL".into(),
                rationale: "May discriminate competing interpretations".into(),
                proposed_by: f.writer_principal,
            },
        )
        .await
        .unwrap();
        let before = investigation_policy::queue(&secure, scope, 3).await.unwrap();
        assert!(before.items.iter().any(|lead| lead.id == request));
        human_handoff::append_source_request_event(
            &secure,
            scope,
            request,
            &human_handoff::SourceRequestEvent {
                event_id: Uuid::new_v4(),
                kind: "DEFERRED".into(),
                note: "Human explicitly postponed this investigation step".into(),
                recorded_by: f.writer_principal,
            },
        )
        .await
        .unwrap();
        let deferred = investigation_policy::queue(&secure, scope, 3).await.unwrap();
        assert!(!deferred.items.iter().any(|lead| lead.id == request));
        human_handoff::append_source_request_event(
            &secure,
            scope,
            request,
            &human_handoff::SourceRequestEvent {
                event_id: Uuid::new_v4(),
                kind: "REOPENED".into(),
                note: "Human explicitly resumed this investigation step".into(),
                recorded_by: f.writer_principal,
            },
        )
        .await
        .unwrap();
        let reopened = investigation_policy::queue(&secure, scope, 3).await.unwrap();
        assert!(reopened.items.iter().any(|lead| lead.id == request));
    })
    .await;
}

#[tokio::test]
async fn rejected_clarification_respects_latest_feedback_and_actor_disagreement() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    let q = Uuid::new_v4();
    seed(&f, s, a, q).await;
    let secure = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let scope = AnalysisScope {
        workspace: f.workspace,
        situation: s,
        analysis: a,
    };
    let second_actor = Uuid::new_v4();
    let second_token =
        authz::provision_token(&f.admin, second_actor, f.workspace, true, true, 1)
            .await
            .unwrap();
    let first_digest = authz::credential_digest(&f.writer).unwrap();
    let second_digest = authz::credential_digest(&second_token).unwrap();
    let initial =
        authz::with_auth_digest(first_digest.clone(), investigation_policy::queue(&secure, scope, 3))
            .await
            .unwrap();
    assert!(initial.items.iter().any(|lead| lead.id == q));

    authz::with_auth_digest(
        first_digest.clone(),
        human_feedback::record_feedback(
            &secure,
            f.workspace,
            f.writer_principal,
            Uuid::new_v4(),
            q,
            human_feedback::HumanResponse::Rejected,
            "I reject that question for now",
        ),
    )
    .await
    .unwrap();
    let rejected =
        authz::with_auth_digest(first_digest.clone(), investigation_policy::queue(&secure, scope, 3))
            .await
            .unwrap();
    assert!(!rejected.items.iter().any(|lead| lead.id == q));

    authz::with_auth_digest(
        second_digest.clone(),
        human_feedback::record_feedback(
            &secure,
            f.workspace,
            second_actor,
            Uuid::new_v4(),
            q,
            human_feedback::HumanResponse::Unknown,
            "I do not know the answer",
        ),
    )
    .await
    .unwrap();
    let contested =
        authz::with_auth_digest(first_digest.clone(), investigation_policy::queue(&secure, scope, 3))
            .await
            .unwrap();
    assert!(contested.items.iter().any(|lead| lead.id == q));

    authz::with_auth_digest(
        second_digest,
        human_feedback::record_feedback(
            &secure,
            f.workspace,
            second_actor,
            Uuid::new_v4(),
            q,
            human_feedback::HumanResponse::Rejected,
            "I also reject it after reconsideration",
        ),
    )
    .await
    .unwrap();
    let unanimous =
        authz::with_auth_digest(first_digest.clone(), investigation_policy::queue(&secure, scope, 3))
            .await
            .unwrap();
    assert!(!unanimous.items.iter().any(|lead| lead.id == q));

    authz::with_auth_digest(
        first_digest.clone(),
        human_feedback::record_feedback(
            &secure,
            f.workspace,
            f.writer_principal,
            Uuid::new_v4(),
            q,
            human_feedback::HumanResponse::InsufficientEvidence,
            "I now want to investigate missing evidence",
        ),
    )
    .await
    .unwrap();
    let reopened =
        authz::with_auth_digest(first_digest, investigation_policy::queue(&secure, scope, 3))
            .await
            .unwrap();
    assert!(reopened.items.iter().any(|lead| lead.id == q));
}

#[tokio::test]
async fn answer_retry_is_idempotent_but_mutation_conflicts() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    let q = Uuid::new_v4();
    let ans = Uuid::new_v4();
    seed(&f, s, a, q).await;
    let path = format!("{}/answers/{ans}", question(&f, s, a, q));
    let body = json!({"text":"No."});
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path.clone(), &f.writer, Some(body.clone()))
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, replay) = call(
        f.router.clone(),
        req("POST", path.clone(), &f.writer, Some(body)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path,
                &f.writer,
                Some(json!({"text":"Changed answer"}))
            )
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn old_question_can_be_answered_later_without_rewriting_its_origin() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    let q = Uuid::new_v4();
    seed(&f, s, a, q).await;
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/focus", sit(&f, s)),
                &f.writer,
                Some(json!({"focus":"Inspect Tuesday instead"}))
            )
        )
        .await
        .0,
        StatusCode::OK
    );
    let ans = Uuid::new_v4();
    let v = call(
        f.router.clone(),
        req(
            "POST",
            format!("{}/answers/{ans}", question(&f, s, a, q)),
            &f.writer,
            Some(json!({"text":"I learned this only today."})),
        ),
    )
    .await
    .1;
    assert_eq!(v["question_input_revision"], 1);
    assert_eq!(v["answer_revision"], 3);
    assert_eq!(v["current_situation_revision"], 3);
    let list = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}/answers", question(&f, s, a, q)),
            &f.reader,
            None,
        ),
    )
    .await
    .1;
    assert_eq!(list["answers_do_not_rewrite_frozen_analysis_input"], true);
    assert_eq!(list["answers"][0]["answer_id"], ans.to_string());
}
#[tokio::test]
async fn reader_cannot_answer_and_concurrent_retry_creates_one_binding() {
    let f = setup().await;
    let s = Uuid::new_v4();
    let a = Uuid::new_v4();
    let q = Uuid::new_v4();
    let ans = Uuid::new_v4();
    seed(&f, s, a, q).await;
    let path = format!("{}/answers/{ans}", question(&f, s, a, q));
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                path.clone(),
                &f.reader,
                Some(json!({"text":"No write"}))
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut joins = Vec::new();
    for _ in 0..12 {
        let r = f.router.clone();
        let p = path.clone();
        let t = f.writer.clone();
        joins.push(tokio::spawn(async move {
            call(r, req("POST", p, &t, Some(json!({"text":"Stable answer"}))))
                .await
                .0
        }));
    }
    let mut created = 0;
    for j in joins {
        let st = j.await.unwrap();
        assert!(matches!(st, StatusCode::CREATED | StatusCode::OK));
        if st == StatusCode::CREATED {
            created += 1;
        }
    }
    assert_eq!(created, 1);
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_clarification_answers WHERE workspace_id=$1 AND situation_id=$2 AND answer_id=$3").bind(f.workspace).bind(s).bind(ans).fetch_one(&f.admin).await.unwrap();
    assert_eq!(count, 1);
}
