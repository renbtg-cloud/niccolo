//! v0.81-v0.89: explicit human review workflow and inspectable rationale/condition records.
//! These records document human/agent reasoning state; none of them establish truth or start work automatically.
use crate::analysis_structure::AnalysisScope;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReviewWorkflowError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid review workflow input")]
    Invalid,
    #[error("review object is not visible")]
    Invisible,
    #[error("review workflow replay conflicts with stored content")]
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewSessionScope {
    pub workspace: Uuid,
    pub situation: Uuid,
    pub session: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewSessionDigest {
    pub session_id: Uuid,
    pub analysis_id: Option<Uuid>,
    pub purpose: String,
    pub note_count: i64,
    pub bookmark_count: i64,
    pub ended: bool,
    pub session_means_analysis_approved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionNote {
    pub note_id: Uuid,
    pub kind: String,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub text: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseRationale {
    pub rationale_id: Uuid,
    pub reason_kind: String,
    pub reason: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimQuestion {
    pub claim_id: Uuid,
    pub question_id: Uuid,
    pub relation_kind: String,
    pub answered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionCondition {
    pub claim_id: Uuid,
    pub condition_id: Uuid,
    pub trigger_kind: String,
    pub text: String,
    pub latest_event: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReanalysisCandidate {
    pub claim_id: Uuid,
    pub condition_id: Uuid,
    pub trigger_kind: String,
    pub condition_text: String,
    pub latest_event_id: Uuid,
    pub suggestion_only: bool,
    pub analysis_started: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ReviewWorkflowError> {
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
        return Err(ReviewWorkflowError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

pub async fn start_session(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    session: Uuid,
    analysis: Option<Uuid>,
    purpose: &str,
    principal: Uuid,
) -> Result<(), ReviewWorkflowError> {
    if !valid_text(purpose, 2048) {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_sessions \
         (workspace_id,situation_id,session_id,analysis_id,purpose,started_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(session)
    .bind(analysis)
    .bind(purpose)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT analysis_id,purpose,started_by_principal_id
           FROM niccolo.situation_review_sessions
          WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(session)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((analysis, purpose.to_owned(), principal)) {
        return Err(ReviewWorkflowError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn end_session(
    pool: &PgPool,
    scope: ReviewSessionScope,
    end_id: Uuid,
    note: Option<&str>,
    principal: Uuid,
) -> Result<(), ReviewWorkflowError> {
    if note.is_some_and(|v| !valid_text(v, 4096)) {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_session_ends \
         (workspace_id,situation_id,session_id,end_id,note,ended_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(end_id)
    .bind(note)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn add_session_note(
    pool: &PgPool,
    scope: ReviewSessionScope,
    item: &SessionNote,
) -> Result<(), ReviewWorkflowError> {
    if !matches!(item.kind.as_str(), "NOTE" | "BOOKMARK")
        || !valid_text(&item.text, 4096)
        || item.target_type.is_some() != item.target_id.is_some()
    {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_session_notes \
         (workspace_id,situation_id,session_id,note_id,note_kind,target_type,target_id,note_text,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(item.note_id)
    .bind(&item.kind)
    .bind(&item.target_type)
    .bind(item.target_id)
    .bind(&item.text)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(String, Option<String>, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT note_kind,target_type,target_id,note_text,recorded_by_principal_id
           FROM niccolo.situation_review_session_notes
          WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3 AND note_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .bind(item.note_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            item.kind.clone(),
            item.target_type.clone(),
            item.target_id,
            item.text.clone(),
            item.recorded_by,
        ))
    {
        return Err(ReviewWorkflowError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn session_digest(
    pool: &PgPool,
    scope: ReviewSessionScope,
) -> Result<ReviewSessionDigest, ReviewWorkflowError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    type Row = (Option<Uuid>, String, i64, i64, bool);
    let row: Option<Row> = sqlx::query_as(
        "SELECT s.analysis_id,s.purpose, \
         count(n.note_id) FILTER (WHERE n.note_kind='NOTE'), \
         count(n.note_id) FILTER (WHERE n.note_kind='BOOKMARK'), \
         EXISTS(SELECT 1 FROM niccolo.situation_review_session_ends e WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id AND e.session_id=s.session_id) \
         FROM niccolo.situation_review_sessions s \
         LEFT JOIN niccolo.situation_review_session_notes n USING(workspace_id,situation_id,session_id) \
         WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.session_id=$3 \
         GROUP BY s.analysis_id,s.purpose,s.workspace_id,s.situation_id,s.session_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.session)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let Some((analysis_id, purpose, note_count, bookmark_count, ended)) = row else {
        return Err(ReviewWorkflowError::Invisible);
    };
    Ok(ReviewSessionDigest {
        session_id: scope.session,
        analysis_id,
        purpose,
        note_count,
        bookmark_count,
        ended,
        session_means_analysis_approved: false,
    })
}

pub async fn record_context_use_rationale(
    pool: &PgPool,
    scope: AnalysisScope,
    contribution: Uuid,
    item: &UseRationale,
) -> Result<(), ReviewWorkflowError> {
    if !matches!(
        item.reason_kind.as_str(),
        "GOAL_RELEVANCE" | "CLARIFICATION_DEPENDENCY" | "DISAMBIGUATION" | "CROSS_CHECK" | "OTHER"
    ) || !valid_text(&item.reason, 2048)
    {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_used_context_rationales \
         (workspace_id,situation_id,analysis_id,contribution_id,rationale_id,reason_kind,reason,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(contribution)
    .bind(item.rationale_id)
    .bind(&item.reason_kind)
    .bind(&item.reason)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT contribution_id,reason_kind,reason,recorded_by_principal_id
           FROM niccolo.situation_analysis_used_context_rationales
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND rationale_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.rationale_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((contribution, item.reason_kind.clone(), item.reason.clone(), item.recorded_by)) {
        return Err(ReviewWorkflowError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn record_fragment_rationale(
    pool: &PgPool,
    scope: AnalysisScope,
    evidence: Uuid,
    item: &UseRationale,
) -> Result<(), ReviewWorkflowError> {
    if !matches!(
        item.reason_kind.as_str(),
        "DIRECT_SUPPORT" | "CROSS_CHECK" | "CONTEXT" | "CONTRADICTION" | "OTHER"
    ) || !valid_text(&item.reason, 2048)
    {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_examined_fragment_rationales \
         (workspace_id,situation_id,analysis_id,evidence_id,rationale_id,reason_kind,reason,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(evidence)
    .bind(item.rationale_id)
    .bind(&item.reason_kind)
    .bind(&item.reason)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT evidence_id,reason_kind,reason,recorded_by_principal_id
           FROM niccolo.situation_analysis_examined_fragment_rationales
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND rationale_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.rationale_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((evidence, item.reason_kind.clone(), item.reason.clone(), item.recorded_by)) {
        return Err(ReviewWorkflowError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn link_claim_question(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
    question: Uuid,
    relation_kind: &str,
    principal: Uuid,
) -> Result<(), ReviewWorkflowError> {
    if !matches!(
        relation_kind,
        "OPEN_QUESTION" | "WOULD_CLARIFY" | "CHALLENGES"
    ) {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_question_links \
         (workspace_id,situation_id,analysis_id,claim_id,question_id,relation_kind,linked_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim)
    .bind(question)
    .bind(relation_kind)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT relation_kind,linked_by_principal_id
           FROM niccolo.situation_analysis_claim_question_links
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND claim_id=$4 AND question_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim)
    .bind(question)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((relation_kind.to_owned(), principal)) {
        return Err(ReviewWorkflowError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn claim_questions(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<ClaimQuestion>, ReviewWorkflowError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid, Uuid, String, bool)> = sqlx::query_as(
        "SELECT l.claim_id,l.question_id,l.relation_kind, \
         EXISTS(SELECT 1 FROM niccolo.situation_analysis_clarification_answers a \
           WHERE a.workspace_id=l.workspace_id AND a.situation_id=l.situation_id AND a.analysis_id=l.analysis_id AND a.question_id=l.question_id) \
         FROM niccolo.situation_analysis_claim_question_links l \
         WHERE l.workspace_id=$1 AND l.situation_id=$2 AND l.analysis_id=$3 \
         ORDER BY l.claim_id,l.question_id,l.relation_kind",
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
            |(claim_id, question_id, relation_kind, answered)| ClaimQuestion {
                claim_id,
                question_id,
                relation_kind,
                answered,
            },
        )
        .collect())
}

pub async fn record_revision_condition(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
    condition: Uuid,
    trigger_kind: &str,
    text: &str,
    principal: Uuid,
) -> Result<(), ReviewWorkflowError> {
    if !matches!(
        trigger_kind,
        "NEW_EVIDENCE" | "CLARIFICATION" | "CONTEXT_CORRECTION" | "OTHER"
    ) || !valid_text(text, 4096)
    {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_revision_conditions \
         (workspace_id,situation_id,analysis_id,claim_id,condition_id,trigger_kind,condition_text,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim)
    .bind(condition)
    .bind(trigger_kind)
    .bind(text)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn append_condition_event(
    pool: &PgPool,
    scope: AnalysisScope,
    claim: Uuid,
    condition: Uuid,
    event: Uuid,
    kind: &str,
    note: &str,
    related_evidence: Option<Uuid>,
    related_contribution: Option<Uuid>,
    principal: Uuid,
) -> Result<(), ReviewWorkflowError> {
    if !matches!(kind, "MET" | "NOT_MET" | "REOPENED")
        || !valid_text(note, 4096)
        || (related_evidence.is_some() && related_contribution.is_some())
    {
        return Err(ReviewWorkflowError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_condition_events \
         (workspace_id,situation_id,analysis_id,claim_id,condition_id,event_id,event_kind,resolution_note,related_evidence_id,related_contribution_id,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(claim)
    .bind(condition)
    .bind(event)
    .bind(kind)
    .bind(note)
    .bind(related_evidence)
    .bind(related_contribution)
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn revision_conditions(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<RevisionCondition>, ReviewWorkflowError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid, Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT c.claim_id,c.condition_id,c.trigger_kind,c.condition_text, \
          (SELECT e.event_kind FROM niccolo.situation_analysis_claim_condition_events e \
           WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id \
             AND e.claim_id=c.claim_id AND e.condition_id=c.condition_id \
           ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) \
         FROM niccolo.situation_analysis_claim_revision_conditions c \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 \
         ORDER BY c.claim_id,c.condition_id",
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
            |(claim_id, condition_id, trigger_kind, text, latest_event)| RevisionCondition {
                claim_id,
                condition_id,
                trigger_kind,
                text,
                latest_event,
            },
        )
        .collect())
}

pub async fn reanalysis_candidates(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<ReanalysisCandidate>, ReviewWorkflowError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    type Row = (Uuid, Uuid, String, String, Uuid);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT c.claim_id,c.condition_id,c.trigger_kind,c.condition_text,e.event_id \
         FROM niccolo.situation_analysis_claim_revision_conditions c \
         JOIN LATERAL (SELECT event_id,event_kind FROM niccolo.situation_analysis_claim_condition_events e \
           WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id \
             AND e.claim_id=c.claim_id AND e.condition_id=c.condition_id \
           ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) e ON true \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND e.event_kind='MET' \
         ORDER BY c.claim_id,c.condition_id",
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
            |(claim_id, condition_id, trigger_kind, condition_text, latest_event_id)| {
                ReanalysisCandidate {
                    claim_id,
                    condition_id,
                    trigger_kind,
                    condition_text,
                    latest_event_id,
                    suggestion_only: true,
                    analysis_started: false,
                }
            },
        )
        .collect())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_workflow_text_rejects_control_characters() {
        for value in ["purpose\nforged", "note\rforged", "reason\tforged", "x\0y"] {
            assert!(!valid_text(value, 4096));
        }
        assert!(valid_text("Review unresolved contradictions without implying approval.", 4096));
    }
}
