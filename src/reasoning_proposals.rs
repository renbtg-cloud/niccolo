//! Durable proposal boundary for model-generated reasoning updates.
//!
//! A validated model update can be staged before any of its items are treated as
//! canonical reasoning objects. This first slice preserves current promotion
//! behavior while making proposal identity and promotion decisions explicit.
use crate::{analysis_structure::AnalysisScope, reasoning_updates::ReasoningUpdate};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid reasoning proposal")]
    Invalid,
    #[error("reasoning proposal replay conflict")]
    Conflict,
    #[error("proposal is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalSummary {
    pub proposal_id: Uuid,
    pub step_kind: String,
    pub item_count: usize,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionSummary {
    pub proposal_id: Uuid,
    pub promoted_count: usize,
    pub replayed_count: usize,
    pub proposal_is_truth: bool,
}

const STEP_KINDS: &[&str] = &[
    "ASSESS_CONTEXT",
    "CHECK_ASSUMPTIONS",
    "PROPOSE_CLARIFICATION",
    "CHECK_ALTERNATIVES",
];

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, ProposalError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ProposalError::Invalid);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn proposal_items(
    update: &ReasoningUpdate,
) -> Result<Vec<(&'static str, Uuid, Value)>, ProposalError> {
    let mut out = Vec::new();
    for item in &update.candidate_interpretations {
        out.push((
            "HYPOTHESIS",
            item.interpretation_id,
            serde_json::to_value(item)?,
        ));
    }
    for item in &update.assumptions {
        out.push((
            "ASSUMPTION",
            item.assumption_id,
            serde_json::to_value(item)?,
        ));
    }
    for item in &update.evidence_gaps {
        out.push(("EVIDENCE_GAP", item.gap_id, serde_json::to_value(item)?));
    }
    for item in &update.clarifications {
        out.push((
            "CLARIFICATION",
            item.question_id,
            serde_json::to_value(item)?,
        ));
    }
    for item in &update.source_queries {
        out.push(("SOURCE_QUERY", item.query_id, serde_json::to_value(item)?));
    }
    Ok(out)
}

pub async fn stage(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    proposal_id: Uuid,
    step_kind: &str,
    update: &ReasoningUpdate,
) -> Result<ProposalSummary, ProposalError> {
    if !STEP_KINDS.contains(&step_kind) {
        return Err(ProposalError::Invalid);
    }
    let payload = serde_json::to_value(update)?;
    let items = proposal_items(update)?;
    let mut tx = scoped(pool, scope.workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_reasoning_proposals \
         (workspace_id,situation_id,analysis_id,proposal_id,step_kind,proposal_payload,proposed_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(proposal_id)
    .bind(step_kind)
    .bind(Json(payload.clone()))
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let stored: Option<(String, Json<Value>, Uuid)> = sqlx::query_as(
        "SELECT step_kind,proposal_payload,proposed_by_principal_id \
         FROM niccolo.situation_analysis_reasoning_proposals \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(proposal_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((step_kind.to_owned(), Json(payload), actor)) {
        return Err(ProposalError::Conflict);
    }

    for (ordinal, (kind, item_id, item_payload)) in items.iter().enumerate() {
        sqlx::query(
            "INSERT INTO niccolo.situation_analysis_reasoning_proposal_items \
             (workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id,ordinal,item_payload) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(proposal_id)
        .bind(*kind)
        .bind(*item_id)
        .bind(i32::try_from(ordinal).map_err(|_| ProposalError::Invalid)?)
        .bind(Json(item_payload.clone()))
        .execute(&mut *tx)
        .await?;
        let row: Option<(i32, Json<Value>)> = sqlx::query_as(
            "SELECT ordinal,item_payload FROM niccolo.situation_analysis_reasoning_proposal_items \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4 \
               AND item_kind=$5 AND item_id=$6",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(proposal_id)
        .bind(*kind)
        .bind(*item_id)
        .fetch_optional(&mut *tx)
        .await?;
        if row != Some((ordinal as i32, Json(item_payload.clone()))) {
            return Err(ProposalError::Conflict);
        }
    }
    let stored_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_reasoning_proposal_items \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(proposal_id)
    .fetch_one(&mut *tx)
    .await?;
    if usize::try_from(stored_count).ok() != Some(items.len()) {
        return Err(ProposalError::Conflict);
    }
    tx.commit().await?;
    Ok(ProposalSummary {
        proposal_id,
        step_kind: step_kind.to_owned(),
        item_count: items.len(),
        replayed: !inserted,
    })
}

async fn canonical_exists(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
    kind: &str,
    item_id: Uuid,
) -> Result<bool, ProposalError> {
    let table = match kind {
        "HYPOTHESIS" => "situation_analysis_candidate_interpretations",
        "ASSUMPTION" => "situation_analysis_assumptions",
        "EVIDENCE_GAP" => "situation_analysis_evidence_gaps",
        "CLARIFICATION" => "situation_analysis_clarification_questions",
        "SOURCE_QUERY" => "situation_analysis_observation_queries",
        _ => return Err(ProposalError::Invalid),
    };
    let id_col = match kind {
        "HYPOTHESIS" => "interpretation_id",
        "ASSUMPTION" => "assumption_id",
        "EVIDENCE_GAP" => "gap_id",
        "CLARIFICATION" => "question_id",
        "SOURCE_QUERY" => "query_id",
        _ => unreachable!(),
    };
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM niccolo.{table} \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND {id_col}=$4)"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(item_id)
        .fetch_one(&mut **tx)
        .await?)
}

pub async fn mark_all_promoted(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    proposal_id: Uuid,
) -> Result<PromotionSummary, ProposalError> {
    const RATIONALE: &str = "Compatibility promotion after validated canonical persistence.";
    let mut tx = scoped(pool, scope.workspace).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_reasoning_proposals \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(proposal_id)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ProposalError::Invisible);
    }
    let items: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT item_kind,item_id FROM niccolo.situation_analysis_reasoning_proposal_items \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4 \
         ORDER BY item_kind,ordinal",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(proposal_id)
    .fetch_all(&mut *tx)
    .await?;

    let mut replayed_count = 0usize;
    for (kind, item_id) in &items {
        if !canonical_exists(&mut tx, scope, kind, *item_id).await? {
            return Err(ProposalError::Invalid);
        }
        let inserted = sqlx::query(
            "INSERT INTO niccolo.situation_analysis_reasoning_proposal_dispositions \
             (workspace_id,situation_id,analysis_id,proposal_id,item_kind,item_id,event_id,disposition,decision_source,rationale,decided_by_principal_id) \
             VALUES($1,$2,$3,$4,$5,$6,$6,'PROMOTED','DETERMINISTIC',$7,$8) ON CONFLICT DO NOTHING",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(proposal_id)
        .bind(kind)
        .bind(*item_id)
        .bind(RATIONALE)
        .bind(actor)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !inserted {
            replayed_count += 1;
        }
        let row: Option<(String, String, String, Uuid)> = sqlx::query_as(
            "SELECT disposition,decision_source,rationale,decided_by_principal_id \
             FROM niccolo.situation_analysis_reasoning_proposal_dispositions \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND proposal_id=$4 \
               AND item_kind=$5 AND item_id=$6 AND event_id=$6",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(proposal_id)
        .bind(kind)
        .bind(*item_id)
        .fetch_optional(&mut *tx)
        .await?;
        if row
            != Some((
                "PROMOTED".to_owned(),
                "DETERMINISTIC".to_owned(),
                RATIONALE.to_owned(),
                actor,
            ))
        {
            return Err(ProposalError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(PromotionSummary {
        proposal_id,
        promoted_count: items.len(),
        replayed_count,
        proposal_is_truth: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_runtime_steps_are_closed() {
        assert!(STEP_KINDS.contains(&"CHECK_ALTERNATIVES"));
        assert!(!STEP_KINDS.contains(&"SYNTHESIZE"));
    }
}
