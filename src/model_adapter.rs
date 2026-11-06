//! v0.276-v0.290: model-adapter boundary plus durable invocation ledger.
//! Adapter output is data to inspect; successful transport does not establish truth.
use crate::bff_operations::OperationScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{future::Future, pin::Pin, time::Instant};
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
                "ASSESS_CONTEXT" | "CHECK_ASSUMPTIONS"
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
                if policy.max_total_tokens.is_some_and(|max| total > max) {
                    last_error = ModelError::Budget;
                } else {
                    let response_json = serde_json::to_string(&response.value)?;
                    if response_json.len() > 1_048_576 {
                        return Err(ModelError::Invalid);
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
