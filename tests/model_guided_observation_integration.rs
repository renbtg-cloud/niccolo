#![cfg(feature = "pg-integration")]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    analysis_structure::AnalysisScope, api, authz, reasoning_packet, reasoning_updates,
    observation_retrieval, observation_query_ledger,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Harness {
    router: Router,
    secure: PgPool,
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
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
        secure,
        workspace,
        situation,
        principal,
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
async fn a_model_intent_retrieves_late_frozen_evidence_into_the_next_packet() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &situation_path, &h.token, Some(json!({"focus":"Investigate competing explanations"}))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);

    let content = (0..300).map(|i| {
        let text = if i == 299 { "MENSAGEMDECISIVATARDIA" } else { "routine communication" };
        format!("01/01/2026, 10:{:02} - Ana: {} ordinal {}\n", i % 60, text, i)
    }).collect::<String>();
    let content: &'static str = Box::leak(content.into_boxed_str());
    let document = ingest_and_attach(&h, "hypothesis-search.txt", content).await;
    let analysis = Uuid::new_v4();
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &analysis_path, &h.token, Some(json!({
            "goal":"Find observations that discriminate live hypotheses",
            "reanalysis_of":null
        }))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);

    let query_id = Uuid::new_v4();
    let update: reasoning_updates::ReasoningUpdate = serde_json::from_value(json!({
        "source_queries":[{
            "query_id":query_id,
            "query":"MENSAGEMDECISIVATARDIA",
            "rationale":"Look for evidence outside the initial context",
            "expected_observation":"A late message changes the plausible interpretation",
            "direction":"DISCRIMINATE",
            "interpretation_id":null
        }]
    })).unwrap();
    let scope = AnalysisScope {workspace:h.workspace,situation:h.situation,analysis};
    let digest = authz::credential_digest(&h.token).unwrap();
    let packet = authz::with_auth_digest(digest, async {
        let before = reasoning_packet::build(&h.secure, scope).await.unwrap();
        assert_eq!(before.frozen_input.source_fragment_count_total, 300);
        assert_eq!(before.frozen_input.source_fragment_count_included, 256);
        assert!(before.observation_queries.is_empty());

        let applied = reasoning_updates::apply(&h.secure,scope,h.principal,&update).await.unwrap();
        assert_eq!(applied.source_query_count,1);
        let matches = observation_retrieval::search(
            &h.secure,scope,&update.source_queries[0].query,5
        ).await.unwrap();
        assert_eq!(matches.results.len(),1);
        assert_eq!(matches.results[0].ordinal,299);
        observation_query_ledger::record_hits(
            &h.secure,scope,query_id,h.principal,&matches.results
        ).await.unwrap();
        // An idempotent replay does not duplicate or alter the trace.
        observation_query_ledger::record_hits(
            &h.secure,scope,query_id,h.principal,&matches.results
        ).await.unwrap();
        reasoning_packet::build(&h.secure,scope).await.unwrap()
    }).await;

    assert_eq!(packet.observation_queries.len(),1);
    let trace = &packet.observation_queries[0];
    assert_eq!(trace.query_id,query_id);
    assert_eq!(trace.direction,"DISCRIMINATE");
    assert_eq!(trace.returned_hit_count,1);
    assert!(!trace.is_evidence_verification);
    assert_eq!(trace.retrieved_observations.len(),1);
    assert_eq!(trace.retrieved_observations[0].ordinal,299);
    assert_eq!(trace.retrieved_observations[0].document_id,document);
    assert_eq!(trace.retrieved_observations[0].speaker_label,"Ana");
    assert!(!packet.observation_query_packet_truncated);

    // The model request can now carry an observation that did not fit in
    // the first 256-fragment packet, without changing the historical input.
    assert!(packet.frozen_input.source_packet_truncated);
    assert_eq!(packet.frozen_input.source_fragment_count_included,256);
}

#[tokio::test]
async fn query_hit_recording_rejects_an_unauthorized_document() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &situation_path, &h.token, Some(json!({"focus":"Freeze first"}))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    let analysis = Uuid::new_v4();
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &analysis_path, &h.token, Some(json!({
            "goal":"Do not import future fragments",
            "reanalysis_of":null
        }))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    ingest_and_attach(&h,"later.txt","01/01/2026, 10:00 - Ana: FUTUREEVIDENCETOKEN\n").await;
    let query_id = Uuid::new_v4();
    let update: reasoning_updates::ReasoningUpdate = serde_json::from_value(json!({
        "source_queries":[{
            "query_id":query_id,
            "query":"FUTUREEVIDENCETOKEN",
            "rationale":"Check exclusion",
            "expected_observation":"Should not find later sources",
            "direction":"COUNTEREVIDENCE",
            "interpretation_id":null
        }]
    })).unwrap();
    let scope = AnalysisScope {workspace:h.workspace,situation:h.situation,analysis};
    let digest = authz::credential_digest(&h.token).unwrap();
    authz::with_auth_digest(digest, async {
        reasoning_updates::apply(&h.secure,scope,h.principal,&update).await.unwrap();
        let found = observation_retrieval::search(&h.secure,scope,"FUTUREEVIDENCETOKEN",5).await.unwrap();
        assert!(found.results.is_empty());
        observation_query_ledger::record_hits(
            &h.secure,scope,query_id,h.principal,&found.results
        ).await.unwrap();
        let packet = reasoning_packet::build(&h.secure,scope).await.unwrap();
        assert_eq!(packet.observation_queries.len(),1);
        assert_eq!(packet.observation_queries[0].returned_hit_count,0);
    }).await;
}
