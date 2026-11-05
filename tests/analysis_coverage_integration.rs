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
        .max_connections(24)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'analysis-coverage-test')",
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

fn situation_path(f: &Setup, situation: Uuid) -> String {
    format!("/v1/workspaces/{}/situations/{situation}", f.workspace)
}

fn analysis_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    )
}

fn coverage_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "{}/examined-fragments",
        analysis_path(f, situation, analysis)
    )
}

async fn create_situation(f: &Setup, situation: Uuid) {
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                situation_path(f, situation),
                &f.writer,
                Some(json!({"focus":"Understand the situation"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

async fn seed_document(f: &Setup, fragments: usize) -> (Uuid, Vec<Uuid>) {
    let document = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) \
         VALUES($1,$2,'WHATSAPP_EXPORT',$3,$4,1000)",
    )
    .bind(f.workspace)
    .bind(document)
    .bind(format!("fixture://{document}"))
    .bind("a".repeat(64))
    .execute(&f.admin)
    .await
    .unwrap();
    let mut ids = Vec::with_capacity(fragments);
    for ordinal in 0..fragments {
        let evidence = Uuid::new_v4();
        let start = (ordinal * 20) as i32;
        let end = start + 10;
        sqlx::query(
            "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) \
             VALUES($1,$2,$3,$4,'REPORTED_SPEECH')",
        )
        .bind(f.workspace)
        .bind(evidence)
        .bind(format!("fixture://{document}#{ordinal}"))
        .bind(format!("fragment {ordinal}"))
        .execute(&f.admin)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) \
             VALUES($1,$2,$3,$4,$5,$6,'Alice')",
        )
        .bind(f.workspace)
        .bind(document)
        .bind(ordinal as i32)
        .bind(evidence)
        .bind(start)
        .bind(end)
        .execute(&f.admin)
        .await
        .unwrap();
        ids.push(evidence);
    }
    (document, ids)
}

async fn add_source(f: &Setup, situation: Uuid, document: Uuid) {
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/sources/{document}", situation_path(f, situation)),
                &f.writer,
                None,
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

async fn begin_analysis(f: &Setup, situation: Uuid, analysis: Uuid, parent: Option<Uuid>) {
    let mut body = json!({"goal":"inspect selected fragments"});
    if let Some(parent) = parent {
        body["reanalysis_of"] = json!(parent);
    }
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                analysis_path(f, situation, analysis),
                &f.writer,
                Some(body),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn exact_fragment_can_be_recorded_only_from_frozen_source_set() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (document, ids) = seed_document(&f, 1).await;
    add_source(&f, situation, document).await;
    begin_analysis(&f, situation, analysis, None).await;

    let path = format!("{}/{}", coverage_path(&f, situation, analysis), ids[0]);
    let (status, saved) = call(f.router.clone(), req("POST", path.clone(), &f.writer, None)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(saved["evidence_id"], ids[0].to_string());
    assert_eq!(saved["document_id"], document.to_string());
    assert_eq!(saved["coverage_claim"], "EXAMINED_FRAGMENT");
    assert_eq!(saved["proves_full_source_examined"], false);

    let (status, replay) = call(f.router.clone(), req("POST", path, &f.writer, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let (status, listed) = call(
        f.router.clone(),
        req(
            "GET",
            coverage_path(&f, situation, analysis),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["fragments"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn source_added_later_cannot_be_backdated_into_old_analysis_coverage() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let original = Uuid::new_v4();
    let reanalysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (first_doc, _) = seed_document(&f, 1).await;
    add_source(&f, situation, first_doc).await;
    begin_analysis(&f, situation, original, None).await;

    let (later_doc, later_ids) = seed_document(&f, 1).await;
    add_source(&f, situation, later_doc).await;
    let old_path = format!(
        "{}/{}",
        coverage_path(&f, situation, original),
        later_ids[0]
    );
    assert_eq!(
        call(f.router.clone(), req("POST", old_path, &f.writer, None))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    begin_analysis(&f, situation, reanalysis, Some(original)).await;
    let new_path = format!(
        "{}/{}",
        coverage_path(&f, situation, reanalysis),
        later_ids[0]
    );
    assert_eq!(
        call(f.router.clone(), req("POST", new_path, &f.writer, None))
            .await
            .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn coverage_is_bounded_paginated_and_reader_cannot_write() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (document, ids) = seed_document(&f, 3).await;
    add_source(&f, situation, document).await;
    begin_analysis(&f, situation, analysis, None).await;
    for id in &ids {
        assert_eq!(
            call(
                f.router.clone(),
                req(
                    "POST",
                    format!("{}/{}", coverage_path(&f, situation, analysis), id),
                    &f.writer,
                    None,
                ),
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
                format!("{}/{}", coverage_path(&f, situation, analysis), ids[0]),
                &f.reader,
                None,
            ),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, first) = call(
        f.router.clone(),
        req(
            "GET",
            format!("{}?limit=2", coverage_path(&f, situation, analysis)),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(first["fragments"].as_array().unwrap().len(), 2);
    let cursor = first["next_after"].as_str().unwrap();
    let (_, second) = call(
        f.router.clone(),
        req(
            "GET",
            format!(
                "{}?limit=2&after={cursor}",
                coverage_path(&f, situation, analysis)
            ),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(second["fragments"].as_array().unwrap().len(), 1);
    assert!(second["next_after"].is_null());
    assert_eq!(
        call(
            f.router.clone(),
            req(
                "GET",
                format!("{}?limit=0", coverage_path(&f, situation, analysis)),
                &f.reader,
                None,
            ),
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_identical_coverage_records_one_row() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (document, ids) = seed_document(&f, 1).await;
    add_source(&f, situation, document).await;
    begin_analysis(&f, situation, analysis, None).await;
    let path = format!("{}/{}", coverage_path(&f, situation, analysis), ids[0]);

    let mut tasks = Vec::new();
    for _ in 0..24 {
        let router = f.router.clone();
        let token = f.writer.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            call(router, req("POST", path, &token, None)).await.0
        }));
    }
    let mut created = 0;
    for task in tasks {
        match task.await.unwrap() {
            StatusCode::CREATED => created += 1,
            StatusCode::OK => {}
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(created, 1);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_examined_fragments \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(f.workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

fn coverage_summary_path(f: &Setup, situation: Uuid, analysis: Uuid) -> String {
    format!("{}/coverage", analysis_path(f, situation, analysis))
}

#[tokio::test]
async fn coverage_summary_distinguishes_present_partial_and_all_catalogued_text() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (document, ids) = seed_document(&f, 2).await;
    add_source(&f, situation, document).await;
    begin_analysis(&f, situation, analysis, None).await;

    let summary_path = coverage_summary_path(&f, situation, analysis);
    let (_, none) = call(
        f.router.clone(),
        req("GET", summary_path.clone(), &f.reader, None),
    )
    .await;
    let doc = &none["documents"][0];
    assert_eq!(doc["catalogued_fragment_count"], 2);
    assert_eq!(doc["examined_fragment_count"], 0);
    assert_eq!(doc["all_catalogued_fragments_examined"], false);
    assert_eq!(doc["proves_full_source_examined"], false);
    assert_eq!(none["source_present_does_not_mean_examined"], true);

    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/{}", coverage_path(&f, situation, analysis), ids[0]),
                &f.writer,
                None,
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (_, partial) = call(
        f.router.clone(),
        req("GET", summary_path.clone(), &f.reader, None),
    )
    .await;
    assert_eq!(partial["documents"][0]["examined_fragment_count"], 1);
    assert_eq!(
        partial["documents"][0]["all_catalogued_fragments_examined"],
        false
    );

    assert_eq!(
        call(
            f.router.clone(),
            req(
                "POST",
                format!("{}/{}", coverage_path(&f, situation, analysis), ids[1]),
                &f.writer,
                None,
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (_, complete_catalogue) =
        call(f.router.clone(), req("GET", summary_path, &f.reader, None)).await;
    assert_eq!(
        complete_catalogue["documents"][0]["examined_fragment_count"],
        2
    );
    assert_eq!(
        complete_catalogue["documents"][0]["all_catalogued_fragments_examined"],
        true
    );
    assert_eq!(
        complete_catalogue["documents"][0]["proves_full_source_examined"],
        false
    );
    assert_eq!(
        complete_catalogue["all_catalogued_fragments_examined_does_not_prove_unsupported_media_examined"],
        true
    );
}

#[tokio::test]
async fn coverage_summary_uses_frozen_source_set_not_current_situation_sources() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let original = Uuid::new_v4();
    let reanalysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (first_doc, _) = seed_document(&f, 1).await;
    add_source(&f, situation, first_doc).await;
    begin_analysis(&f, situation, original, None).await;

    let (later_doc, _) = seed_document(&f, 1).await;
    add_source(&f, situation, later_doc).await;

    let (_, old) = call(
        f.router.clone(),
        req(
            "GET",
            coverage_summary_path(&f, situation, original),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(old["documents"].as_array().unwrap().len(), 1);
    assert_eq!(old["documents"][0]["document_id"], first_doc.to_string());

    begin_analysis(&f, situation, reanalysis, Some(original)).await;
    let (_, newer) = call(
        f.router.clone(),
        req(
            "GET",
            coverage_summary_path(&f, situation, reanalysis),
            &f.reader,
            None,
        ),
    )
    .await;
    let ids = newer["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["document_id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(ids.contains(first_doc.to_string().as_str()));
    assert!(ids.contains(later_doc.to_string().as_str()));
}

#[tokio::test]
async fn coverage_summary_is_bounded_paginated_and_rejects_invalid_limit() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    for _ in 0..3 {
        let (document, _) = seed_document(&f, 1).await;
        add_source(&f, situation, document).await;
    }
    begin_analysis(&f, situation, analysis, None).await;
    let base = coverage_summary_path(&f, situation, analysis);
    let (_, first) = call(
        f.router.clone(),
        req("GET", format!("{base}?limit=2"), &f.reader, None),
    )
    .await;
    assert_eq!(first["documents"].as_array().unwrap().len(), 2);
    let cursor = first["next_after"].as_str().unwrap();
    let (_, second) = call(
        f.router.clone(),
        req(
            "GET",
            format!("{base}?limit=2&after={cursor}"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(second["documents"].as_array().unwrap().len(), 1);
    assert!(second["next_after"].is_null());
    assert_eq!(
        call(
            f.router.clone(),
            req("GET", format!("{base}?limit=0"), &f.reader, None),
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn zero_catalogued_fragments_never_claims_complete_source_examination() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    create_situation(&f, situation).await;
    let (document, ids) = seed_document(&f, 0).await;
    assert!(ids.is_empty());
    add_source(&f, situation, document).await;
    begin_analysis(&f, situation, analysis, None).await;

    let (status, summary) = call(
        f.router.clone(),
        req(
            "GET",
            coverage_summary_path(&f, situation, analysis),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary["documents"][0]["catalogued_fragment_count"], 0);
    assert_eq!(summary["documents"][0]["examined_fragment_count"], 0);
    assert_eq!(
        summary["documents"][0]["all_catalogued_fragments_examined"],
        false
    );
    assert_eq!(
        summary["documents"][0]["proves_full_source_examined"],
        false
    );
}
