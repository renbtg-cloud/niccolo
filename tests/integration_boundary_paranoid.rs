#![cfg(feature = "pg-integration")]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    api, authz, postgres_ingest::ingest_source_with_operational_id,
    postgres_repository::load_workspace_graph, source_import::SourceFormat,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Setup {
    admin: PgPool,
    service: PgPool,
    router: Router,
    workspace: Uuid,
    writer: String,
}

async fn setup() -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").expect("admin DB"))
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").expect("service DB"))
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(64)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").expect("secure DB"))
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'integration-boundary-paranoid')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1)
        .await
        .unwrap();
    Setup {
        admin,
        service,
        router: api::router(secure),
        workspace,
        writer,
    }
}

fn req(
    method: &str,
    path: String,
    token: &str,
    body: Option<Value>,
    extra_headers: &[(&str, &str)],
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    builder
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

async fn create_situation(f: &Setup, situation: Uuid) {
    let path = format!("/v1/workspaces/{}/situations/{situation}", f.workspace);
    let (status, _) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            &f.writer,
            Some(json!({"focus":"Investigate what the available observations support."})),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn repeated_operational_delivery_is_suppressed_without_new_evidence() {
    let f = setup().await;
    let deliveries = std::env::var("NICCOLO_PARANOID_DUPLICATE_DELIVERIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(128)
        .clamp(2, 10_000);
    let operational_id = "queue:event:resolution-credit:42";
    let raw = b"From: A

same transport event";

    let first = ingest_source_with_operational_id(
        &f.service,
        f.workspace,
        Uuid::new_v4(),
        SourceFormat::Eml,
        "fixture://retry-storm",
        raw,
        Some(operational_id),
    )
    .await
    .unwrap();
    assert!(!first.operational_duplicate);

    for _ in 1..deliveries {
        let duplicate = ingest_source_with_operational_id(
            &f.service,
            f.workspace,
            Uuid::new_v4(),
            SourceFormat::Eml,
            "fixture://retry-storm",
            raw,
            Some(operational_id),
        )
        .await
        .unwrap();
        assert!(duplicate.operational_duplicate);
        assert_eq!(duplicate.document_id, first.document_id);
        assert_eq!(duplicate.fragment_ids, first.fragment_ids);
        assert_eq!(duplicate.graph_revision, first.graph_revision);
    }

    let documents: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_documents WHERE workspace_id=$1")
            .bind(f.workspace)
            .fetch_one(&f.admin)
            .await
            .unwrap();
    let evidence: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_fragments WHERE workspace_id=$1")
            .bind(f.workspace)
            .fetch_one(&f.admin)
            .await
            .unwrap();
    let delivery_count: i64 = sqlx::query_scalar(
        "SELECT delivery_count FROM niccolo.source_operational_events
          WHERE workspace_id=$1 AND operational_event_id=$2",
    )
    .bind(f.workspace)
    .bind(operational_id)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(documents, 1);
    assert_eq!(evidence, 1);
    assert_eq!(delivery_count as usize, deliveries);
}

#[tokio::test]
async fn identical_text_with_distinct_operational_identity_is_not_collapsed() {
    let f = setup().await;
    let raw = b"From: Independent Reporter

identical words";
    let count = 32usize;
    let mut roots = Vec::new();

    for n in 0..count {
        let result = ingest_source_with_operational_id(
            &f.service,
            f.workspace,
            Uuid::new_v4(),
            SourceFormat::Eml,
            &format!("fixture://distinct/{n}"),
            raw,
            Some(&format!("source-event-{n}")),
        )
        .await
        .unwrap();
        assert!(!result.operational_duplicate);
        roots.push(result.fragment_ids[0]);
    }

    let documents: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_documents WHERE workspace_id=$1")
            .bind(f.workspace)
            .fetch_one(&f.admin)
            .await
            .unwrap();
    assert_eq!(documents as usize, count);

    let graph = load_workspace_graph(&f.service, f.workspace).await.unwrap();
    let assessment = graph.assess(&roots).unwrap();
    assert_eq!(assessment.roots_per_report.len(), count);
    assert!(assessment.shared_source_documents.is_empty());
    assert!(!assessment.independence_established);
}

#[tokio::test]
async fn changed_ceo_purpose_is_history_and_old_snapshot_keeps_old_purpose() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    create_situation(&f, situation).await;

    let p1 = Uuid::new_v4();
    let p1_path = format!(
        "/v1/workspaces/{}/situations/{situation}/purpose/{p1}",
        f.workspace
    );
    let (status, first_purpose) = call(
        f.router.clone(),
        req(
            "POST",
            p1_path,
            &f.writer,
            Some(json!({
                "purpose_text":"Find what the available observations support.",
                "reason":"Initial investigation mandate",
                "downstream_use":"Internal bonus review"
            })),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let a1 = Uuid::new_v4();
    let a1_path = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{a1}",
        f.workspace
    );
    let (status, old_snapshot) = call(
        f.router.clone(),
        req(
            "POST",
            a1_path.clone(),
            &f.writer,
            Some(json!({"goal":"Assess the observations without a predetermined verdict."})),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        old_snapshot["declared_purpose"],
        "Find what the available observations support."
    );
    assert_eq!(
        old_snapshot["purpose_event_no_max"],
        first_purpose["event_no"]
    );

    let p2 = Uuid::new_v4();
    let p2_path = format!(
        "/v1/workspaces/{}/situations/{situation}/purpose/{p2}",
        f.workspace
    );
    let (status, second_purpose) = call(
        f.router.clone(),
        req(
            "POST",
            p2_path,
            &f.writer,
            Some(json!({
                "purpose_text":"Determine whether the current record supports terminating Kessler.",
                "reason":"CEO changed the decision question",
                "downstream_use":"Employment decision"
            })),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, still_old) =
        call(f.router.clone(), req("GET", a1_path, &f.writer, None, &[])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        still_old["declared_purpose"],
        "Find what the available observations support."
    );
    assert_eq!(still_old["purpose_event_no_max"], first_purpose["event_no"]);

    let a2 = Uuid::new_v4();
    let a2_path = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{a2}",
        f.workspace
    );
    let (status, new_snapshot) = call(
        f.router.clone(),
        req(
            "POST",
            a2_path,
            &f.writer,
            Some(
                json!({"goal":"Assess whether termination is supported and what remains unknown."}),
            ),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        new_snapshot["declared_purpose"],
        "Determine whether the current record supports terminating Kessler."
    );
    assert_eq!(
        new_snapshot["purpose_event_no_max"],
        second_purpose["event_no"]
    );

    let history_path = format!(
        "/v1/workspaces/{}/situations/{situation}/purpose",
        f.workspace
    );
    let (status, history) = call(
        f.router.clone(),
        req("GET", history_path, &f.writer, None, &[]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history["events"].as_array().unwrap().len(), 2);
    assert_eq!(history["purpose_changes_rewrite_history"], false);
}

#[tokio::test]
async fn one_root_with_one_hundred_machine_descendants_stays_one_root() {
    let f = setup().await;
    let imported = ingest_source_with_operational_id(
        &f.service,
        f.workspace,
        Uuid::new_v4(),
        SourceFormat::Eml,
        "fixture://root",
        b"From: Human

one unsupported assertion",
        Some("root-event"),
    )
    .await
    .unwrap();
    let root = imported.fragment_ids[0];

    let mut descendants = Vec::new();
    for n in 0..100 {
        let child = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO niccolo.evidence
             (workspace_id,evidence_id,source_locator,content,source_class,admissible)
             VALUES($1,$2,$3,$4,'COPY',true)",
        )
        .bind(f.workspace)
        .bind(child)
        .bind(format!("machine://worker/{n}"))
        .bind(format!("machine descendant {n}"))
        .execute(&f.admin)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO niccolo.evidence_derivation
             (workspace_id,child_id,parent_id,relation)
             VALUES($1,$2,$3,'COPIED_FROM')",
        )
        .bind(f.workspace)
        .bind(child)
        .bind(root)
        .execute(&f.admin)
        .await
        .unwrap();
        descendants.push(child);
    }

    let graph = load_workspace_graph(&f.service, f.workspace).await.unwrap();
    let assessment = graph.assess(&descendants).unwrap();
    assert_eq!(assessment.report_count, 100);
    assert_eq!(assessment.traceable_roots.len(), 1);
    assert!(assessment.traceable_roots.contains(&root));
    assert!(!assessment.independence_established);
}


#[tokio::test]
async fn aggregate_integration_diagnostics_report_transport_patterns_without_truth_claims() {
    let f = setup().await;

    let retry_raw = b"From: Queue\n\nretry storm";
    let retry_document = Uuid::new_v4();
    for attempt in 0..3 {
        let result = ingest_source_with_operational_id(
            &f.service,
            f.workspace,
            if attempt == 0 { retry_document } else { Uuid::new_v4() },
            SourceFormat::Eml,
            "fixture://aggregate/retry",
            retry_raw,
            Some("aggregate-retry-1"),
        )
        .await
        .unwrap();
        assert_eq!(result.delivery_count, attempt + 1);
    }

    let repeated_raw = b"From: Manual\n\nsame manually imported payload";
    for n in 0..2 {
        ingest_source_with_operational_id(
            &f.service,
            f.workspace,
            Uuid::new_v4(),
            SourceFormat::Eml,
            &format!("fixture://aggregate/unidentified/{n}"),
            repeated_raw,
            None,
        )
        .await
        .unwrap();
    }

    let fanout_document = Uuid::new_v4();
    let fanout_raw = b"From: Integrator\n\none document, two operational identities";
    for operational_id in ["fanout-a", "fanout-b"] {
        ingest_source_with_operational_id(
            &f.service,
            f.workspace,
            fanout_document,
            SourceFormat::Eml,
            "fixture://aggregate/fanout",
            fanout_raw,
            Some(operational_id),
        )
        .await
        .unwrap();
    }

    let path = format!("/v1/workspaces/{}/integration-diagnostics", f.workspace);
    let (status, body) = call(
        f.router.clone(),
        req("GET", path, &f.writer, None, &[]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source_document_count"], 4);
    assert_eq!(body["operational_event_count"], 3);
    assert_eq!(body["total_delivery_count"], 5);
    assert_eq!(body["duplicate_delivery_count"], 2);
    assert_eq!(body["duplicate_operational_event_count"], 1);
    assert_eq!(body["max_delivery_count"], 3);
    assert_eq!(body["documents_with_operational_identity"], 2);
    assert_eq!(body["documents_without_operational_identity"], 2);
    assert_eq!(body["repeated_unidentified_payload_group_count"], 1);
    assert_eq!(body["repeated_unidentified_payload_document_count"], 2);
    assert_eq!(body["max_repeated_unidentified_payload_documents"], 2);
    assert_eq!(body["documents_with_multiple_operational_ids"], 1);
    assert_eq!(body["max_operational_ids_per_document"], 2);
    assert_eq!(
        body["warning_codes"],
        json!([
            "DUPLICATE_OPERATIONAL_DELIVERIES",
            "REPEATED_PAYLOAD_WITHOUT_OPERATIONAL_IDENTITY",
            "MULTIPLE_OPERATIONAL_IDS_FOR_DOCUMENT"
        ])
    );
    assert_eq!(
        body["interpretation"],
        "Integration diagnostics describe delivery and identity patterns only; they do not measure evidence quality, credibility, independence, or truth."
    );
}


#[tokio::test]
async fn upstream_processing_declarations_are_attributed_metadata_not_evidence() {
    let f = setup().await;
    let source = ingest_source_with_operational_id(
        &f.service,
        f.workspace,
        Uuid::new_v4(),
        SourceFormat::Eml,
        "fixture://processed/source",
        b"From: Source\n\nretained after upstream filtering",
        Some("processed-source-1"),
    )
    .await
    .unwrap();

    let declaration = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/integration-processing-declarations/{declaration}",
        f.workspace
    );
    let body = json!({
        "processing_kind":"FILTER",
        "scope_locator":"s3://legal-hold/mail/",
        "declaration_text":"Integrator reports filtering to messages dated 2026-01-01 through 2026-06-30.",
        "source_document_id":source.document_id
    });
    let (status, created) = call(
        f.router.clone(),
        req("POST", path.clone(), &f.writer, Some(body.clone()), &[]),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["processing_kind"], "FILTER");
    assert_eq!(created["declaration_is_verified"], false);
    assert_eq!(created["declaration_is_evidence"], false);
    assert_eq!(created["replayed"], false);

    let (status, replayed) = call(
        f.router.clone(),
        req("POST", path.clone(), &f.writer, Some(body), &[]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);

    let (status, _) = call(
        f.router.clone(),
        req(
            "POST",
            path,
            &f.writer,
            Some(json!({
                "processing_kind":"SAMPLING",
                "scope_locator":"s3://legal-hold/mail/",
                "declaration_text":"Conflicting reuse of the same declaration id.",
                "source_document_id":source.document_id
            })),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let second = Uuid::new_v4();
    let second_path = format!(
        "/v1/workspaces/{}/integration-processing-declarations/{second}",
        f.workspace
    );
    let (status, second_created) = call(
        f.router.clone(),
        req(
            "POST",
            second_path,
            &f.writer,
            Some(json!({
                "processing_kind":"SOURCE_SELECTION",
                "scope_locator":"custodian://executive-mailboxes",
                "declaration_text":"Integrator reports selecting only two named custodians.",
                "source_document_id":null
            })),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let list_path = format!(
        "/v1/workspaces/{}/integration-processing-declarations?after=0&limit=1",
        f.workspace
    );
    let (status, first_page) = call(
        f.router.clone(),
        req("GET", list_path, &f.writer, None, &[]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first_page["declarations"].as_array().unwrap().len(), 1);
    assert_eq!(first_page["declarations_are_verified"], false);
    assert_eq!(first_page["declarations_are_evidence"], false);

    let after = first_page["next_after"].as_i64().unwrap();
    let list_path = format!(
        "/v1/workspaces/{}/integration-processing-declarations?after={after}&limit=100",
        f.workspace
    );
    let (status, second_page) = call(
        f.router.clone(),
        req("GET", list_path, &f.writer, None, &[]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second_page["declarations"].as_array().unwrap().len(), 1);
    assert_eq!(second_page["declarations"][0]["declaration_id"], second_created["declaration_id"]);

    let invisible = Uuid::new_v4();
    let invisible_path = format!(
        "/v1/workspaces/{}/integration-processing-declarations/{}",
        f.workspace,
        Uuid::new_v4()
    );
    let (status, _) = call(
        f.router.clone(),
        req(
            "POST",
            invisible_path,
            &f.writer,
            Some(json!({
                "processing_kind":"TRANSFORMATION",
                "scope_locator":"pipeline://ocr",
                "declaration_text":"Integrator reports OCR transformation.",
                "source_document_id":invisible
            })),
            &[],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
