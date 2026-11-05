#![cfg(feature = "pg-integration")]
//! Persistent Situation revisions must preserve exactly what a reasoning step could have used.
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
    other_workspace: Uuid,
    principal: Uuid,
    second_principal: Uuid,
    writer: String,
    second_writer: String,
    reader: String,
    document: Uuid,
    evidence: Uuid,
    foreign_document: Uuid,
}

async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let other_workspace = Uuid::new_v4();
    for ws in [workspace, other_workspace] {
        sqlx::query(
            "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'situation-test')",
        )
        .bind(ws)
        .execute(&admin)
        .await
        .unwrap();
    }
    let document = Uuid::new_v4();
    let evidence = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'test://situation','synthetic','REPORTED_SPEECH')")
        .bind(workspace)
        .bind(evidence)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','test://situation-document',$3,16)")
        .bind(workspace)
        .bind(document)
        .bind("a".repeat(64))
        .execute(&admin)
        .await
        .unwrap();
    let foreign_document = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'EML','test://foreign-document',$3,16)")
        .bind(other_workspace)
        .bind(foreign_document)
        .bind("b".repeat(64))
        .execute(&admin)
        .await
        .unwrap();
    let principal = Uuid::new_v4();
    let second_principal = Uuid::new_v4();
    let reader_principal = Uuid::new_v4();
    let writer = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    let second_writer = authz::provision_token(&admin, second_principal, workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, reader_principal, workspace, true, false, 1)
        .await
        .unwrap();
    Setup {
        admin,
        router: api::router(secure),
        workspace,
        other_workspace,
        principal,
        second_principal,
        writer,
        second_writer,
        reader,
        document,
        evidence,
        foreign_document,
    }
}

fn request(method: &str, path: String, token: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap()
}

async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, value)
}

fn situation_path(workspace: Uuid, situation: Uuid) -> String {
    format!("/v1/workspaces/{workspace}/situations/{situation}")
}

async fn create(f: &Setup, situation: Uuid, token: &str, focus: &str) -> (StatusCode, Value) {
    call(
        f.router.clone(),
        request(
            "POST",
            situation_path(f.workspace, situation),
            Some(token),
            Some(json!({"focus":focus})),
        ),
    )
    .await
}

#[tokio::test]
async fn create_is_idempotent_but_conflicting_replay_is_not_a_rewrite() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let (status, first) = create(&f, situation, &f.writer, "Why did Alice object?").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["revision"], 1);
    assert_eq!(first["replayed"], false);
    let fingerprint = first["fingerprint"].clone();

    let (status, replay) = create(&f, situation, &f.writer, "Why did Alice object?").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["revision"], 1);
    assert_eq!(replay["fingerprint"], fingerprint);
    assert_eq!(replay["replayed"], true);

    let (status, _) = create(&f, situation, &f.writer, "Different focus").await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn incremental_source_changes_future_snapshot_not_past_snapshot() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create(&f, situation, &f.writer, "Understand the disagreement").await;
    let source_path = format!(
        "/v1/workspaces/{}/situations/{}/sources/{}",
        f.workspace, situation, f.document
    );
    let (status, added) = call(
        f.router.clone(),
        request("POST", source_path.clone(), Some(&f.writer), None),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(added["revision"], 2);
    assert_eq!(added["source_document_ids"][0], f.document.to_string());

    let rev1_path = format!(
        "/v1/workspaces/{}/situations/{}/revisions/1",
        f.workspace, situation
    );
    let (status, old) = call(
        f.router.clone(),
        request("GET", rev1_path, Some(&f.writer), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(old["source_document_ids"].as_array().unwrap().len(), 0);

    let (status, replay) = call(
        f.router.clone(),
        request("POST", source_path, Some(&f.writer), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["revision"], 2);
    assert_eq!(replay["replayed"], true);

    let foreign = format!(
        "/v1/workspaces/{}/situations/{}/sources/{}",
        f.workspace, situation, f.foreign_document
    );
    assert_eq!(
        call(
            f.router.clone(),
            request("POST", foreign, Some(&f.writer), None)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn user_report_and_what_if_remain_attributed_non_evidence() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create(&f, situation, &f.writer, "What did Roberto know?").await;
    let report = Uuid::new_v4();
    let hypothetical = Uuid::new_v4();
    let report_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, report
    );
    let (status, _) = call(
        f.router.clone(),
        request(
            "POST",
            report_path,
            Some(&f.writer),
            Some(json!({
                "kind":"user_reported",
                "text":"Alice and the director had agreed the previous day",
                "related_evidence_id":f.evidence
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let hypothetical_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, hypothetical
    );
    let (status, current) = call(
        f.router.clone(),
        request(
            "POST",
            hypothetical_path,
            Some(&f.writer),
            Some(json!({
                "kind":"hypothetical",
                "text":"WHAT IF Roberto never saw the email?",
                "related_evidence_id":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(current["revision"], 3);
    let contributions = current["contributions"].as_array().unwrap();
    assert_eq!(contributions.len(), 2);
    let what_if = contributions
        .iter()
        .find(|item| item["contribution_id"] == hypothetical.to_string())
        .unwrap();
    assert_eq!(what_if["kind"], "HYPOTHETICAL");
    assert_eq!(what_if["is_verified_evidence"], false);
    assert_eq!(what_if["author_principal_id"], f.principal.to_string());

    let invalid = Uuid::new_v4();
    let invalid_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, invalid
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                invalid_path,
                Some(&f.writer),
                Some(json!({"kind":"evidence","text":"not allowed","related_evidence_id":null}))
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn correction_and_retraction_preserve_old_snapshots() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create(&f, situation, &f.writer, "When did Alice know?").await;
    let original = Uuid::new_v4();
    let replacement = Uuid::new_v4();
    let original_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, original
    );
    call(
        f.router.clone(),
        request(
            "POST",
            original_path,
            Some(&f.writer),
            Some(json!({"kind":"user_reported","text":"Alice knew on Monday","related_evidence_id":null})),
        ),
    )
    .await;
    let correction_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}/corrections/{}",
        f.workspace, situation, original, replacement
    );
    let (status, corrected) = call(
        f.router.clone(),
        request(
            "POST",
            correction_path.clone(),
            Some(&f.second_writer),
            Some(json!({"text":"Alice anticipated it Monday but learned the final decision Tuesday","related_evidence_id":null})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(corrected["revision"], 3);
    assert_eq!(corrected["contributions"].as_array().unwrap().len(), 1);
    assert_eq!(
        corrected["contributions"][0]["contribution_id"],
        replacement.to_string()
    );
    assert_eq!(
        corrected["contributions"][0]["author_principal_id"],
        f.second_principal.to_string()
    );

    let rev2 = format!(
        "/v1/workspaces/{}/situations/{}/revisions/2",
        f.workspace, situation
    );
    let (_, historical) = call(
        f.router.clone(),
        request("GET", rev2, Some(&f.writer), None),
    )
    .await;
    assert_eq!(
        historical["contributions"][0]["contribution_id"],
        original.to_string()
    );
    assert_eq!(
        historical["contributions"][0]["text"],
        "Alice knew on Monday"
    );

    let (status, replay) = call(
        f.router.clone(),
        request(
            "POST",
            correction_path,
            Some(&f.second_writer),
            Some(json!({"text":"Alice anticipated it Monday but learned the final decision Tuesday","related_evidence_id":null})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["revision"], 3);
    assert_eq!(replay["replayed"], true);

    let retract_path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}/retract",
        f.workspace, situation, replacement
    );
    let (status, retracted) = call(
        f.router.clone(),
        request("POST", retract_path.clone(), Some(&f.writer), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retracted["revision"], 4);
    assert!(retracted["contributions"].as_array().unwrap().is_empty());
    let (status, replay) = call(
        f.router.clone(),
        request("POST", retract_path, Some(&f.writer), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["revision"], 4);
    assert_eq!(replay["replayed"], true);
}

#[tokio::test]
async fn focus_change_is_a_revision_and_never_rewrites_revision_one() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let (_, created) = create(&f, situation, &f.writer, "Inspect the silence").await;
    let initial_fingerprint = created["fingerprint"].clone();
    let focus_path = format!(
        "/v1/workspaces/{}/situations/{}/focus",
        f.workspace, situation
    );
    let (status, changed) = call(
        f.router.clone(),
        request(
            "POST",
            focus_path.clone(),
            Some(&f.writer),
            Some(json!({"focus":"Inspect unfamiliar evening participants"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(changed["revision"], 2);
    assert_ne!(changed["fingerprint"], initial_fingerprint);
    let (_, replay) = call(
        f.router.clone(),
        request(
            "POST",
            focus_path,
            Some(&f.writer),
            Some(json!({"focus":"Inspect unfamiliar evening participants"})),
        ),
    )
    .await;
    assert_eq!(replay["revision"], 2);
    assert_eq!(replay["replayed"], true);
    let rev1 = format!(
        "/v1/workspaces/{}/situations/{}/revisions/1",
        f.workspace, situation
    );
    let (_, historical) = call(
        f.router.clone(),
        request("GET", rev1, Some(&f.writer), None),
    )
    .await;
    assert_eq!(historical["focus"], "Inspect the silence");
}

#[tokio::test]
async fn read_only_member_can_inspect_but_cannot_steer() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create(&f, situation, &f.writer, "Read boundary").await;
    let (status, _) = call(
        f.router.clone(),
        request(
            "GET",
            situation_path(f.workspace, situation),
            Some(&f.reader),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let contribution = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, contribution
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                path.clone(),
                Some(&f.reader),
                Some(
                    json!({"kind":"user_reported","text":"cannot write","related_evidence_id":null})
                )
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                path,
                None,
                Some(
                    json!({"kind":"user_reported","text":"cannot write","related_evidence_id":null})
                )
            )
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn concurrent_identical_contribution_creates_one_revision() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create(&f, situation, &f.writer, "Concurrent steering").await;
    let contribution = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/situations/{}/contributions/{}",
        f.workspace, situation, contribution
    );
    let mut tasks = Vec::new();
    for _ in 0..24 {
        let router = f.router.clone();
        let token = f.writer.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            call(
                router,
                request(
                    "POST",
                    path,
                    Some(&token),
                    Some(json!({"kind":"user_reported","text":"one contribution","related_evidence_id":null})),
                ),
            )
            .await
        }));
    }
    let mut created = 0;
    let mut replayed = 0;
    for task in tasks {
        let (status, value) = task.await.unwrap();
        match status {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => replayed += 1,
            other => panic!("unexpected status {other}"),
        }
        assert!(value["revision"] == 2 || value["revision"] == 1);
    }
    assert_eq!(created, 1);
    assert_eq!(replayed, 23);
    let (_, current) = call(
        f.router.clone(),
        request(
            "GET",
            situation_path(f.workspace, situation),
            Some(&f.writer),
            None,
        ),
    )
    .await;
    assert_eq!(current["revision"], 2);
    assert_eq!(current["contributions"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn revision_history_is_bounded_paginated_and_actor_attributed() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create(&f, situation, &f.writer, "Revision page").await;
    for index in 0..5 {
        let contribution = Uuid::new_v4();
        let path = format!(
            "/v1/workspaces/{}/situations/{}/contributions/{}",
            f.workspace, situation, contribution
        );
        call(
            f.router.clone(),
            request(
                "POST",
                path,
                Some(&f.writer),
                Some(json!({"kind":"investigative_preference","text":format!("preference {index}"),"related_evidence_id":null})),
            ),
        )
        .await;
    }
    let first = format!(
        "/v1/workspaces/{}/situations/{}/revisions?limit=2",
        f.workspace, situation
    );
    let (_, page1) = call(
        f.router.clone(),
        request("GET", first, Some(&f.writer), None),
    )
    .await;
    assert_eq!(page1["revisions"].as_array().unwrap().len(), 2);
    assert_eq!(page1["revisions"][0]["revision"], 1);
    assert_eq!(
        page1["revisions"][0]["actor_principal_id"],
        f.principal.to_string()
    );
    let after = page1["next_after"].as_i64().unwrap();
    let second = format!(
        "/v1/workspaces/{}/situations/{}/revisions?limit=2&after={after}",
        f.workspace, situation
    );
    let (_, page2) = call(
        f.router.clone(),
        request("GET", second, Some(&f.writer), None),
    )
    .await;
    assert_eq!(page2["revisions"][0]["revision"], 3);
    assert_ne!(
        page1["revisions"][1]["revision"],
        page2["revisions"][0]["revision"]
    );
}

#[tokio::test]
async fn new_situation_tables_are_forced_rls_and_have_all_api_gates() {
    let f = setup().await;
    let rows: Vec<(String, bool, bool, i64)> = sqlx::query_as(
        "SELECT c.relname,c.relrowsecurity,c.relforcerowsecurity, \
         (SELECT count(*) FROM pg_policy p WHERE p.polrelid=c.oid AND NOT p.polpermissive \
           AND p.polname IN ('api_read_gate','api_write_insert_gate','api_write_update_gate','api_write_delete_gate') \
           AND 'niccolo_api'::regrole::oid=ANY(p.polroles)) \
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname='niccolo' AND c.relname LIKE 'situation%' AND c.relkind='r' ORDER BY c.relname",
    )
    .fetch_all(&f.admin)
    .await
    .unwrap();
    let names = rows
        .iter()
        .map(|(name, ..)| name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let expected = [
        "situation_analysis_clarification_answers",
        "situation_analysis_clarification_questions",
        "situation_analysis_examined_fragments",
        "situation_analysis_outputs",
        "situation_analysis_snapshots",
        "situation_analysis_used_contributions",
        "situation_contributions",
        "situation_revision_contributions",
        "situation_revision_sources",
        "situation_revisions",
        "situation_sources",
        "situations",
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, expected);
    for (name, rls, forced, gates) in rows {
        assert!(rls, "{name} RLS disabled");
        assert!(forced, "{name} FORCE RLS disabled");
        assert_eq!(gates, 4, "{name} missing restrictive API gate");
    }
    assert_ne!(f.workspace, f.other_workspace);
}
