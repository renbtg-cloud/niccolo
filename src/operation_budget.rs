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
    #[error("operation deadline entered reserved deterministic finalization window")]
    FinalizationReserve,
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
    /// Number of completed calls whose provider did not report all usage.
    /// Used tokens are a lower bound when this is nonzero.
    pub unreported_token_usage_invocations: i64,
    pub token_usage_complete: bool,
    pub remaining_total_tokens_is_upper_bound: bool,
    pub deadline_active: bool,
    pub within_deadline: bool,
    /// SQL server clock; 0 means expired (or less than 1 ms remains).
    /// None means no deadline was configured; it does NOT mean unlimited costs.
    pub remaining_deadline_ms: Option<i64>,
    /// Wall-clock reserve that is kept for deterministic packaging/checkpointing.
    /// It does not create extra time and only applies when a deadline exists.
    pub finalization_reserve_ms: i64,
    pub within_model_call_window: bool,
    /// Explicit actual reasons why more model calls cannot be started.
    pub stop_reasons: Vec<String>,
    pub can_start_model_call: bool,
    pub budget_is_quality_score: bool,
    /// Agentic execution can now return a visibly provisional result at model
    /// budget/deadline boundaries while preserving deterministic finalization.
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

fn configured(input: &Value) -> Result<(i64, i64, i64), BudgetError> {
    let budget = match input.get("model_budget") {
        None => None,
        Some(Value::Object(fields)) => Some(fields),
        Some(_) => return Err(BudgetError::Invalid),
    };
    // Explicitly malformed user-supplied limits must not silently become defaults.
    // Defaults apply only when a limit was omitted, not when it has the wrong type.
    let read_limit = |name: &str, default: i64| -> Result<i64, BudgetError> {
        match budget.and_then(|fields| fields.get(name)) {
            None => Ok(default),
            Some(value) => value.as_i64().ok_or(BudgetError::Invalid),
        }
    };
    let max_i = read_limit("max_invocations", 32)?;
    let max_t = read_limit("max_total_tokens", 200_000)?;
    let finalization_reserve_ms = read_limit("finalization_reserve_ms", 5_000)?;
    if !(1..=1024).contains(&max_i)
        || !(1..=10_000_000).contains(&max_t)
        || !(0..=300_000).contains(&finalization_reserve_ms)
    {
        return Err(BudgetError::Invalid);
    }
    Ok((max_i, max_t, finalization_reserve_ms))
}

/// Pure display/accounting logic, using a single observed remaining-deadline
/// sample. It never infers truth, certainty, model quality or completed work.
fn summarize(
    input: &Value,
    used_invocations: i64,
    used_total_tokens: i64,
    remaining_deadline_ms: Option<i64>,
) -> Result<BudgetView, BudgetError> {
    let (max_invocations, max_total_tokens, finalization_reserve_ms) = configured(input)?;
    if used_invocations < 0
        || used_total_tokens < 0
        || remaining_deadline_ms.is_some_and(|ms| ms < 0)
    {
        return Err(BudgetError::Invalid);
    }
    let within_deadline = remaining_deadline_ms.is_none_or(|ms| ms > 0);
    let within_model_call_window =
        remaining_deadline_ms.is_none_or(|ms| ms > finalization_reserve_ms);
    let mut stop_reasons = Vec::new();
    if !within_deadline {
        stop_reasons.push("DEADLINE_EXPIRED".into());
    } else if !within_model_call_window {
        stop_reasons.push("FINALIZATION_RESERVE".into());
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
        unreported_token_usage_invocations: 0,
        token_usage_complete: true,
        remaining_total_tokens_is_upper_bound: false,
        deadline_active: remaining_deadline_ms.is_some(),
        within_deadline,
        remaining_deadline_ms,
        finalization_reserve_ms,
        within_model_call_window,
        can_start_model_call: stop_reasons.is_empty(),
        stop_reasons,
        budget_is_quality_score: false,
        graceful_quality_degradation_implemented: true,
    })
}

/// A successful call with absent token usage is not free. Expose the
/// accounting uncertainty instead of claiming the token budget is exact.
fn mark_unknown_usage(mut view: BudgetView, missing: i64) -> Result<BudgetView, BudgetError> {
    if missing < 0 || missing > view.used_invocations {
        return Err(BudgetError::Invalid);
    }
    view.unreported_token_usage_invocations = missing;
    view.token_usage_complete = missing == 0;
    view.remaining_total_tokens_is_upper_bound = missing > 0;
    Ok(view)
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
    let (used_invocations, used_total_tokens, missing_usage): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), \
         coalesce(sum(coalesce(prompt_tokens,0)::bigint+coalesce(completion_tokens,0)::bigint),0)::bigint, \
         count(*) FILTER (WHERE prompt_tokens IS NULL OR completion_tokens IS NULL) \
         FROM niccolo.model_invocations \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND status='SUCCEEDED'",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.operation)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    mark_unknown_usage(
        summarize(
            &input,
            used_invocations,
            used_total_tokens,
            remaining_deadline_ms,
        )?,
        missing_usage,
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
    if !v.within_model_call_window {
        return Err(BudgetError::FinalizationReserve);
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
    fn missing_provider_usage_is_not_treated_as_free_tokens() {
        let base = summarize(&json!({}), 3, 1000, None).unwrap();
        let uncertain = mark_unknown_usage(base.clone(), 2).unwrap();
        assert_eq!(uncertain.unreported_token_usage_invocations, 2);
        assert!(!uncertain.token_usage_complete);
        assert!(uncertain.remaining_total_tokens_is_upper_bound);
        assert_eq!(uncertain.used_total_tokens, 1000);
        assert_eq!(uncertain.remaining_total_tokens, 199_000);
        assert!(uncertain.can_start_model_call);
        assert!(mark_unknown_usage(base, 4).is_err());
    }

    #[test]
    fn no_deadline_and_default_budget_are_not_unlimited_quality() {
        let view = summarize(&json!({}), 3, 500, None).unwrap();
        assert_eq!(view.remaining_invocations, 29);
        assert_eq!(view.remaining_total_tokens, 199_500);
        assert_eq!(view.remaining_deadline_ms, None);
        assert!(!view.deadline_active);
        assert!(view.can_start_model_call);
        assert!(!view.budget_is_quality_score);
        assert!(view.graceful_quality_degradation_implemented);
        assert_eq!(view.finalization_reserve_ms, 5_000);
        assert!(view.within_model_call_window);
    }

    #[test]
    fn time_limit_and_money_like_token_limits_remain_separate() {
        let budget = json!({"model_budget":{"max_invocations":2,"max_total_tokens":200}});
        let view = summarize(&budget, 1, 150, Some(6_000)).unwrap();
        assert_eq!(view.remaining_deadline_ms, Some(6_000));
        assert!(view.within_model_call_window);
        assert!(view.can_start_model_call);
        let reserve = summarize(&budget, 1, 150, Some(5_000)).unwrap();
        assert!(!reserve.within_model_call_window);
        assert!(!reserve.can_start_model_call);
        assert_eq!(reserve.stop_reasons, vec!["FINALIZATION_RESERVE"]);
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
    fn malformed_explicit_budgets_never_fall_back_to_default() {
        for budget in [
            json!({"model_budget":null}),
            json!({"model_budget":"unlimited"}),
            json!({"model_budget":[]}),
            json!({"model_budget":{"max_invocations":null}}),
            json!({"model_budget":{"max_invocations":"10"}}),
            json!({"model_budget":{"max_invocations":true}}),
            json!({"model_budget":{"max_total_tokens":2.5}}),
            json!({"model_budget":{"max_total_tokens":-1}}),
            json!({"model_budget":{"finalization_reserve_ms":-1}}),
            json!({"model_budget":{"finalization_reserve_ms":300001}}),
        ] {
            assert!(
                matches!(summarize(&budget, 0, 0, None), Err(BudgetError::Invalid)),
                "accepted malformed budget: {budget}"
            );
        }
        let valid = summarize(&json!({"model_budget":{"max_invocations":5}}), 0, 0, None).unwrap();
        assert_eq!(valid.max_invocations, 5);
        assert_eq!(valid.max_total_tokens, 200_000);
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
        assert!(view.graceful_quality_degradation_implemented);
    }
}
