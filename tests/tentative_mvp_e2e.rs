#![cfg(feature = "pg-integration")]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz, model_adapter::OpenAiCompatibleModel};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tower::ServiceExt;
use uuid::Uuid;

struct ProviderFixture {
    model: OpenAiCompatibleModel,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
    purposes: Arc<Mutex<Vec<String>>>,
    wires: Arc<Mutex<Vec<String>>>,
}

struct Harness {
    _admin: PgPool,
    router: Router,
    workspace: Uuid,
    situation: Uuid,
    token: String,
    provider_stop: Arc<AtomicBool>,
    provider_thread: Option<thread::JoinHandle<()>>,
    purposes: Arc<Mutex<Vec<String>>>,
    wires: Arc<Mutex<Vec<String>>>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.provider_stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.provider_thread.take() {
            let _ = handle.join();
        }
    }
}

fn model_request_from_wire(wire: &str) -> Value {
    let body = wire
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .expect("provider request has an HTTP body");
    let envelope: Value = serde_json::from_str(body).expect("provider request body is JSON");
    let user = envelope["messages"][1]["content"]
        .as_str()
        .expect("provider request has a user message");
    serde_json::from_str(user).expect("provider user message is a structured Niccolò request")
}

fn response_content(wire: &str) -> (String, String) {
    let purpose = if wire.contains("CHALLENGE_SYNTHESIS") {
        "CHALLENGE_SYNTHESIS"
    } else if wire.contains("SYNTHESIZE") {
        "SYNTHESIZE"
    } else if wire.contains("CHECK_ASSUMPTIONS") {
        "CHECK_ASSUMPTIONS"
    } else if wire.contains("ASSESS_CONTEXT") {
        "ASSESS_CONTEXT"
    } else if wire.contains("INSPECT_SOURCES") {
        "INSPECT_SOURCES"
    } else if wire.contains("COMPARE_ALTERNATIVES") {
        "COMPARE_ALTERNATIVES"
    } else if wire.contains("CHECK_STOP_CONDITIONS") {
        "CHECK_STOP_CONDITIONS"
    } else if wire.contains("PREPARE_HANDOFF") {
        "PREPARE_HANDOFF"
    } else {
        "OTHER"
    };
    let value = match purpose {
        "SYNTHESIZE" => {
            let request = model_request_from_wire(wire);
            let evidence_id = request
                .pointer("/input/reasoning_packet/frozen_input/source_fragments/0/evidence_id")
                .and_then(Value::as_str)
                .expect("synthesis receives frozen source evidence");
            let claim_id = Uuid::new_v4();
            json!({
                "purpose": purpose,
                "status": "ok",
                "niccolo_artifact": {
                    "answer_text": "Tentative MVP answer from the OpenAI-compatible transport.",
                    "coverage_note": "E2E fixture: one WhatsApp export plus attributed human context.",
                    "claims": [{
                        "claim_id": claim_id,
                        "ordinal": 1,
                        "kind": "INFERENCE",
                        "text": "Ana stated that she would arrive at noon.",
                        "qualification": "Attributed statement; not independent proof of arrival."
                    }],
                    "source_links": [{
                        "claim_id": claim_id,
                        "evidence_id": evidence_id,
                        "relation": "SUPPORTS_AS_USED"
                    }]
                }
            })
        }
        "ASSESS_CONTEXT" | "CHECK_ASSUMPTIONS" => json!({
            "purpose": purpose,
            "status": "ok",
            "niccolo_update": {
                "assumptions": [],
                "evidence_gaps": [],
                "clarifications": []
            }
        }),
        "CHALLENGE_SYNTHESIS" => json!({
            "purpose": purpose,
            "status": "ok",
            "niccolo_challenge": {"challenges": []}
        }),
        _ => json!({"purpose": purpose, "status": "ok"}),
    };
    (purpose.to_owned(), value.to_string())
}

fn read_request(socket: &mut std::net::TcpStream) -> String {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let read = socket.read(&mut buf).unwrap_or(0);
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..read]);
        let header_end = bytes.windows(4).position(|w| w == b"\r\n\r\n");
        if let Some(header_end) = header_end {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn provider() -> ProviderFixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let purposes = Arc::new(Mutex::new(Vec::new()));
    let purposes_thread = purposes.clone();
    let wires = Arc::new(Mutex::new(Vec::new()));
    let wires_thread = wires.clone();
    let handle = thread::spawn(move || {
        while !stop_thread.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    let wire = read_request(&mut socket);
                    wires_thread.lock().unwrap().push(wire.clone());
                    let (purpose, content) = response_content(&wire);
                    purposes_thread.lock().unwrap().push(purpose);
                    let envelope = json!({
                        "model": "mock-e2e-model-v1",
                        "choices": [{"message": {"content": content}}],
                        "usage": {"prompt_tokens": 23, "completion_tokens": 11}
                    })
                    .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        envelope.len(),
                        envelope
                    );
                    socket.write_all(response.as_bytes()).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("provider listener failed: {error}"),
            }
        }
    });
    let model = OpenAiCompatibleModel::new(
        "mock-compatible",
        "mock-e2e-model",
        &format!("http://{address}/v1/chat/completions"),
        "e2e-secret",
    )
    .unwrap();
    ProviderFixture {
        model,
        stop,
        thread: handle,
        purposes,
        wires,
    }
}

async fn setup() -> Harness {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'tentative-mvp-e2e')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    let provider = provider();
    Harness {
        _admin: admin,
        router: api::router_with_model(secure, Arc::new(provider.model)),
        workspace,
        situation,
        token,
        provider_stop: provider.stop,
        provider_thread: Some(provider.thread),
        purposes: provider.purposes,
        wires: provider.wires,
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

fn raw_request(
    method: &str,
    path: &str,
    token: &str,
    content_type: &str,
    locator: &str,
    body: &'static str,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, content_type)
        .header("x-source-locator", locator)
        .body(Body::from(body))
        .unwrap()
}

async fn invoke(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, value)
}

#[tokio::test]
async fn v841_v880_tentative_mvp_path_reaches_real_provider_and_publishes_answer() {
    let h = setup().await;

    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, situation) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &situation_path,
            &h.token,
            Some(json!({"focus":"Understand whether Ana's message changes what matters next"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(situation["revision"], 1);

    let document = Uuid::new_v4();
    let source_path = format!("/v1/workspaces/{}/sources/{document}", h.workspace);
    let source = "01/01/2026, 10:00 - Ana: I will arrive at noon\n01/01/2026, 10:01 - Beto: Fine\n";
    let (status, imported) = invoke(
        h.router.clone(),
        raw_request(
            "POST",
            &source_path,
            &h.token,
            "text/plain",
            "chat.txt",
            source,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(imported["fragment_ids"].as_array().unwrap().len(), 2);

    let attach_path = format!("{situation_path}/sources/{document}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request("POST", &attach_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let contribution = Uuid::new_v4();
    let contribution_path = format!("{situation_path}/contributions/{contribution}");
    let (status, context) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &contribution_path,
            &h.token,
            Some(json!({
                "kind":"user_reported",
                "text":"I remember Ana sounding unusually certain.",
                "related_evidence_id":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(context["contributions"][0]["is_verified_evidence"], false);

    let speaker_path = format!("/v1/workspaces/{}/sources/{document}/speakers", h.workspace);
    let (status, speakers) = invoke(
        h.router.clone(),
        json_request("GET", &speaker_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(speakers["speaker_labels_are_verified_identities"], false);
    assert!(
        speakers["speakers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|speaker| speaker["speaker_label"] == "Ana")
    );

    let person = Uuid::new_v4();
    let identity_path = format!("/v1/workspaces/{}/identities/{person}", h.workspace);
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &identity_path,
            &h.token,
            Some(json!({"display_label":"Ana (human confirmed)"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let event = Uuid::new_v4();
    let event_path = format!("/v1/workspaces/{}/identity-events/{event}", h.workspace);
    let (status, assignment) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &event_path,
            &h.token,
            Some(json!({
                "document_id":document,
                "speaker_label":"Ana",
                "operation":"assign",
                "person_id":person,
                "human_reason":"I know who Ana is in this export"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(assignment["person_id"], person.to_string());

    let analysis = Uuid::new_v4();
    let operation = Uuid::new_v4();
    let intent_path = format!("{situation_path}/bff/intents/{operation}/sync");
    let (status, launched) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &intent_path,
            &h.token,
            Some(json!({
                "analysis_id":analysis,
                "intent":"UNDERSTAND",
                "goal":"What can I reasonably conclude and what remains uncertain?",
                "reanalysis_of":null,
                "idempotency_key":format!("mvp-e2e-{operation}"),
                "step_budget":32,
                "context":{"surface":"tentative-mvp-e2e","human_owns_project":true}
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(launched["operation"]["state"], "SUCCEEDED");
    assert_eq!(launched["operation"]["terminal"], true);

    let output_path = format!("{situation_path}/analysis-snapshots/{analysis}/output");
    let (status, output) = invoke(
        h.router.clone(),
        json_request("GET", &output_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        output["answer_text"],
        "Tentative MVP answer from the OpenAI-compatible transport."
    );
    assert_eq!(output["historical_output_preserved"], true);

    let epistemic_path = format!("{situation_path}/analysis-snapshots/{analysis}/bff/epistemic");
    let (status, epistemic) = invoke(
        h.router.clone(),
        json_request("GET", &epistemic_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(epistemic["claims"].as_array().unwrap().len(), 1);
    assert_eq!(
        epistemic["claims"][0]["source_provenance"][0]["relation"],
        "SUPPORTS_AS_USED"
    );
    assert!(
        epistemic["claims"][0]["source_provenance"][0]["content"]
            .as_str()
            .unwrap()
            .contains("I will arrive at noon")
    );
    assert_eq!(
        epistemic["claims"][0]["source_provenance"][0]["speaker_label"],
        "Ana"
    );
    assert_eq!(epistemic["claims_are_verified_facts"], false);
    assert_eq!(epistemic["claim_fragment_relations_establish_truth"], false);

    let examined: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_examined_fragments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(h.workspace)
    .bind(h.situation)
    .bind(analysis)
    .fetch_one(&h._admin)
    .await
    .unwrap();
    assert_eq!(examined, 2);

    let invocations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND status='SUCCEEDED' AND effective_provider='mock-compatible'",
    )
    .bind(h.workspace)
    .bind(h.situation)
    .bind(operation)
    .fetch_one(&h._admin)
    .await
    .unwrap();
    assert!(invocations >= 4);

    let purposes = h.purposes.lock().unwrap().clone();
    assert!(purposes.iter().any(|purpose| purpose == "SYNTHESIZE"));
    assert!(
        purposes
            .iter()
            .any(|purpose| purpose == "CHALLENGE_SYNTHESIS")
    );
    assert!(purposes.iter().any(|purpose| purpose == "ASSESS_CONTEXT"));

    let wires = h.wires.lock().unwrap().clone();
    assert!(
        wires.iter().any(|wire| wire.contains("I will arrive at noon")),
        "provider requests must include source fragment text from the frozen Situation input"
    );
    assert!(
        wires.iter().any(|wire| wire.contains("I remember Ana sounding unusually certain.")),
        "human context must remain available to the provider separately from source evidence"
    );
}
