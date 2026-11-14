//! v0.31-v0.35: structured analysis claims, explicit support, append-only human review,
//! direct-reanalysis claim lineage, and recorded clarification impact.
//!
//! These records describe what an analysis said and what inputs it used. They do not establish
//! that a claim is true, that linked sources are independent, or that user context is evidence.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid claim request")]
    Invalid,
    #[error("analysis or claim is not visible")]
    Invisible,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
    pub created_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimReview {
    pub review_id: Uuid,
    pub claim_id: Uuid,
    pub kind: String,
    pub note: String,
    pub reviewer: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimLineage {
    pub lineage_id: Uuid,
    pub prior_analysis_id: Uuid,
    pub prior_claim_id: Uuid,
    pub new_analysis_id: Uuid,
    pub new_claim_id: Option<Uuid>,
    pub relation: String,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClarificationImpact {
    pub impact_id: Uuid,
    pub source_analysis_id: Uuid,
    pub question_id: Uuid,
    pub answer_id: Uuid,
    pub contribution_id: Uuid,
    pub target_analysis_id: Uuid,
    pub target_claim_id: Uuid,
    pub recorded_effect: String,
    pub note: String,
    pub recorded_by: Uuid,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ClaimError> {
    let mut tx = pool.begin().await?;
    let mode = if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        // Idempotent writes use PostgreSQL READ COMMITTED. REPEATABLE READ can
        // raise SQLSTATE 40001 when many callers converge on the same unique
        // claim, even though ON CONFLICT makes the write semantically safe.
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    };
    sqlx::query(mode).execute(&mut *tx).await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ClaimError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_claim_kind(v: &str) -> bool {
    matches!(
        v,
        "OBSERVATION" | "INFERENCE" | "POSSIBILITY" | "RECOMMENDATION" | "UNCERTAINTY"
    )
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

pub async fn record_claim(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    principal: Uuid,
    claim: NewClaim,
) -> Result<Claim, ClaimError> {
    if claim.ordinal < 1
        || !valid_claim_kind(&claim.kind)
        || !valid_text(&claim.text, 8192)
        || claim
            .qualification
            .as_ref()
            .is_some_and(|qualification| !valid_text(qualification, 4096))
    {
        return Err(ClaimError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claims \
         (workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,qualification,created_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim.claim_id)
    .bind(claim.ordinal)
    .bind(&claim.kind)
    .bind(&claim.text)
    .bind(&claim.qualification)
    .bind(principal)
    .execute(&mut *tx)
    .await?;

    let row: Option<(i32, String, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT ordinal,claim_kind,claim_text,qualification,created_by_principal_id \
         FROM niccolo.situation_analysis_claims \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim.claim_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((ordinal, kind, text, qualification, created_by)) = row else {
        return Err(ClaimError::Invisible);
    };
    if inserted.rows_affected() == 0
        && (ordinal != claim.ordinal
            || kind != claim.kind
            || text != claim.text
            || qualification != claim.qualification
            || created_by != principal)
    {
        return Err(ClaimError::Conflict);
    }
    tx.commit().await?;
    Ok(Claim {
        claim_id: claim.claim_id,
        ordinal,
        kind,
        text,
        qualification,
        created_by,
    })
}

pub async fn list_claims(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<Vec<Claim>, ClaimError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<(Uuid, i32, String, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT claim_id,ordinal,claim_kind,claim_text,qualification,created_by_principal_id \
         FROM niccolo.situation_analysis_claims \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         ORDER BY ordinal,claim_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(claim_id, ordinal, kind, text, qualification, created_by)| Claim {
                claim_id,
                ordinal,
                kind,
                text,
                qualification,
                created_by,
            },
        )
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn link_fragment(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    claim_id: Uuid,
    evidence_id: Uuid,
    relation: &str,
    principal: Uuid,
) -> Result<(), ClaimError> {
    if !matches!(
        relation,
        "SUPPORTS_AS_USED" | "CONTRADICTS_AS_USED" | "CONTEXT_ONLY"
    ) {
        return Err(ClaimError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_fragments \
         (workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .bind(evidence_id)
    .bind(relation)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT relation,recorded_by_principal_id
           FROM niccolo.situation_analysis_claim_fragments
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND claim_id=$4 AND evidence_id=$5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .bind(evidence_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((relation.to_owned(), principal)) {
        return Err(ClaimError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn link_contribution(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    claim_id: Uuid,
    contribution_id: Uuid,
    relation: &str,
    principal: Uuid,
) -> Result<(), ClaimError> {
    if !matches!(
        relation,
        "INFORMS" | "QUALIFIES" | "CONFLICTS_WITH_REPORTED_CONTEXT"
    ) {
        return Err(ClaimError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_contributions \
         (workspace_id,situation_id,analysis_id,claim_id,contribution_id,relation,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .bind(contribution_id)
    .bind(relation)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT relation,recorded_by_principal_id
           FROM niccolo.situation_analysis_claim_contributions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND claim_id=$4 AND contribution_id=$5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .bind(contribution_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((relation.to_owned(), principal)) {
        return Err(ClaimError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn append_review(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    claim_id: Uuid,
    review_id: Uuid,
    kind: &str,
    note: &str,
    reviewer: Uuid,
) -> Result<ClaimReview, ClaimError> {
    if !matches!(
        kind,
        "CHALLENGED" | "INSUFFICIENT_EVIDENCE" | "UNKNOWN_TO_USER" | "CORRECTION" | "COMMENT"
    ) || !valid_text(note, 4096)
    {
        return Err(ClaimError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_reviews \
         (workspace_id,situation_id,analysis_id,claim_id,review_id,review_kind,note,reviewer_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .bind(review_id)
    .bind(kind)
    .bind(note)
    .bind(reviewer)
    .execute(&mut *tx)
    .await?;
    let row: Option<(String, String, Uuid)> = sqlx::query_as(
        "SELECT review_kind,note,reviewer_principal_id FROM niccolo.situation_analysis_claim_reviews \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND review_id=$5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .bind(review_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored_kind, stored_note, stored_reviewer)) = row else {
        return Err(ClaimError::Invisible);
    };
    if stored_kind != kind || stored_note != note || stored_reviewer != reviewer {
        return Err(ClaimError::Conflict);
    }
    tx.commit().await?;
    Ok(ClaimReview {
        review_id,
        claim_id,
        kind: stored_kind,
        note: stored_note,
        reviewer: stored_reviewer,
    })
}

pub async fn record_lineage(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    lineage: ClaimLineage,
) -> Result<(), ClaimError> {
    if !matches!(
        lineage.relation.as_str(),
        "RETAINED" | "REFINED" | "REVERSED" | "DROPPED" | "SPLIT" | "MERGED"
    ) || !valid_text(&lineage.rationale, 4096)
        || (lineage.relation == "DROPPED") != lineage.new_claim_id.is_none()
    {
        return Err(ClaimError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_lineage \
         (workspace_id,situation_id,lineage_id,prior_analysis_id,prior_claim_id,new_analysis_id,new_claim_id,relation,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(lineage.lineage_id)
    .bind(lineage.prior_analysis_id)
    .bind(lineage.prior_claim_id)
    .bind(lineage.new_analysis_id)
    .bind(lineage.new_claim_id)
    .bind(&lineage.relation)
    .bind(&lineage.rationale)
    .bind(lineage.recorded_by)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn record_clarification_impact(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    impact: ClarificationImpact,
) -> Result<(), ClaimError> {
    if impact.source_analysis_id == impact.target_analysis_id
        || !matches!(
            impact.recorded_effect.as_str(),
            "INFORMS" | "QUALIFIES" | "RESOLVES_FOR_THIS_ANALYSIS" | "INTRODUCES_UNCERTAINTY"
        )
        || !valid_text(&impact.note, 4096)
    {
        return Err(ClaimError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_clarification_claim_impacts \
         (workspace_id,situation_id,impact_id,source_analysis_id,question_id,answer_id,contribution_id,target_analysis_id,target_claim_id,recorded_effect,note,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(impact.impact_id)
    .bind(impact.source_analysis_id)
    .bind(impact.question_id)
    .bind(impact.answer_id)
    .bind(impact.contribution_id)
    .bind(impact.target_analysis_id)
    .bind(impact.target_claim_id)
    .bind(&impact.recorded_effect)
    .bind(&impact.note)
    .bind(impact.recorded_by)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn list_reviews(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    claim_id: Uuid,
) -> Result<Vec<ClaimReview>, ClaimError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT review_id,review_kind,note,reviewer_principal_id \
         FROM niccolo.situation_analysis_claim_reviews \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 \
         ORDER BY created_at,review_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(review_id, kind, note, reviewer)| ClaimReview {
            review_id,
            claim_id,
            kind,
            note,
            reviewer,
        })
        .collect())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_text_fields_reject_control_characters() {
        for value in ["claim\nforged", "qualification\rforged", "note\tforged", "x\0y"] {
            assert!(!valid_text(value, 8192));
        }
        assert!(valid_text(
            "Inference remains provisional pending independent corroboration.",
            8192
        ));
    }
}
