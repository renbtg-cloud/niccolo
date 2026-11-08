//! Cumulative operation-wide model and deadline accounting.
//! A model-call budget is not a measure of epistemic quality. Existing execution
//! still hard-stops on exhaustion; graceful anytime degradation is a later step.
use crate::bff_operations::OperationScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum BudgetError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("operation not visible")]
    Missing,
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
    pub remaining_invocations: i64,
    pub max_total_tokens: i64,
    pub used_total_tokens: i64,
    pub remaining_total_tokens: i64,
    pub deadline_active: bool,
    pub within_deadline: bool,
    /// SQL server clock; 0 means expired (or less than 1 ms remains).
    /// None means no deadline was configured; it does NOT mean unlimited costs.
    pub remaining_deadline_ms: Option<i64>,
    /// Explicit actual reasons why more model calls cannot be started.
    pub stop_reasons: Vec<String>,
    pub can_start_model_call: bool,
    pub budget_is_quality_score: bool,
    /// Exposes accounting; does NOT assert the runtime already produces
    /// partial/degraded conclusions at the deadline.
    pub graceful_quality_degradation_implemented: bool,
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
    let max_i = b
        .and_then(|x| x.get("max_invocations"))
        .and_then(Value::as_i64)
        .unwrap_or(32);
    let max_t = b
        .and_then(|x| x.get("max_total_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(200_000);
    if !(1..=1024).contains(&max_i) || !(1..=10_000_000).contains(&max_t) {
        return Err(BudgetError::Invalid);
    }
    Ok((max_i, max_t))
}

/// Pure display/accounting logic, using a single observed remaining-deadline
/// sample. It never infers truth, certainty, model quality or completed work.
fn summarize(
    input: &Value,
    used_invocations: i64,
    used_total_tokens: i64,
    remaining_deadline_ms: Option<i64>,
) -> Result<BudgetView, BudgetError> {
    let (max_invocations, max_total_tokens) = configured(input)?;
    if used_invocations < 0
        || used_total_tokens < 0
        || remaining_deadline_ms.is_some_and(|ms| ms < 0)
    {
        return Err(BudgetError::Invalid);
    }
    let within_deadline = remaining_deadline_ms.is_none_or(|ms| ms > 0);
    let mut stop_reasons = Vec::new();
    if !within_deadline {
        stop_reasons.push("DEADLINE_EXPIRED".into());
    }
    if used_invocations >= max_invocations {
        stop_reasons.push("INVOCATION_LIMIT".into());
    }
    if used_total_tokens >= max_total_tokens {
        stop_reasons.push("TOKEN_LIMIT".into());
    }
    Ok(BudgetView {
        max_invocations,
        used_invocations,
        remaining_invocations: (max_invocations - used_invocations).max(0),
        max_total_tokens,
        used_total_tokens,
        remaining_total_tokens: (max_total_tokens - used_total_tokens).max(0),
        deadline_active: remaining_deadline_ms.is_some(),
        within_deadline,
        remaining_deadline_ms,
        can_start_model_call: stop_reasons.is_empty(),
        stop_reasons,
        budget_is_quality_score: false,
        graceful_quality_degradation_implemented: false,
    })
}

/// Auth-scoped PostgreSQL observation. No mutation, external provider call,
/// approximate host CPU estimate, or publication side effect.
pub async fn view(pool: &PgPool, s: OperationScope) -> Result<BudgetView, BudgetError> {
    let mut tx = scoped(pool, s.workspace).await?;
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        "SELECT input_json, \
         CASE WHEN deadline_at IS NULL THEN NULL \
         ELSE GREATEST(0, FLOOR(EXTRACT(EPOCH FROM (deadline_at - clock_timestamp()))*1000)::bigint) END \
         FROM niccolo.bff_operations \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.operation)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input, remaining_deadline_ms)) = row else {
        return Err(BudgetError::Missing);
    };
    let input: Value = serde_json::from_str(&input).map_err(|_| BudgetError::Invalid)?;
    let (used_invocations, used_total_tokens): (i64, i64) = sqlx::query_as(
        "SELECT count(*), \
         coalesce(sum(coalesce(prompt_tokens,0)::bigint+coalesce(completion_tokens,0)::bigint),0)::bigint \
         FROM niccolo.model_invocations \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND status='SUCCEEDED'",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.operation)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    summarize(
        &input,
        used_invocations,
        used_total_tokens,
        remaining_deadline_ms,
    )
}

pub async fn require_available(
    pool: &PgPool,
    s: OperationScope,
) -> Result<BudgetView, BudgetError> {
    let v = view(pool, s).await?;
    if !v.within_deadline {
        return Err(BudgetError::Deadline);
    }
    if v.used_invocations >= v.max_invocations {
        return Err(BudgetError::Invocations);
    }
    if v.used_total_tokens >= v.max_total_tokens {
        return Err(BudgetError::Tokens);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_deadline_and_default_budget_are_not_unlimited_quality() {
        let view = summarize(&json!({}), 3, 500, None).unwrap();
        assert_eq!(view.remaining_invocations, 29);
        assert_eq!(view.remaining_total_tokens, 199_500);
        assert_eq!(view.remaining_deadline_ms, None);
        assert!(!view.deadline_active);
        assert!(view.can_start_model_call);
        assert!(!view.budget_is_quality_score);
        assert!(!view.graceful_quality_degradation_implemented);
    }

    #[test]
    fn time_limit_and_money_like_token_limits_remain_separate() {
        let budget = json!({"model_budget":{"max_invocations":2,"max_total_tokens":200}});
        let view = summarize(&budget, 1, 150, Some(1)).unwrap();
        assert_eq!(view.remaining_deadline_ms, Some(1));
        assert!(view.can_start_model_call);
        let expired = summarize(&budget, 2, 220, Some(0)).unwrap();
        assert!(!expired.can_start_model_call);
        assert_eq!(expired.remaining_invocations, 0);
        assert_eq!(expired.remaining_total_tokens, 0);
        assert_eq!(
            expired.stop_reasons,
            vec!["DEADLINE_EXPIRED", "INVOCATION_LIMIT", "TOKEN_LIMIT"]
        );
    }

    #[test]
    fn strict_bounds_and_impossible_negative_observations_fail() {
        assert!(summarize(&json!({"model_budget":{"max_invocations":0}}), 0, 0, None).is_err());
        assert!(summarize(&json!({}), -1, 0, None).is_err());
        assert!(summarize(&json!({}), 1, -1, None).is_err());
        assert!(summarize(&json!({}), 1, 0, Some(-1)).is_err());
    }

    #[test]
    fn token_exhaustion_does_not_claim_a_false_fact() {
        let view = summarize(
            &json!({"model_budget":{"max_total_tokens":20}}),
            0,
            20,
            None,
        )
        .unwrap();
        assert_eq!(view.stop_reasons, vec!["TOKEN_LIMIT"]);
        assert!(!view.budget_is_quality_score);
        assert!(!view.graceful_quality_degradation_implemented);
    }
}
