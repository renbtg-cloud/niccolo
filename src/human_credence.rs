//! Append-only human judgments about analysis claims.
//!
//! Subjective percentages and operational premises are human-authored control
//! inputs, not model probabilities, documentary support, or verified facts.
use crate::{analysis_structure::AnalysisScope, authz};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CredenceError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid human credence")]
    Invalid,
    #[error("claim not visible within analysis")]
    Invisible,
    #[error("credence ID reused with different contents")]
    Conflict,
    #[error("privileged SQL session is prohibited")]
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewHumanCredence {
    pub credence_id: Uuid,
    pub claim_id: Uuid,
    pub confidence_percent: i16,
    pub usage_policy: String,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanCredenceView {
    pub credence_id: Uuid,
    pub claim_id: Uuid,
    pub confidence_percent: i16,
    pub usage_policy: String,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub is_human_declaration: bool,
    pub is_calibrated_probability: bool,
    pub is_documentary_evidence: bool,
    pub makes_claim_a_fact: bool,
    pub part_of_frozen_analysis_input: bool,
}

pub fn valid(input: &NewHumanCredence) -> bool {
    !input.credence_id.is_nil()
        && !input.claim_id.is_nil()
        && !input.recorded_by.is_nil()
        && (0..=100).contains(&input.confidence_percent)
        && matches!(
            input.usage_policy.as_str(),
            "SUBJECTIVE_BELIEF" | "OPERATIONAL_PREMISE"
        )
        && !input.rationale.trim().is_empty()
        && input.rationale.len() <= 4096
        && !input.rationale.chars().any(char::is_control)
}

fn valid_scope(scope: AnalysisScope) -> bool {
    !scope.workspace.is_nil() && !scope.situation.is_nil() && !scope.analysis.is_nil()
}

fn view(row: (Uuid, Uuid, i16, String, String, Uuid)) -> HumanCredenceView {
    let (credence_id, claim_id, confidence_percent, usage_policy, rationale, recorded_by) = row;
    HumanCredenceView {
        credence_id,
        claim_id,
        confidence_percent,
        usage_policy,
        rationale,
        recorded_by,
        is_human_declaration: true,
        is_calibrated_probability: false,
        is_documentary_evidence: false,
        makes_claim_a_fact: false,
        part_of_frozen_analysis_input: false,
    }
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, CredenceError> {
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
        return Err(CredenceError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    input: &NewHumanCredence,
) -> Result<bool, CredenceError> {
    if !valid_scope(scope) || !valid(input) {
        return Err(CredenceError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let claim_visible: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM niccolo.situation_analysis_claims \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.claim_id)
    .fetch_one(&mut *tx)
    .await?;
    if !claim_visible {
        return Err(CredenceError::Invisible);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_human_credence \
         (workspace_id,situation_id,analysis_id,claim_id,credence_id,confidence_percent,usage_policy,rationale,recorded_by_principal_id) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.claim_id)
    .bind(input.credence_id)
    .bind(input.confidence_percent)
    .bind(&input.usage_policy)
    .bind(input.rationale.trim())
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected() == 1;

    let existing: Option<(i16, String, String, Uuid)> = sqlx::query_as(
        "SELECT confidence_percent,usage_policy,rationale,recorded_by_principal_id \
         FROM niccolo.situation_analysis_human_credence \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND credence_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.claim_id)
    .bind(input.credence_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            input.confidence_percent,
            input.usage_policy.clone(),
            input.rationale.trim().into(),
            input.recorded_by,
        ))
    {
        return Err(CredenceError::Conflict);
    }
    tx.commit().await?;
    Ok(inserted)
}

pub async fn list_for_claim(
    pool: &PgPool,
    scope: AnalysisScope,
    claim_id: Uuid,
) -> Result<Vec<HumanCredenceView>, CredenceError> {
    if !valid_scope(scope) || claim_id.is_nil() {
        return Err(CredenceError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM niccolo.situation_analysis_claims \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(CredenceError::Invisible);
    }
    let rows: Vec<(Uuid, Uuid, i16, String, String, Uuid)> = sqlx::query_as(
        "SELECT credence_id,claim_id,confidence_percent,usage_policy,rationale,recorded_by_principal_id \
         FROM niccolo.situation_analysis_human_credence \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 \
         ORDER BY created_at,credence_id LIMIT 512",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(view).collect())
}

pub async fn list_for_analysis(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<HumanCredenceView>, CredenceError> {
    if !valid_scope(scope) {
        return Err(CredenceError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid, Uuid, i16, String, String, Uuid)> = sqlx::query_as(
        "SELECT credence_id,claim_id,confidence_percent,usage_policy,rationale,recorded_by_principal_id \
         FROM niccolo.situation_analysis_human_credence \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         ORDER BY claim_id,created_at,credence_id LIMIT 512",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(view).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> NewHumanCredence {
        NewHumanCredence {
            credence_id: Uuid::from_u128(3),
            claim_id: Uuid::from_u128(1),
            confidence_percent: 90,
            usage_policy: "SUBJECTIVE_BELIEF".into(),
            rationale: "I know contextual facts not in the PDF".into(),
            recorded_by: Uuid::from_u128(2),
        }
    }
    #[test]
    fn nil_credence_identities_are_invalid() {
        let mut value = sample();
        value.credence_id = Uuid::nil();
        assert!(!valid(&value));
        value = sample();
        value.claim_id = Uuid::nil();
        assert!(!valid(&value));
        value = sample();
        value.recorded_by = Uuid::nil();
        assert!(!valid(&value));
    }

    #[test]
    fn human_percentage_is_bounded() {
        assert!(valid(&sample()));
        let mut v = sample();
        v.confidence_percent = 101;
        assert!(!valid(&v));
        v.confidence_percent = -1;
        assert!(!valid(&v));
    }
    #[test]
    fn rationale_rejects_control_characters() {
        for value in ["because\nforged", "because\rforged", "because\tforged", "x\0y"] {
            let mut v = sample();
            v.rationale = value.into();
            assert!(!valid(&v));
        }
    }

    #[test]
    fn operational_premise_is_explicit_not_implicit() {
        let mut v = sample();
        v.usage_policy = "OPERATIONAL_PREMISE".into();
        assert!(valid(&v));
        v.usage_policy = "VERIFIED_FACT".into();
        assert!(!valid(&v));
    }
    #[test]
    fn human_record_cannot_be_transmuted_into_evidence() {
        let v = view((
            Uuid::nil(),
            Uuid::from_u128(1),
            90,
            "SUBJECTIVE_BELIEF".into(),
            "my context".into(),
            Uuid::from_u128(2),
        ));
        assert!(v.is_human_declaration);
        assert!(!v.is_documentary_evidence);
        assert!(!v.is_calibrated_probability);
        assert!(!v.makes_claim_a_fact);
        assert!(!v.part_of_frozen_analysis_input);
    }
}
