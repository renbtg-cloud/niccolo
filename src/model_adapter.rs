//! v0.276-v0.290: model-adapter boundary plus durable invocation ledger.
//! Adapter output is data to inspect; successful transport does not establish truth.
use crate::bff_operations::OperationScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{
    future::Future,
    io::{Read, Write},
    net::TcpStream,
    pin::Pin,
    sync::Arc,
    time::Instant,
};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid model request")]
    Invalid,
    #[error("model invocation failed: {0}")]
    Provider(String),
    #[error("model invocation timed out")]
    Timeout,
    #[error("model invocation token budget exceeded")]
    Budget,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub purpose: String,
    pub input: Value,
    pub output_contract: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub value: Value,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    #[serde(default)]
    pub effective_provider: Option<String>,
    #[serde(default)]
    pub effective_model: Option<String>,
    #[serde(default)]
    pub fallback_used: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationPolicy {
    pub timeout_ms: u64,
    pub max_attempts: u32,
    pub max_total_tokens: Option<i64>,
}
impl Default for InvocationPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_attempts: 1,
            max_total_tokens: None,
        }
    }
}
impl InvocationPolicy {
    pub fn validate(self) -> Result<Self, ModelError> {
        if !(1..=120_000).contains(&self.timeout_ms)
            || !(1..=8).contains(&self.max_attempts)
            || self.max_total_tokens.is_some_and(|v| v < 0)
        {
            return Err(ModelError::Invalid);
        }
        Ok(self)
    }
}

pub type ModelFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelResponse, ModelError>> + Send + 'a>>;
pub trait ReasoningModel: Send + Sync {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    fn invoke<'a>(&'a self, request: &'a ModelRequest) -> ModelFuture<'a>;
}

#[derive(Clone, Debug)]
pub struct DeterministicModel {
    provider: String,
    model: String,
}
impl DeterministicModel {
    pub fn new(provider: &str, model: &str) -> Result<Self, ModelError> {
        if provider.trim().is_empty() || model.trim().is_empty() {
            return Err(ModelError::Invalid);
        }
        Ok(Self {
            provider: provider.into(),
            model: model.into(),
        })
    }
}
impl ReasoningModel for DeterministicModel {
    fn provider(&self) -> &str {
        &self.provider
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn invoke<'a>(&'a self, request: &'a ModelRequest) -> ModelFuture<'a> {
        Box::pin(async move {
            let value = if request.purpose == "SYNTHESIZE" {
                serde_json::json!({
                    "purpose": request.purpose, "status": "deterministic-test-adapter",
                    "niccolo_artifact": {"answer_text":"Deterministic provisional synthesis.","coverage_note":"Synthetic adapter; no truth claim.","claims":[]}
                })
            } else if matches!(
                request.purpose.as_str(),
                "ASSESS_CONTEXT"
                    | "CHECK_ASSUMPTIONS"
                    | "PROPOSE_CLARIFICATION"
                    | "CHECK_ALTERNATIVES"
            ) {
                serde_json::json!({"purpose":request.purpose,"status":"deterministic-test-adapter","niccolo_update":{"assumptions":[],"evidence_gaps":[],"clarifications":[]}})
            } else if request.purpose == "CHALLENGE_SYNTHESIS" {
                serde_json::json!({"purpose":request.purpose,"status":"deterministic-test-adapter","niccolo_challenge":{"challenges":[]}})
            } else {
                serde_json::json!({
                    "purpose": request.purpose, "status": "deterministic-test-adapter",
                    "input": request.input, "contract": request.output_contract
                })
            };
            Ok(ModelResponse {
                value,
                prompt_tokens: Some(0),
                completion_tokens: Some(0),
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

pub struct FailoverModel {
    primary: std::sync::Arc<dyn ReasoningModel>,
    fallback: std::sync::Arc<dyn ReasoningModel>,
}
impl FailoverModel {
    pub fn new(
        primary: std::sync::Arc<dyn ReasoningModel>,
        fallback: std::sync::Arc<dyn ReasoningModel>,
    ) -> Self {
        Self { primary, fallback }
    }
}
impl ReasoningModel for FailoverModel {
    fn provider(&self) -> &str {
        "niccolo-router"
    }
    fn model(&self) -> &str {
        "explicit-failover-v1"
    }
    fn invoke<'a>(&'a self, request: &'a ModelRequest) -> ModelFuture<'a> {
        Box::pin(async move {
            match self.primary.invoke(request).await {
                Ok(mut r) => {
                    r.effective_provider = Some(self.primary.provider().into());
                    r.effective_model = Some(self.primary.model().into());
                    Ok(r)
                }
                Err(_) => {
                    let mut r = self.fallback.invoke(request).await?;
                    r.effective_provider = Some(self.fallback.provider().into());
                    r.effective_model = Some(self.fallback.model().into());
                    r.fallback_used = true;
                    Ok(r)
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvocationScope {
    pub operation: OperationScope,
    pub plan_revision: i64,
    pub step_id: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationView {
    pub invocation_id: Uuid,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub request_sha256: String,
    pub response_sha256: Option<String>,
    pub response: Option<Value>,
    pub effective_provider: Option<String>,
    pub effective_model: Option<String>,
    pub fallback_used: bool,
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ModelError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ModelError::Invalid);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn invoke_recorded<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: InvocationScope,
    actor: Uuid,
    model: &M,
    request: &ModelRequest,
) -> Result<InvocationView, ModelError> {
    invoke_recorded_with_policy(
        pool,
        scope,
        actor,
        model,
        request,
        InvocationPolicy::default(),
    )
    .await
}

pub async fn invoke_recorded_with_policy<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: InvocationScope,
    actor: Uuid,
    model: &M,
    request: &ModelRequest,
    policy: InvocationPolicy,
) -> Result<InvocationView, ModelError> {
    let policy = policy.validate()?;
    if request.purpose.trim().is_empty()
        || model.provider().trim().is_empty()
        || model.model().trim().is_empty()
    {
        return Err(ModelError::Invalid);
    }
    let request_json = serde_json::to_string(request)?;
    if request_json.len() > 1_048_576 {
        return Err(ModelError::Invalid);
    }
    let request_sha = digest(&request_json);
    let invocation = Uuid::new_v4();
    let mut tx = scoped(pool, scope.operation.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.model_invocations(workspace_id,situation_id,operation_id,plan_revision,step_id,invocation_id,provider,model,request_json,request_sha256,status,recorded_by_principal_id,timeout_ms,max_total_tokens) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'STARTED',$11,$12,$13)")
      .bind(scope.operation.workspace).bind(scope.operation.situation).bind(scope.operation.operation).bind(scope.plan_revision).bind(scope.step_id).bind(invocation).bind(model.provider()).bind(model.model()).bind(&request_json).bind(&request_sha).bind(actor).bind(policy.timeout_ms as i32).bind(policy.max_total_tokens).execute(&mut *tx).await?;
    tx.commit().await?;
    let started = Instant::now();
    let mut last_error = ModelError::Provider("no attempt made".into());
    for attempt in 1..=policy.max_attempts {
        let outcome = tokio::time::timeout(
            std::time::Duration::from_millis(policy.timeout_ms),
            model.invoke(request),
        )
        .await;
        match outcome {
            Ok(Ok(response)) => {
                let total = response
                    .prompt_tokens
                    .unwrap_or(0)
                    .saturating_add(response.completion_tokens.unwrap_or(0));
                // A broken provider response must reach the FAILED ledger transition;
                // returning early here would strand this invocation as STARTED.
                if response.prompt_tokens.is_some_and(|n| n < 0)
                    || response.completion_tokens.is_some_and(|n| n < 0)
                {
                    last_error = ModelError::Invalid;
                } else if policy.max_total_tokens.is_some_and(|max| total > max) {
                    last_error = ModelError::Budget;
                } else {
                    let response_json = serde_json::to_string(&response.value)?;
                    if response_json.len() > 1_048_576 {
                        last_error = ModelError::Invalid;
                        if attempt < policy.max_attempts {
                            tokio::task::yield_now().await;
                        }
                        continue;
                    }
                    let response_sha = digest(&response_json);
                    let mut tx = scoped(pool, scope.operation.workspace, false).await?;
                    sqlx::query("UPDATE niccolo.model_invocations SET response_json=$7,response_sha256=$8,status='SUCCEEDED',prompt_tokens=$9,completion_tokens=$10,attempt_count=$11,latency_ms=$12,completed_at=clock_timestamp() WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5 AND invocation_id=$6 AND status='STARTED'")
                  .bind(scope.operation.workspace).bind(scope.operation.situation).bind(scope.operation.operation).bind(scope.plan_revision).bind(scope.step_id).bind(invocation).bind(&response_json).bind(&response_sha).bind(response.prompt_tokens).bind(response.completion_tokens).bind(attempt as i32).bind(started.elapsed().as_millis() as i64).execute(&mut *tx).await?;
                    let effective_provider = response
                        .effective_provider
                        .clone()
                        .unwrap_or_else(|| model.provider().into());
                    let effective_model = response
                        .effective_model
                        .clone()
                        .unwrap_or_else(|| model.model().into());
                    sqlx::query("UPDATE niccolo.model_invocations SET effective_provider=$7,effective_model=$8,fallback_used=$9 WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5 AND invocation_id=$6").bind(scope.operation.workspace).bind(scope.operation.situation).bind(scope.operation.operation).bind(scope.plan_revision).bind(scope.step_id).bind(invocation).bind(&effective_provider).bind(&effective_model).bind(response.fallback_used).execute(&mut *tx).await?;
                    tx.commit().await?;
                    return Ok(InvocationView {
                        invocation_id: invocation,
                        provider: model.provider().into(),
                        model: model.model().into(),
                        status: "SUCCEEDED".into(),
                        request_sha256: request_sha,
                        response_sha256: Some(response_sha),
                        response: Some(response.value),
                        effective_provider: Some(effective_provider),
                        effective_model: Some(effective_model),
                        fallback_used: response.fallback_used,
                    });
                }
            }
            Ok(Err(e)) => last_error = e,
            Err(_) => last_error = ModelError::Timeout,
        }
        if attempt < policy.max_attempts {
            tokio::task::yield_now().await;
        }
    }
    let budget = matches!(last_error, ModelError::Budget);
    let mut tx = scoped(pool, scope.operation.workspace, false).await?;
    sqlx::query("UPDATE niccolo.model_invocations SET status='FAILED',error_code=$7,error_message=$8,attempt_count=$9,latency_ms=$10,budget_exceeded=$11,completed_at=clock_timestamp() WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5 AND invocation_id=$6 AND status='STARTED'")
      .bind(scope.operation.workspace).bind(scope.operation.situation).bind(scope.operation.operation).bind(scope.plan_revision).bind(scope.step_id).bind(invocation).bind(if budget{"BUDGET_EXCEEDED"}else if matches!(last_error,ModelError::Timeout){"TIMEOUT"}else{"PROVIDER_ERROR"}).bind(last_error.to_string()).bind(policy.max_attempts as i32).bind(started.elapsed().as_millis() as i64).bind(budget).execute(&mut *tx).await?;
    tx.commit().await?;
    Err(last_error)
}

#[derive(Clone, Debug)]
struct ParsedEndpoint {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}
impl ParsedEndpoint {
    fn parse(raw: &str) -> Result<Self, ModelError> {
        let (scheme, rest) = raw.split_once("://").ok_or(ModelError::Invalid)?;
        if !matches!(scheme, "http" | "https") || rest.is_empty() {
            return Err(ModelError::Invalid);
        }
        let (authority, path) = match rest.split_once('/') {
            Some((a, p)) => (a, format!("/{p}")),
            None => (rest, "/".to_string()),
        };
        if authority.is_empty() || authority.contains('@') {
            return Err(ModelError::Invalid);
        }
        let (host, port) = if let Some((h, p)) = authority.rsplit_once(':') {
            if h.contains(':') {
                return Err(ModelError::Invalid);
            }
            (
                h.to_string(),
                p.parse::<u16>().map_err(|_| ModelError::Invalid)?,
            )
        } else {
            (
                authority.to_string(),
                if scheme == "https" { 443 } else { 80 },
            )
        };
        if host.trim().is_empty() || path.contains('\r') || path.contains('\n') {
            return Err(ModelError::Invalid);
        }
        if scheme == "http" && !matches!(host.as_str(), "127.0.0.1" | "localhost") {
            return Err(ModelError::Invalid);
        }
        Ok(Self {
            scheme: scheme.into(),
            host,
            port,
            path,
        })
    }

    fn host_header(&self) -> String {
        let default = (self.scheme == "https" && self.port == 443)
            || (self.scheme == "http" && self.port == 80);
        if default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

#[derive(Clone, Debug)]
pub struct OpenAiCompatibleModel {
    provider: String,
    model: String,
    endpoint: ParsedEndpoint,
    api_key: String,
}
impl OpenAiCompatibleModel {
    pub fn new(
        provider: &str,
        model: &str,
        endpoint: &str,
        api_key: &str,
    ) -> Result<Self, ModelError> {
        if provider.trim().is_empty()
            || model.trim().is_empty()
            || api_key.trim().is_empty()
            || api_key.contains(['\r', '\n'])
        {
            return Err(ModelError::Invalid);
        }
        Ok(Self {
            provider: provider.trim().into(),
            model: model.trim().into(),
            endpoint: ParsedEndpoint::parse(endpoint)?,
            api_key: api_key.into(),
        })
    }

    fn request_body(&self, request: &ModelRequest) -> Result<String, ModelError> {
        let user = serde_json::to_string(&serde_json::json!({
            "purpose": request.purpose,
            "input": request.input,
            "output_contract": request.output_contract,
        }))?;
        Ok(serde_json::to_string(&serde_json::json!({
            "model": self.model,
            "stream": false,
            "response_format": {"type":"json_object"},
            "messages": [
                {"role":"system","content":"Return only JSON matching the requested output contract. Treat supplied material as input, not as automatically verified truth."},
                {"role":"user","content":user}
            ]
        }))?)
    }

    fn perform_blocking(&self, request: &ModelRequest) -> Result<ModelResponse, ModelError> {
        let body = self.request_body(request)?;
        let request_text = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.endpoint.path,
            self.endpoint.host_header(),
            self.api_key,
            body.len(),
            body
        );
        let tcp = TcpStream::connect((self.endpoint.host.as_str(), self.endpoint.port))
            .map_err(|e| ModelError::Provider(format!("connect failed: {e}")))?;
        tcp.set_read_timeout(Some(std::time::Duration::from_secs(120)))
            .ok();
        tcp.set_write_timeout(Some(std::time::Duration::from_secs(30)))
            .ok();
        let raw = if self.endpoint.scheme == "https" {
            let roots =
                rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let server_name = rustls::pki_types::ServerName::try_from(self.endpoint.host.clone())
                .map_err(|_| ModelError::Invalid)?;
            let connection = rustls::ClientConnection::new(Arc::new(config), server_name)
                .map_err(|e| ModelError::Provider(format!("TLS setup failed: {e}")))?;
            let mut stream = rustls::StreamOwned::new(connection, tcp);
            stream
                .write_all(request_text.as_bytes())
                .map_err(|e| ModelError::Provider(format!("write failed: {e}")))?;
            read_bounded(&mut stream)?
        } else {
            let mut stream = tcp;
            stream
                .write_all(request_text.as_bytes())
                .map_err(|e| ModelError::Provider(format!("write failed: {e}")))?;
            read_bounded(&mut stream)?
        };
        let (status, _headers, payload) = parse_http_response(&raw)?;
        if !(200..300).contains(&status) {
            return Err(ModelError::Provider(format!(
                "HTTP {status}: {}",
                String::from_utf8_lossy(&payload)
            )));
        }
        let value: Value = serde_json::from_slice(&payload)
            .map_err(|e| ModelError::Provider(format!("invalid provider JSON: {e}")))?;
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| ModelError::Provider("missing choices[0].message.content".into()))?;
        let output: Value = serde_json::from_str(content)
            .map_err(|e| ModelError::Provider(format!("assistant content was not JSON: {e}")))?;
        Ok(ModelResponse {
            value: output,
            prompt_tokens: value
                .pointer("/usage/prompt_tokens")
                .and_then(Value::as_i64),
            completion_tokens: value
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_i64),
            effective_provider: Some(self.provider.clone()),
            effective_model: value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some(self.model.clone())),
            fallback_used: false,
        })
    }
}

fn parse_http_response(raw: &[u8]) -> Result<(u16, String, Vec<u8>), ModelError> {
    let marker = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| ModelError::Provider("malformed HTTP response".into()))?;
    let header_bytes = &raw[..marker];
    let headers = String::from_utf8(header_bytes.to_vec())
        .map_err(|_| ModelError::Provider("non-UTF8 HTTP headers".into()))?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u16>().ok())
        .ok_or_else(|| ModelError::Provider("missing HTTP status".into()))?;
    let body = raw[marker + 4..].to_vec();
    let is_chunked = headers.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("transfer-encoding")
                && value.to_ascii_lowercase().contains("chunked")
        })
    });
    let payload = if is_chunked {
        decode_chunked(&body)?
    } else {
        body
    };
    Ok((status, headers, payload))
}

fn decode_chunked(mut input: &[u8]) -> Result<Vec<u8>, ModelError> {
    let mut out = Vec::new();
    loop {
        let line_end = input
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| ModelError::Provider("malformed chunked response".into()))?;
        let size_text = std::str::from_utf8(&input[..line_end])
            .map_err(|_| ModelError::Provider("invalid chunk size".into()))?;
        let size_hex = size_text.split(';').next().unwrap_or(size_text);
        let size = usize::from_str_radix(size_hex.trim(), 16)
            .map_err(|_| ModelError::Provider("invalid chunk size".into()))?;
        input = &input[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        let end = size
            .checked_add(2)
            .ok_or_else(|| ModelError::Provider("invalid chunk length".into()))?;
        if input.len() < end || &input[size..end] != b"\r\n" {
            return Err(ModelError::Provider("truncated chunked response".into()));
        }
        out.extend_from_slice(&input[..size]);
        input = &input[size + 2..];
    }
}

// Guard memory before decoding HTTP; a remote response must never grow the
// local worker buffer without bound. Includes headers and transport framing.
const MAX_HTTP_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

fn read_bounded<R: Read>(reader: &mut R) -> Result<Vec<u8>, ModelError> {
    let mut raw = Vec::new();
    reader
        .take(MAX_HTTP_RESPONSE_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|e| ModelError::Provider(format!("read failed: {e}")))?;
    if raw.len() as u64 > MAX_HTTP_RESPONSE_BYTES {
        return Err(ModelError::Provider("provider response too large".into()));
    }
    Ok(raw)
}

impl ReasoningModel for OpenAiCompatibleModel {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn invoke<'a>(&'a self, request: &'a ModelRequest) -> ModelFuture<'a> {
        let model = self.clone();
        let request = request.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || model.perform_blocking(&request))
                .await
                .map_err(|e| ModelError::Provider(format!("provider task failed: {e}")))?
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct UnavailableModel;
impl ReasoningModel for UnavailableModel {
    fn provider(&self) -> &str {
        "unconfigured"
    }
    fn model(&self) -> &str {
        "unconfigured"
    }
    fn invoke<'a>(&'a self, _request: &'a ModelRequest) -> ModelFuture<'a> {
        Box::pin(async { Err(ModelError::Provider("no reasoning model configured".into())) })
    }
}

#[cfg(test)]
mod bounded_response_tests {
    use super::*;

    #[test]
    fn accepts_small_http_body_without_modification() {
        let expected = b"HTTP/1.1 200 OK\r\n\r\n{}";
        assert_eq!(read_bounded(&mut &expected[..]).unwrap(), expected);
    }

    #[test]
    fn rejects_overflowing_chunk_length_without_panicking() {
        let chunk = b"ffffffffffffffff\r\n";
        assert!(matches!(decode_chunked(chunk), Err(ModelError::Provider(_))));
    }

    #[test]
    fn rejects_response_above_maximum_before_unbounded_allocation() {
        let source = vec![b'x'; MAX_HTTP_RESPONSE_BYTES as usize + 1];
        assert!(matches!(read_bounded(&mut &source[..]), Err(ModelError::Provider(_))));
    }
}
