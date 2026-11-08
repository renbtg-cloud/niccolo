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

struct Harness {
    router: Router,
    workspace: Uuid,
    situation: Uuid,
    token: String,
}

async fn setup() -> Harness {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").expect("admin DB"))
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(12)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").expect("secure DB"))
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'reasoning-packet-sources')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    Harness {
        router: api::router(secure.clone()),
        workspace,
        situation,
        token,
    }
}

fn json_request(method: &str, path: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json");
    let bytes = body
        .map(|value| serde_json::to_vec(&value).unwrap())
        .unwrap_or_default();
    if !bytes.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder.body(Body::from(bytes)).unwrap()
}

fn source_request(path: &str, token: &str, locator: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "text/plain")
        .header("x-source-locator", locator)
        .body(Body::from(body))
        .unwrap()
}

async fn invoke(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}

async fn ingest_and_attach(h: &Harness, locator: &str, text: &'static str) -> Uuid {
    let document = Uuid::new_v4();
    let source_path = format!("/v1/workspaces/{}/sources/{document}", h.workspace);
    let (status, _) = invoke(
        h.router.clone(),
        source_request(&source_path, &h.token, locator, text),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let attach_path = format!(
        "/v1/workspaces/{}/situations/{}/sources/{document}",
        h.workspace, h.situation
    );
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &attach_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    document
}


#[tokio::test]
async fn searches_all_frozen_fragments_not_only_first_256() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &situation_path, &h.token, Some(json!({"focus":"Find late evidence"}))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);

    let content = (0..300)
        .map(|i| {
            let message = if i == 299 { "ULTIMAMENSAGEMUNICA" } else { "ordinary context" };
            format!("01/01/2026, 10:{:02} - Ana: {} ordinal {}\n", i % 60, message, i)
        })
        .collect::<String>();
    let content: &'static str = Box::leak(content.into_boxed_str());
    let before = ingest_and_attach(&h, "long-conversation.txt", content).await;

    let analysis = Uuid::new_v4();
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST", &analysis_path, &h.token,
            Some(json!({"goal":"Find the hidden late message","reanalysis_of":null})),
        ),
    ).await;
    assert_eq!(status, StatusCode::CREATED);

    let search_path = format!("{analysis_path}/observations/search?q=ULTIMAMENSAGEMUNICA&limit=10");
    let (status, page) = invoke(
        h.router.clone(),
        json_request("GET", &search_path, &h.token, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["searched_entire_frozen_revision"], true);
    assert_eq!(page["results_establish_truth"], false);
    assert_eq!(page["lexical_ranking_is_evidentiary_weight"], false);
    assert_eq!(page["results"].as_array().unwrap().len(), 1);
    assert_eq!(page["results"][0]["ordinal"], 299);
    assert_eq!(page["results"][0]["document_id"], before.to_string());
    assert_eq!(page["results"][0]["speaker_label"], "Ana");
    let start = page["results"][0]["byte_start"].as_u64().unwrap() as usize;
    let end = page["results"][0]["byte_end"].as_u64().unwrap() as usize;
    assert_eq!(
        page["results"][0]["content"].as_str().unwrap().as_bytes(),
        &content.as_bytes()[start..end],
    );
    assert_eq!(page["has_more"], false);

    // Same exact frozen snapshot produces a deterministic response.
    let (status, again) = invoke(
        h.router.clone(),
        json_request("GET", &search_path, &h.token, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page, again);

    let after = ingest_and_attach(
        &h, "later.txt", "01/01/2026, 11:01 - Beto: DEPOISCONGELAMENTO\n",
    ).await;
    assert_ne!(before, after);
    let after_path = format!("{analysis_path}/observations/search?q=DEPOISCONGELAMENTO");
    let (status, frozen_result) = invoke(
        h.router.clone(),
        json_request("GET", &after_path, &h.token, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(frozen_result["results"].as_array().unwrap().len(), 0);

    // A new analysis with an updated revision can see the later source.
    let second_analysis = Uuid::new_v4();
    let second_path = format!("{situation_path}/analysis-snapshots/{second_analysis}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST", &second_path, &h.token,
            Some(json!({"goal":"Use the updated scope","reanalysis_of":analysis})),
        ),
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, updated_result) = invoke(
        h.router.clone(),
        json_request(
            "GET", &format!("{second_path}/observations/search?q=DEPOISCONGELAMENTO"), &h.token, None,
        ),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated_result["results"][0]["document_id"], after.to_string());
}

#[tokio::test]
async fn search_rejects_invalid_requests_and_does_not_leak_analyses() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &situation_path, &h.token, Some(json!({"focus":"Access isolation"}))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    let analysis = Uuid::new_v4();
    let path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST", &path, &h.token,
            Some(json!({"goal":"Authorized retrieval","reanalysis_of":null})),
        ),
    ).await;
    assert_eq!(status, StatusCode::CREATED);

    for bad in ["q=---", "q=foo&limit=26", "q=foo&limit=0", "q=foo%0Abar"] {
        let (status, _) = invoke(
            h.router.clone(),
            json_request("GET", &format!("{path}/observations/search?{bad}"), &h.token, None),
        ).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "GET",
            &format!("{situation_path}/analysis-snapshots/{}/observations/search?q=foo", Uuid::new_v4()),
            &h.token, None,
        ),
    ).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let other = setup().await;
    let (status, _) = invoke(
        h.router.clone(),
        json_request("GET", &format!("{path}/observations/search?q=foo"), &other.token, None),
    ).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
