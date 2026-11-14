//! v0.112-v0.117: explicit assumptions and alternative interpretations.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum AssumptionError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid assumption input")]
    Invalid,
    #[error("assumption replay conflicts with stored content")]
    Conflict,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssumptionInput {
    pub assumption_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub statement: String,
    pub basis_note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssumptionEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlternativeInput {
    pub alternative_id: Uuid,
    pub claim_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssumptionView {
    pub assumption_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub statement: String,
    pub basis_note: String,
    pub latest_status: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlternativeView {
    pub alternative_id: Uuid,
    pub claim_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub automatically_ranked: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateInterpretationView {
    pub interpretation_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub is_fact: bool,
    pub automatically_ranked: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisLens {
    pub assumptions: Vec<AssumptionView>,
    pub alternatives: Vec<AlternativeView>,
    pub assumptions_are_facts: bool,
    pub alternatives_are_automatically_ranked: bool,
}
type AssumptionRow = (Uuid, Option<Uuid>, String, String, Option<String>);
type AlternativeRow = (Uuid, Uuid, String, String);

async fn scoped(
    pool: &PgPool,
    w: Uuid,
    ro: bool,
) -> Result<Transaction<'_, Postgres>, AssumptionError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if ro {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let p: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if p {
        return Err(AssumptionError::Privileged);
    };
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
fn valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
}
pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    a: &AssumptionInput,
) -> Result<(), AssumptionError> {
    if !valid(&a.statement) || !valid(&a.basis_note) {
        return Err(AssumptionError::Invalid);
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_assumptions(workspace_id,situation_id,analysis_id,assumption_id,claim_id,statement,basis_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.assumption_id).bind(a.claim_id).bind(&a.statement).bind(&a.basis_note).bind(a.recorded_by).execute(&mut *tx).await?;
    let stored: Option<(Option<Uuid>, String, String, Uuid)> = sqlx::query_as(
        "SELECT claim_id,statement,basis_note,recorded_by_principal_id
           FROM niccolo.situation_analysis_assumptions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND assumption_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(a.assumption_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((a.claim_id, a.statement.clone(), a.basis_note.clone(), a.recorded_by)) {
        return Err(AssumptionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn append_event(
    pool: &PgPool,
    scope: AnalysisScope,
    assumption: Uuid,
    e: &AssumptionEvent,
) -> Result<(), AssumptionError> {
    if !matches!(
        e.kind.as_str(),
        "CHALLENGED" | "RETAINED_FOR_ANALYSIS" | "RETRACTED_FOR_ANALYSIS"
    ) || !valid(&e.note)
    {
        return Err(AssumptionError::Invalid);
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_assumption_events(workspace_id,situation_id,analysis_id,assumption_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(assumption).bind(e.event_id).bind(&e.kind).bind(&e.note).bind(e.recorded_by).execute(&mut *tx).await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT assumption_id,event_kind,note,recorded_by_principal_id
           FROM niccolo.situation_analysis_assumption_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND event_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(e.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((assumption, e.kind.clone(), e.note.clone(), e.recorded_by)) {
        return Err(AssumptionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn add_alternative(
    pool: &PgPool,
    scope: AnalysisScope,
    a: &AlternativeInput,
) -> Result<(), AssumptionError> {
    if !valid(&a.text) || !valid(&a.rationale) {
        return Err(AssumptionError::Invalid);
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_claim_alternatives(workspace_id,situation_id,analysis_id,claim_id,alternative_id,alternative_text,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.claim_id).bind(a.alternative_id).bind(&a.text).bind(&a.rationale).bind(a.recorded_by).execute(&mut *tx).await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT claim_id,alternative_text,rationale,recorded_by_principal_id
           FROM niccolo.situation_analysis_claim_alternatives
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND alternative_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(a.alternative_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((a.claim_id, a.text.clone(), a.rationale.clone(), a.recorded_by)) {
        return Err(AssumptionError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn candidate_interpretations(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<CandidateInterpretationView>, AssumptionError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT interpretation_id,interpretation_text,rationale FROM niccolo.situation_analysis_candidate_interpretations WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY created_at,interpretation_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(interpretation_id, text, rationale)| CandidateInterpretationView {
                interpretation_id,
                text,
                rationale,
                is_fact: false,
                automatically_ranked: false,
            },
        )
        .collect())
}

pub async fn lens(pool: &PgPool, scope: AnalysisScope) -> Result<AnalysisLens, AssumptionError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let arows: Vec<AssumptionRow> = sqlx::query_as("SELECT a.assumption_id,a.claim_id,a.statement,a.basis_note,(SELECT e.event_kind FROM niccolo.situation_analysis_assumption_events e WHERE e.workspace_id=a.workspace_id AND e.situation_id=a.situation_id AND e.analysis_id=a.analysis_id AND e.assumption_id=a.assumption_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_analysis_assumptions a WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3 ORDER BY a.assumption_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    let brows: Vec<AlternativeRow> = sqlx::query_as("SELECT alternative_id,claim_id,alternative_text,rationale FROM niccolo.situation_analysis_claim_alternatives WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY claim_id,alternative_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(AnalysisLens {
        assumptions: arows
            .into_iter()
            .map(
                |(assumption_id, claim_id, statement, basis_note, s)| AssumptionView {
                    assumption_id,
                    claim_id,
                    statement,
                    basis_note,
                    latest_status: s.unwrap_or_else(|| "RECORDED".into()),
                },
            )
            .collect(),
        alternatives: brows
            .into_iter()
            .map(
                |(alternative_id, claim_id, text, rationale)| AlternativeView {
                    alternative_id,
                    claim_id,
                    text,
                    rationale,
                    automatically_ranked: false,
                },
            )
            .collect(),
        assumptions_are_facts: false,
        alternatives_are_automatically_ranked: false,
    })
}
pub fn canonical_json(v: &AnalysisLens) -> Result<String, serde_json::Error> {
    serde_json::to_string(v)
}
pub fn sha256(v: &AnalysisLens) -> Result<String, serde_json::Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_json(v)?.as_bytes())
    ))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assumption_text_rejects_control_characters() {
        for value in ["statement\nforged", "basis\rforged", "note\tforged", "x\0y"] {
            assert!(!valid(value));
        }
        assert!(valid("Competing interpretation with explicit uncertainty."));
    }
}
