//! Versioned, human-controlled heuristic weighting policies.
//!
//! Workspace default < Situation override. Neither one rewrites historical input,
//! creates documentary evidence, nor supplies calibrated truth probabilities.
use crate::{authz, epistemic_weights::WeightPolicy};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PolicyStoreError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid heuristic policy input")]
    Invalid,
    #[error("policy ID replay conflict")]
    Conflict,
    #[error("situation is not visible")]
    Invisible,
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPolicy {
    pub policy_id: Uuid,
    pub workspace_id: Uuid,
    /// None = workspace default, Some = single Situation override.
    pub situation_id: Option<Uuid>,
    pub policy: WeightPolicy,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyView {
    pub policy_id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub situation_id: Option<Uuid>,
    pub scope: String,
    pub policy: WeightPolicy,
    pub rationale: String,
    pub recorded_by: Option<Uuid>,
    pub is_default: bool,
    pub is_probability_model: bool,
    pub retroactively_changes_frozen_analysis: bool,
}

pub fn valid(input: &NewPolicy) -> bool {
    input.policy.support_multiplier_pct <= 200
        && input.policy.counter_multiplier_pct <= 200
        && input.policy.per_origin_cap <= 100
        && !input.rationale.trim().is_empty()
        && input.rationale.len() <= 4096
        && !input.rationale.contains('\0')
}

type PolicyRow = (Uuid, Option<Uuid>, i16, i16, i16, String, Uuid);
fn view(workspace: Uuid, row: PolicyRow) -> PolicyView {
    let (id, situation_id, support, counter, cap, rationale, recorded_by) = row;
    PolicyView {
        policy_id: Some(id),
        workspace_id: workspace,
        situation_id,
        scope: if situation_id.is_some() {
            "SITUATION"
        } else {
            "WORKSPACE"
        }
        .into(),
        policy: WeightPolicy {
            support_multiplier_pct: support as u16,
            counter_multiplier_pct: counter as u16,
            per_origin_cap: cap as u16,
        },
        rationale,
        recorded_by: Some(recorded_by),
        is_default: false,
        is_probability_model: false,
        retroactively_changes_frozen_analysis: false,
    }
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, PolicyStoreError> {
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
        return Err(PolicyStoreError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn record(pool: &PgPool, input: &NewPolicy) -> Result<bool, PolicyStoreError> {
    if !valid(input) {
        return Err(PolicyStoreError::Invalid);
    }
    let mut tx = scoped(pool, input.workspace_id, false).await?;
    if let Some(situation) = input.situation_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)"
        ).bind(input.workspace_id).bind(situation).fetch_one(&mut *tx).await?;
        if !exists {
            return Err(PolicyStoreError::Invisible);
        }
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_weight_policies \
         (workspace_id,policy_id,situation_id,support_multiplier_pct,counter_multiplier_pct,per_origin_cap,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING"
    )
    .bind(input.workspace_id)
    .bind(input.policy_id)
    .bind(input.situation_id)
    .bind(i16::try_from(input.policy.support_multiplier_pct).map_err(|_| PolicyStoreError::Invalid)?)
    .bind(i16::try_from(input.policy.counter_multiplier_pct).map_err(|_| PolicyStoreError::Invalid)?)
    .bind(i16::try_from(input.policy.per_origin_cap).map_err(|_| PolicyStoreError::Invalid)?)
    .bind(input.rationale.trim())
    .bind(input.recorded_by)
    .execute(&mut *tx).await?.rows_affected() == 1;

    let actual: Option<(Option<Uuid>,i16,i16,i16,String,Uuid)> = sqlx::query_as(
        "SELECT situation_id,support_multiplier_pct,counter_multiplier_pct,per_origin_cap,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_weight_policies WHERE workspace_id=$1 AND policy_id=$2"
    ).bind(input.workspace_id).bind(input.policy_id).fetch_optional(&mut *tx).await?;
    if actual
        != Some((
            input.situation_id,
            input.policy.support_multiplier_pct as i16,
            input.policy.counter_multiplier_pct as i16,
            input.policy.per_origin_cap as i16,
            input.rationale.trim().into(),
            input.recorded_by,
        ))
    {
        return Err(PolicyStoreError::Conflict);
    }
    tx.commit().await?;
    Ok(inserted)
}

pub async fn effective(
    pool: &PgPool,
    workspace: Uuid,
    situation: Option<Uuid>,
) -> Result<PolicyView, PolicyStoreError> {
    let mut tx = scoped(pool, workspace, true).await?;
    if let Some(id) = situation {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)"
        ).bind(workspace).bind(id).fetch_one(&mut *tx).await?;
        if !exists {
            return Err(PolicyStoreError::Invisible);
        }
    }
    let row: Option<PolicyRow> = sqlx::query_as(
        "SELECT policy_id,situation_id,support_multiplier_pct,counter_multiplier_pct,per_origin_cap,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_weight_policies \
         WHERE workspace_id=$1 AND (situation_id IS NULL OR situation_id=$2) \
         ORDER BY CASE WHEN situation_id IS NULL THEN 1 ELSE 0 END, created_at DESC, policy_id DESC LIMIT 1"
    ).bind(workspace).bind(situation).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    Ok(match row {
        Some(record) => view(workspace, record),
        None => PolicyView {
            policy_id: None,
            workspace_id: workspace,
            situation_id: None,
            scope: "BUILTIN_DEFAULT".into(),
            policy: WeightPolicy::default(),
            rationale: "Neutral weights; no user policy recorded".into(),
            recorded_by: None,
            is_default: true,
            is_probability_model: false,
            retroactively_changes_frozen_analysis: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_explicit_policy_bounds_and_rationale() {
        let mut v = NewPolicy {
            policy_id: Uuid::nil(),
            workspace_id: Uuid::nil(),
            situation_id: None,
            policy: WeightPolicy::default(),
            rationale: "User-directed attention balance".into(),
            recorded_by: Uuid::nil(),
        };
        assert!(valid(&v));
        v.policy.counter_multiplier_pct = 201;
        assert!(!valid(&v));
        v.policy.counter_multiplier_pct = 100;
        v.rationale = " ".into();
        assert!(!valid(&v));
    }
    #[test]
    fn no_policy_view_can_claim_probability() {
        let x = view(
            Uuid::nil(),
            (
                Uuid::nil(),
                None,
                100,
                100,
                100,
                "example".into(),
                Uuid::nil(),
            ),
        );
        assert!(!x.is_probability_model);
        assert!(!x.retroactively_changes_frozen_analysis);
    }
}
