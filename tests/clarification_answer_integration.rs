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

struct Setup {
    admin: PgPool,
    router: Router,
    workspace: Uuid,
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
    let writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Setup {
        admin,
        router: api::router(secure),
        workspace,
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

#[tokio::test]
async fn live_lead_returns_only_after_the_current_answer_is_retracted() {
    let f = setup().await;
    let (s, a, q, answer, replacement) = (
        Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
    );
    seed(&f, s, a, q).await;
    let dashboard = format!("{}/bff/dashboard", analysis(&f, s, a));
    let lead_count = || async {
        let (status, result) = call(f.router.clone(), req("GET", dashboard.clone(), &f.reader, None)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        result["proactive"]["leads"].as_array().unwrap().len()
    };
    assert_eq!(lead_count().await, 1);
    let (status, _) = call(f.router.clone(), req(
        "POST", format!("{}/answers/{answer}", question(&f, s, a, q)),
        &f.writer, Some(json!({"text":"I saw the final decision on Tuesday."})),
    )).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(lead_count().await, 0);

    let (status, _) = call(f.router.clone(), req(
        "POST", format!("{}/contributions/{answer}/corrections/{replacement}", sit(&f, s)),
        &f.writer, Some(json!({"text":"I saw the final decision on Wednesday."})),
    )).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(lead_count().await, 0);

    let (status, _) = call(f.router.clone(), req(
        "POST", format!("{}/contributions/{replacement}/retract", sit(&f, s)),
        &f.writer, Some(json!({})),
    )).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lead_count().await, 1);
}

#[tokio::test]
async fn source_request_decisions_are_write_authorized_idempotent_and_do_not_fetch() {
    let f = setup().await;
    let (s, a, q, request_id, event_id) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    seed(&f, s, a, q).await;
    let principal: Uuid = sqlx::query_scalar(
        "SELECT created_by_principal_id FROM niccolo.situations \
         WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(f.workspace)
    .bind(s)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_source_requests \
         (workspace_id,situation_id,analysis_id,request_id,request_text,source_kind,rationale,proposed_by_principal_id) \
         VALUES($1,$2,$3,$4,'Obtain the original exchange','message','Resolve the timeline',$5)",
    )
    .bind(f.workspace)
    .bind(s)
    .bind(a)
    .bind(request_id)
    .bind(principal)
    .execute(&f.admin)
    .await
    .unwrap();
    let list_path = format!("{}/source-requests", analysis(&f, s, a));
    let (status, listed) = call(
        f.router.clone(),
        req("GET", list_path.clone(), &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["source_requests"][0]["request_id"], request_id.to_string());
    assert_eq!(listed["source_requests"][0]["state"], "PROPOSED");
    assert_eq!(listed["human_owns_external_action"], true);
    assert_eq!(listed["acceptance_fetches_external_material"], false);
    let path = format!(
        "{}/source-requests/{request_id}/events/{event_id}",
        analysis(&f, s, a)
    );
    let body = json!({"kind":"DEFERRED","note":"I will revisit this tomorrow."});
    assert_eq!(
        call(
            f.router.clone(),
            req("POST", path.clone(), &f.reader, Some(body.clone()))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    for (expected, replayed) in [
        (StatusCode::CREATED, false),
        (StatusCode::OK, true),
    ] {
        let (status, result) = call(
            f.router.clone(),
            req("POST", path.clone(), &f.writer, Some(body.clone())),
        )
        .await;
        assert_eq!(status, expected, "{result}");
        assert_eq!(result["replayed"], replayed);
        assert_eq!(result["accepted_means_fetched"], false);
        assert_eq!(result["system_fetched_external_material"], false);
    }
    let (status, listed) = call(f.router.clone(), req("GET", list_path, &f.reader, None)).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["source_requests"][0]["state"], "DEFERRED");
    assert_eq!(listed["source_requests"][0]["events"][0]["event_id"], event_id.to_string());
    assert_eq!(listed["source_requests"][0]["events"][0]["kind"], "DEFERRED");
    assert_eq!(listed["source_requests"][0]["events"][0]["note"], "I will revisit this tomorrow.");
    assert!(listed["source_requests"][0]["events"][0]["recorded_by"].is_string());
    assert!(listed["source_requests"][0]["events"][0]["recorded_at"].is_string());
    assert_eq!(listed["source_requests"][0]["accepted_means_fetched"], false);
    let (status, conflict) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            &f.writer,
            Some(json!({"kind":"DECLINED","note":"conflicting replay"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
}
