//! Targeted clarification questions bound to the frozen analysis input that motivated them.
//!
//! A clarification is a question, not evidence. Later Situation changes never
//! rewrite which input revision caused an earlier clarification to be proposed.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ClarificationError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid clarification request")]
    Invalid,
    #[error("clarification identifier replay conflicts with stored content")]
    Conflict,
    #[error("analysis or question is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clarification {
    pub question_id: Uuid,
    pub text: String,
    pub reason: String,
    pub proposed_by: Uuid,
    pub input_revision: i64,
    pub current_revision: i64,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClarificationValue {
    pub question_id: Uuid,
    pub discriminates_interpretation_ids: Vec<Uuid>,
    pub possible_outcome_count: usize,
    pub decision_relevance: Option<u16>,
    pub acquisition_cost: Option<u16>,
    pub cheaper_alternative: Option<String>,
}

type ClarificationValueRow = (Uuid, Option<i16>, Option<i16>, Option<String>);

fn valid(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ClarificationError> {
    let mut tx = pool.begin().await?;
    if read_only {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ClarificationError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

#[expect(
    clippy::too_many_arguments,
    reason = "frozen-analysis provenance and question identity are independent inputs"
)]
pub async fn propose(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    question: Uuid,
    text: &str,
    reason: &str,
) -> Result<Clarification, ClarificationError> {
    if !valid(text, 4096) || !valid(reason, 2048) {
        return Err(ClarificationError::Invalid);
    }
    let text = text.trim();
    let reason = reason.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let revision: Option<(i64, i64)> = sqlx::query_as(
        "SELECT a.input_revision,s.current_revision \
         FROM niccolo.situation_analysis_snapshots a \
         JOIN niccolo.situations s USING(workspace_id,situation_id) \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3 FOR UPDATE OF s",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input_revision, current_revision)) = revision else {
        return Err(ClarificationError::Invisible);
    };

    let question_inserted = sqlx::query(
        "INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) \
         VALUES($1,$2,$3) ON CONFLICT(workspace_id,question_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(question)
    .bind(text)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !question_inserted {
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT question FROM niccolo.investigative_questions \
             WHERE workspace_id=$1 AND question_id=$2",
        )
        .bind(workspace)
        .bind(question)
        .fetch_optional(&mut *tx)
        .await?;
        if stored.as_deref() != Some(text) {
            return Err(ClarificationError::Conflict);
        }
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_clarification_questions \
         (workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .bind(reason)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT reason,proposed_by_principal_id \
         FROM niccolo.situation_analysis_clarification_questions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND question_id=$4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((stored_reason, stored_actor)) = stored else {
        return Err(ClarificationError::Invisible);
    };
    if stored_reason != reason || stored_actor != actor {
        return Err(ClarificationError::Conflict);
    }
    tx.commit().await?;
    Ok(Clarification {
        question_id: question,
        text: text.to_owned(),
        reason: stored_reason,
        proposed_by: stored_actor,
        input_revision,
        current_revision,
        replayed: !inserted,
    })
}

pub async fn values(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<Vec<ClarificationValue>, ClarificationError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ClarificationError::Invisible);
    }

    let rows: Vec<ClarificationValueRow> = sqlx::query_as(
        "SELECT question_id,decision_relevance,acquisition_cost,cheaper_alternative
         FROM niccolo.situation_analysis_clarification_values
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
         ORDER BY question_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for (question_id, decision_relevance, acquisition_cost, cheaper_alternative) in rows {
        let interpretations: Vec<Uuid> = sqlx::query_scalar(
            "SELECT interpretation_id
             FROM niccolo.situation_analysis_clarification_interpretations
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND question_id=$4
             ORDER BY interpretation_id",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .bind(question_id)
        .fetch_all(&mut *tx)
        .await?;
        let possible_outcome_count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint
             FROM niccolo.situation_analysis_clarification_outcomes
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND question_id=$4",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .bind(question_id)
        .fetch_one(&mut *tx)
        .await?;
        out.push(ClarificationValue {
            question_id,
            discriminates_interpretation_ids: interpretations,
            possible_outcome_count: usize::try_from(possible_outcome_count)
                .map_err(|_| ClarificationError::Invalid)?,
            decision_relevance: decision_relevance.map(|v| v as u16),
            acquisition_cost: acquisition_cost.map(|v| v as u16),
            cheaper_alternative,
        });
    }
    tx.commit().await?;
    Ok(out)
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<Clarification>, Option<Uuid>), ClarificationError> {
    if !(1..=100).contains(&limit) {
        return Err(ClarificationError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let revisions: Option<(i64, i64)> = sqlx::query_as(
        "SELECT a.input_revision,s.current_revision \
         FROM niccolo.situation_analysis_snapshots a \
         JOIN niccolo.situations s USING(workspace_id,situation_id) \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input_revision, current_revision)) = revisions else {
        return Err(ClarificationError::Invisible);
    };
    let rows: Vec<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT q.question_id,q.question,c.reason,c.proposed_by_principal_id \
         FROM niccolo.situation_analysis_clarification_questions c \
         JOIN niccolo.investigative_questions q USING(workspace_id,question_id) \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 \
           AND ($4::uuid IS NULL OR c.question_id>$4) \
         ORDER BY c.question_id LIMIT $5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    let more = rows.len() as i64 > limit;
    let questions = rows
        .into_iter()
        .take(limit as usize)
        .map(|(question_id, text, reason, proposed_by)| Clarification {
            question_id,
            text,
            reason,
            proposed_by,
            input_revision,
            current_revision,
            replayed: false,
        })
        .collect::<Vec<_>>();
    let next_after = more.then(|| questions.last().expect("non-empty page").question_id);
    tx.commit().await?;
    Ok((questions, next_after))
}
