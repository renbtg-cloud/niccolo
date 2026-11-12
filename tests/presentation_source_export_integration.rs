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

struct Fixture {
    router: Router,
    workspace: Uuid,
    writer: String,
    reader: String,
}

async fn setup() -> Fixture {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'presentation-source-export')")
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
    Fixture {
        router: api::router(secure),
        workspace,
        writer,
        reader,
    }
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

fn json_request(method: &str, uri: String, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap()
}

fn analysis_path(f: &Fixture, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    )
}

#[tokio::test]
async fn examined_source_text_requires_explicit_write_authorized_export() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let document = Uuid::new_v4();
    let analysis = Uuid::new_v4();

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!("/v1/workspaces/{}/situations/{situation}", f.workspace),
            &f.writer,
            Some(json!({"focus":"Which message was actually examined?"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let source = b"[02/11/2026, 12:00] Ana: examined private text\n[02/11/2026, 12:01] Bob: unexamined private text\n";
    let ingest = Request::builder()
        .method("POST")
        .uri(format!("/v1/workspaces/{}/sources/{document}", f.workspace))
        .header(header::AUTHORIZATION, format!("Bearer {}", f.writer))
        .header(header::CONTENT_TYPE, "text/plain")
        .header("x-source-locator", "fixture://presentation-source-export")
        .body(Body::from(source.as_slice()))
        .unwrap();
    let (status, imported) = call(f.router.clone(), ingest).await;
    assert_eq!(status, StatusCode::CREATED);
    let fragments = imported["fragment_ids"].as_array().unwrap();
    assert_eq!(fragments.len(), 2);
    let examined = fragments[0].as_str().unwrap().parse::<Uuid>().unwrap();
    let unexamined = fragments[1].as_str().unwrap().parse::<Uuid>().unwrap();

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!(
                "/v1/workspaces/{}/situations/{situation}/sources/{document}",
                f.workspace
            ),
            &f.writer,
            None,
        ),
    )
    .await;
    assert!(matches!(status, StatusCode::CREATED | StatusCode::OK));

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            analysis_path(&f, situation, analysis),
            &f.writer,
            Some(json!({"goal":"Explain only what the examined source supports"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!(
                "{}/examined-fragments/{examined}",
                analysis_path(&f, situation, analysis)
            ),
            &f.writer,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!("{}/output", analysis_path(&f, situation, analysis)),
            &f.writer,
            Some(json!({
                "answer_text":"Only Ana's recorded fragment was examined.",
                "coverage_note":"One of two source fragments examined."
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let package_path = format!(
        "{}/presentation-package",
        analysis_path(&f, situation, analysis)
    );
    let (status, standard) = call(
        f.router.clone(),
        json_request("GET", package_path.clone(), &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(standard["includes_examined_source_fragment_contents"], false);
    assert_eq!(standard["includes_original_source_fragment_contents"], false);
    assert!(standard["files"]["data/examined_source_fragments.json"].is_null());

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path.clone(),
            &f.reader,
            Some(json!({"include_examined_source_fragments":true})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path.clone(),
            &f.writer,
            Some(json!({"include_examined_source_fragments":false})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, sensitive) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path,
            &f.writer,
            Some(json!({"include_examined_source_fragments":true})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sensitive["includes_examined_source_fragment_contents"], true);
    assert_eq!(sensitive["includes_original_source_fragment_contents"], false);
    assert_eq!(sensitive["examined_source_extracts_are_original_uploads"], false);
    assert_eq!(sensitive["examined_source_extracts_establish_truth"], false);
    assert_eq!(sensitive["published_to_git"], false);

    let extracts: Value = serde_json::from_str(
        sensitive["files"]["data/examined_source_fragments.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let rows = extracts.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["evidence_id"], examined.to_string());
    assert_ne!(rows[0]["evidence_id"], unexamined.to_string());
    assert!(rows[0]["content"].as_str().unwrap().contains("examined private text"));
    assert!(!rows[0]["content"].as_str().unwrap().contains("unexamined private text"));
    assert!(
        sensitive["files"]["README.md"]
            .as_str()
            .unwrap()
            .contains("do not establish truth")
    );
}
