//! Purpose-bound decision readiness.
//!
//! "Ready" means the current evidence can support a human decision for the stated
//! purpose while explicitly naming unknowns that could still reverse it. It never
//! grants authority, executes an action or claims uncertainty is exhausted.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DecisionReadinessError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid decision-readiness input")]
    Invalid,
    #[error("decision unavailable")]
    Invisible,
    #[error("decision-readiness replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReversalConditionInput {
    pub condition_id: Uuid,
    pub condition_text: String,
    pub impact: String,
    pub acquisition_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionReadinessInput {
    pub assessment_id: Uuid,
    pub decision_id: Uuid,
    pub subject_label: String,
    pub readiness: String,
    pub rationale: String,
    pub unresolved_unknowns: String,
    pub reversal_conditions: Vec<ReversalConditionInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReversalConditionView {
    pub condition_id: Uuid,
    pub condition_text: String,
    pub impact: String,
    pub acquisition_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionReadinessView {
    pub assessment_id: Uuid,
    pub decision_id: Uuid,
    pub subject_label: String,
    pub readiness: String,
    pub rationale: String,
    pub unresolved_unknowns: String,
    pub reversal_conditions: Vec<ReversalConditionView>,
    pub human_owns_decision: bool,
    pub readiness_claims_omniscience: bool,
    pub replayed: bool,
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

fn valid(input: &DecisionReadinessInput) -> bool {
    if input.assessment_id.is_nil()
        || input.decision_id.is_nil()
        || !valid_text(&input.subject_label, 512)
        || !valid_text(&input.rationale, 4096)
        || !valid_text(&input.unresolved_unknowns, 4096)
        || !matches!(input.readiness.as_str(), "READY" | "NOT_READY" | "ESCALATE")
        || input.reversal_conditions.len() > 32
    {
        return false;
    }
    let mut ids = std::collections::BTreeSet::new();
    input.reversal_conditions.iter().all(|c| {
        !c.condition_id.is_nil()
            && ids.insert(c.condition_id)
            && valid_text(&c.condition_text, 4096)
            && matches!(c.impact.as_str(), "MAY_REVERSE" | "REQUIRES_REVIEW" | "LOW_IMPACT")
            && c.acquisition_note.as_ref().is_none_or(|x| valid_text(x, 4096))
    })
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, DecisionReadinessError> {
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
        return Err(DecisionReadinessError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

async fn load(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
    decision_id: Uuid,
    assessment_id: Uuid,
    replayed: bool,
) -> Result<Option<DecisionReadinessView>, sqlx::Error> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT subject_label,readiness,rationale,unresolved_unknowns
           FROM niccolo.situation_analysis_decision_readiness
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND decision_id=$4 AND assessment_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(decision_id)
    .bind(assessment_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((subject_label, readiness, rationale, unresolved_unknowns)) = row else {
        return Ok(None);
    };
    let conditions: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT condition_id,condition_text,impact,acquisition_note
           FROM niccolo.situation_analysis_decision_reversal_conditions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND decision_id=$4 AND assessment_id=$5
          ORDER BY condition_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(decision_id)
    .bind(assessment_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(Some(DecisionReadinessView {
        assessment_id,
        decision_id,
        subject_label,
        readiness,
        rationale,
        unresolved_unknowns,
        reversal_conditions: conditions
            .into_iter()
            .map(|(condition_id, condition_text, impact, acquisition_note)| ReversalConditionView {
                condition_id,
                condition_text,
                impact,
                acquisition_note,
            })
            .collect(),
        human_owns_decision: true,
        readiness_claims_omniscience: false,
        replayed,
    }))
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    input: &DecisionReadinessInput,
) -> Result<DecisionReadinessView, DecisionReadinessError> {
    if !valid(input) {
        return Err(DecisionReadinessError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_decision_points
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND decision_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.decision_id)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(DecisionReadinessError::Invisible);
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_decision_readiness
         (workspace_id,situation_id,analysis_id,decision_id,assessment_id,subject_label,
          readiness,rationale,unresolved_unknowns,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.decision_id)
    .bind(input.assessment_id)
    .bind(input.subject_label.trim())
    .bind(&input.readiness)
    .bind(input.rationale.trim())
    .bind(input.unresolved_unknowns.trim())
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    for condition in &input.reversal_conditions {
        sqlx::query(
            "INSERT INTO niccolo.situation_analysis_decision_reversal_conditions
             (workspace_id,situation_id,analysis_id,decision_id,assessment_id,condition_id,
              condition_text,impact,acquisition_note,recorded_by_principal_id)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(input.decision_id)
        .bind(input.assessment_id)
        .bind(condition.condition_id)
        .bind(condition.condition_text.trim())
        .bind(&condition.impact)
        .bind(condition.acquisition_note.as_deref().map(str::trim))
        .bind(actor)
        .execute(&mut *tx)
        .await?;
    }

    let Some(view) = load(&mut tx, scope, input.decision_id, input.assessment_id, !inserted).await?
    else {
        return Err(DecisionReadinessError::Invisible);
    };
    if view.subject_label != input.subject_label.trim()
        || view.readiness != input.readiness
        || view.rationale != input.rationale.trim()
        || view.unresolved_unknowns != input.unresolved_unknowns.trim()
        || view.reversal_conditions.len() != input.reversal_conditions.len()
    {
        return Err(DecisionReadinessError::Conflict);
    }
    for condition in &input.reversal_conditions {
        let Some(stored) = view
            .reversal_conditions
            .iter()
            .find(|x| x.condition_id == condition.condition_id)
        else {
            return Err(DecisionReadinessError::Conflict);
        };
        if stored.condition_text != condition.condition_text.trim()
            || stored.impact != condition.impact
            || stored.acquisition_note
                != condition
                    .acquisition_note
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_owned)
        {
            return Err(DecisionReadinessError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(view)
}

pub async fn latest(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<DecisionReadinessView>, DecisionReadinessError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(DecisionReadinessError::Invisible);
    }
    let ids: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT DISTINCT ON (decision_id) decision_id,assessment_id
           FROM niccolo.situation_analysis_decision_readiness
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
          ORDER BY decision_id,created_at DESC,assessment_id DESC",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    let mut out = Vec::with_capacity(ids.len());
    for (decision_id, assessment_id) in ids {
        if let Some(view) = load(&mut tx, scope, decision_id, assessment_id, false).await? {
            out.push(view);
        }
    }
    tx.commit().await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_can_coexist_with_material_reversal_condition() {
        let x = DecisionReadinessInput {
            assessment_id: Uuid::new_v4(),
            decision_id: Uuid::new_v4(),
            subject_label: "Kessler".into(),
            readiness: "READY".into(),
            rationale: "Current evidence supports a bounded human decision.".into(),
            unresolved_unknowns: "Personal mobile messages were not collected.".into(),
            reversal_conditions: vec![ReversalConditionInput {
                condition_id: Uuid::new_v4(),
                condition_text: "A contemporaneous record showing explicit contrary authorization.".into(),
                impact: "MAY_REVERSE".into(),
                acquisition_note: Some("If recovered, reanalyse before action.".into()),
            }],
        };
        assert!(valid(&x));
    }

    #[test]
    fn nil_and_duplicate_reversal_identities_are_rejected() {
        let mut input = DecisionReadinessInput {
            assessment_id: Uuid::new_v4(),
            decision_id: Uuid::new_v4(),
            subject_label: "A human decision".into(),
            readiness: "NOT_READY".into(),
            rationale: "Evidence is incomplete.".into(),
            unresolved_unknowns: "A missing original record.".into(),
            reversal_conditions: vec![ReversalConditionInput {
                condition_id: Uuid::new_v4(),
                condition_text: "An original contradicting the summary.".into(),
                impact: "MAY_REVERSE".into(),
                acquisition_note: None,
            }],
        };
        assert!(valid(&input));
        input.assessment_id = Uuid::nil();
        assert!(!valid(&input));
        input.assessment_id = Uuid::new_v4();
        input.decision_id = Uuid::nil();
        assert!(!valid(&input));
        input.decision_id = Uuid::new_v4();
        let original_id = input.reversal_conditions[0].condition_id;
        input.reversal_conditions[0].condition_id = Uuid::nil();
        assert!(!valid(&input));
        input.reversal_conditions[0].condition_id = original_id;
        input.reversal_conditions.push(input.reversal_conditions[0].clone());
        assert!(!valid(&input));
    }

    #[test]
    fn unknown_readiness_state_is_rejected() {
        let x = DecisionReadinessInput {
            assessment_id: Uuid::new_v4(),
            decision_id: Uuid::new_v4(),
            subject_label: "Actor".into(),
            readiness: "CERTAIN".into(),
            rationale: "No".into(),
            unresolved_unknowns: "Unknown".into(),
            reversal_conditions: vec![],
        };
        assert!(!valid(&x));
    }
}
