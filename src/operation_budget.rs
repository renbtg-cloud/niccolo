//! v1.101-v1.200: cumulative operation-wide model/deadline budget.
//! Per-call limits do not replace a whole-agentic-run budget.
use crate::bff_operations::OperationScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid operation model budget")]
    Invalid,
    #[error("operation deadline exceeded")]
    Deadline,
    #[error("operation invocation budget exhausted")]
    Invocations,
    #[error("operation token budget exhausted")]
    Tokens,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetView {
    pub max_invocations: i64,
    pub used_invocations: i64,
    pub max_total_tokens: i64,
    pub used_total_tokens: i64,
    pub deadline_active: bool,
    pub within_deadline: bool,
    pub budget_is_quality_score: bool,
}
async fn scoped(pool: &PgPool, w: uuid::Uuid) -> Result<Transaction<'_, Postgres>, BudgetError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
fn configured(input: &Value) -> Result<(i64, i64), BudgetError> {
    let b = input.get("model_budget");
    let max_i = b.and_then(|x| x.get("max_invocations")).and_then(Value::as_i64).unwrap_or(32);
    let max_t = b.and_then(|x| x.get("max_total_tokens")).and_then(Value::as_i64).unwrap_or(200_000);
    if !(1..=1024).contains(&max_i) || !(1..=10_000_000).contains(&max_t) { return Err(BudgetError::Invalid); }
    Ok((max_i, max_t))
}
pub async fn view(pool: &PgPool, s: OperationScope) -> Result<BudgetView, BudgetError> {
    let mut tx = scoped(pool, s.workspace).await?;
    let row:Option<(String,bool,bool)>=sqlx::query_as("SELECT input_json,deadline_at IS NOT NULL,deadline_at IS NULL OR deadline_at>clock_timestamp() FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3")
        .bind(s.workspace).bind(s.situation).bind(s.operation).fetch_optional(&mut *tx).await?;
    let Some((input, deadline_active, within_deadline)) = row else { return Err(BudgetError::Invalid); };
    let input: Value = serde_json::from_str(&input).map_err(|_| BudgetError::Invalid)?;
    let (max_invocations, max_total_tokens) = configured(&input)?;
    let(used_invocations,used_total_tokens):(i64,i64)=sqlx::query_as("SELECT count(*),coalesce(sum(coalesce(prompt_tokens,0)::bigint+coalesce(completion_tokens,0)::bigint),0)::bigint FROM niccolo.model_invocations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND status='SUCCEEDED'")
        .bind(s.workspace).bind(s.situation).bind(s.operation).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(BudgetView{max_invocations,used_invocations,max_total_tokens,used_total_tokens,deadline_active,within_deadline,budget_is_quality_score:false})
}
pub async fn require_available(pool:&PgPool,s:OperationScope)->Result<BudgetView,BudgetError>{
    let v=view(pool,s).await?;
    if !v.within_deadline{return Err(BudgetError::Deadline);}
    if v.used_invocations>=v.max_invocations{return Err(BudgetError::Invocations);}
    if v.used_total_tokens>=v.max_total_tokens{return Err(BudgetError::Tokens);}
    Ok(v)
}
