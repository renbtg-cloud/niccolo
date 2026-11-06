use niccolo_evidence::model_adapter::{ModelRequest, OpenAiCompatibleModel, ReasoningModel};
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn read_request(mut stream: &TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&bytes);
        if let Some(header_end) = text.find("\r\n\r\n") {
            let headers = &text[..header_end];
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn one_shot_server(status: &str, body: &str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = body.to_owned();
    let status = status.to_owned();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&stream);
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        )
        .unwrap();
        request
    });
    (format!("http://{address}/v1/chat/completions"), handle)
}

fn request() -> ModelRequest {
    ModelRequest {
        purpose: "SYNTHESIZE".into(),
        input: json!({"situation":"ambiguous message","evidence":["hello"]}),
        output_contract: json!({"type":"object","required":["niccolo_artifact"]}),
    }
}

#[tokio::test]
async fn openai_compatible_provider_posts_structured_request_and_parses_json_content() {
    let provider_body = json!({
        "model":"mock-model-reported",
        "choices":[{"message":{"role":"assistant","content":"{\"niccolo_artifact\":{\"answer_text\":\"provisional\",\"coverage_note\":\"one fragment\"}}"}}],
        "usage":{"prompt_tokens":17,"completion_tokens":9}
    })
    .to_string();
    let (endpoint, server) = one_shot_server("200 OK", &provider_body);
    let model = OpenAiCompatibleModel::new(
        "mock-provider",
        "requested-model",
        &endpoint,
        "secret-token",
    )
    .unwrap();

    let response = model.invoke(&request()).await.unwrap();
    assert_eq!(response.value["niccolo_artifact"]["answer_text"], "provisional");
    assert_eq!(response.prompt_tokens, Some(17));
    assert_eq!(response.completion_tokens, Some(9));
    assert_eq!(response.effective_provider.as_deref(), Some("mock-provider"));
    assert_eq!(
        response.effective_model.as_deref(),
        Some("mock-model-reported")
    );

    let raw = server.join().unwrap();
    assert!(raw.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
    assert!(raw.contains("Authorization: Bearer secret-token\r\n"));
    assert!(raw.contains("Content-Type: application/json\r\n"));
    let body = raw.split("\r\n\r\n").nth(1).unwrap();
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(sent["model"], "requested-model");
    assert_eq!(sent["stream"], false);
    assert_eq!(sent["response_format"]["type"], "json_object");
    let user = sent["messages"][1]["content"].as_str().unwrap();
    let envelope: serde_json::Value = serde_json::from_str(user).unwrap();
    assert_eq!(envelope["purpose"], "SYNTHESIZE");
    assert_eq!(envelope["input"]["situation"], "ambiguous message");
    assert_eq!(
        envelope["output_contract"]["required"][0],
        "niccolo_artifact"
    );
}

#[tokio::test]
async fn openai_compatible_provider_surfaces_non_success_without_accepting_body_as_reasoning() {
    let (endpoint, server) = one_shot_server("429 Too Many Requests", "{\"error\":\"rate limited\"}");
    let model =
        OpenAiCompatibleModel::new("mock-provider", "model", &endpoint, "token").unwrap();
    let error = model.invoke(&request()).await.unwrap_err();
    assert!(error.to_string().contains("HTTP 429"));
    let _ = server.join().unwrap();
}

#[test]
fn openai_compatible_provider_allows_plain_http_only_on_loopback() {
    assert!(
        OpenAiCompatibleModel::new(
            "provider",
            "model",
            "http://example.com/v1/chat/completions",
            "token"
        )
        .is_err()
    );
    assert!(
        OpenAiCompatibleModel::new(
            "provider",
            "model",
            "http://127.0.0.1:9999/v1/chat/completions",
            "token"
        )
        .is_ok()
    );
    assert!(
        OpenAiCompatibleModel::new(
            "provider",
            "model",
            "https://example.com/v1/chat/completions",
            "token"
        )
        .is_ok()
    );
}
