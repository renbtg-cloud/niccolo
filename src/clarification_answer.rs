//! Human answers to targeted clarifications.
//!
//! An answer is new attributed Situation context. It never rewrites the frozen
//! analysis input that caused the question and is never promoted to evidence.
use crate::situation_store::{self, SituationStoreError};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ClarificationAnswerError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid clarification answer")]
    Invalid,
    #[error("clarification answer identifier conflicts with stored content")]
    Conflict,
    #[error("analysis clarification is not visible")]
    Invisible,
    #[error("situation storage failure: {0}")]
    Situation(#[from] SituationStoreError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClarificationAnswer {
    pub answer_id: Uuid,
    pub question_id: Uuid,
    pub analysis_id: Uuid,
    pub contribution_id: Uuid,
    pub text: String,
    pub answered_by: Uuid,
    pub question_input_revision: i64,
    pub answer_revision: i64,
    pub current_revision: i64,
    pub replayed: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ClarificationAnswerError> {
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
        return Err(ClarificationAnswerError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

type AnswerRow = (Uuid, String, Uuid, i64, i64);

async fn question_input_revision(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    question: Uuid,
) -> Result<i64, ClarificationAnswerError> {
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT a.input_revision FROM niccolo.situation_analysis_clarification_questions q \
         JOIN niccolo.situation_analysis_snapshots a \
           ON a.workspace_id=q.workspace_id AND a.situation_id=q.situation_id AND a.analysis_id=q.analysis_id \
         WHERE q.workspace_id=$1 AND q.situation_id=$2 AND q.analysis_id=$3 AND q.question_id=$4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .fetch_optional(&mut **tx)
    .await?;
    revision.ok_or(ClarificationAnswerError::Invisible)
}

#[expect(
    clippy::too_many_arguments,
    reason = "answer identity and clarification provenance are independent"
)]
pub async fn answer(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    question: Uuid,
    answer: Uuid,
    text: &str,
) -> Result<ClarificationAnswer, ClarificationAnswerError> {
    let text = situation_store::clean_text(text).map_err(|_| ClarificationAnswerError::Invalid)?;
    let mut tx = scoped(pool, workspace, false).await?;
    let input_revision =
        question_input_revision(&mut tx, workspace, situation, analysis, question).await?;

    let existing: Option<AnswerRow> = sqlx::query_as(
        "SELECT a.contribution_id,c.contribution_text,a.answered_by_principal_id,a.answer_revision,s.current_revision \
         FROM niccolo.situation_analysis_clarification_answers a \
         JOIN niccolo.situation_contributions c \
           ON c.workspace_id=a.workspace_id AND c.situation_id=a.situation_id AND c.contribution_id=a.contribution_id \
         JOIN niccolo.situations s ON s.workspace_id=a.workspace_id AND s.situation_id=a.situation_id \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3 AND a.question_id=$4 AND a.answer_id=$5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .bind(answer)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((contribution_id, stored_text, answered_by, answer_revision, current_revision)) =
        existing
    {
        if stored_text != text || answered_by != actor || contribution_id != answer {
            return Err(ClarificationAnswerError::Conflict);
        }
        tx.commit().await?;
        return Ok(ClarificationAnswer {
            answer_id: answer,
            question_id: question,
            analysis_id: analysis,
            contribution_id,
            text: stored_text,
            answered_by,
            question_input_revision: input_revision,
            answer_revision,
            current_revision,
            replayed: true,
        });
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_contributions \
         (workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text) \
         VALUES($1,$2,$3,$4,'USER_REPORTED',$5) \
         ON CONFLICT(workspace_id,situation_id,contribution_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(answer)
    .bind(actor)
    .bind(&text)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let raced: Option<AnswerRow> = sqlx::query_as(
            "SELECT a.contribution_id,c.contribution_text,a.answered_by_principal_id,a.answer_revision,s.current_revision \
             FROM niccolo.situation_analysis_clarification_answers a \
             JOIN niccolo.situation_contributions c \
               ON c.workspace_id=a.workspace_id AND c.situation_id=a.situation_id AND c.contribution_id=a.contribution_id \
             JOIN niccolo.situations s ON s.workspace_id=a.workspace_id AND s.situation_id=a.situation_id \
             WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3 AND a.question_id=$4 AND a.answer_id=$5",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .bind(question)
        .bind(answer)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((contribution_id, stored_text, answered_by, answer_revision, current_revision)) =
            raced
            && stored_text == text
            && answered_by == actor
            && contribution_id == answer
        {
            tx.commit().await?;
            return Ok(ClarificationAnswer {
                answer_id: answer,
                question_id: question,
                analysis_id: analysis,
                contribution_id,
                text: stored_text,
                answered_by,
                question_input_revision: input_revision,
                answer_revision,
                current_revision,
                replayed: true,
            });
        }
        return Err(ClarificationAnswerError::Conflict);
    }
    let snapshot =
        situation_store::checkpoint(&mut tx, workspace, situation, actor, "CONTRIBUTION_ADDED")
            .await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_clarification_answers \
         (workspace_id,situation_id,analysis_id,question_id,answer_id,contribution_id,answered_by_principal_id,answer_revision) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .bind(answer)
    .bind(answer)
    .bind(actor)
    .bind(snapshot.revision)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ClarificationAnswer {
        answer_id: answer,
        question_id: question,
        analysis_id: analysis,
        contribution_id: answer,
        text,
        answered_by: actor,
        question_input_revision: input_revision,
        answer_revision: snapshot.revision,
        current_revision: snapshot.revision,
        replayed: false,
    })
}

/// IDs already answered by a human for this frozen analysis.
/// Answers are attributed context, never independent documentary evidence.
pub async fn answered_question_ids(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<std::collections::BTreeSet<Uuid>, ClarificationAnswerError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT question_id
           FROM niccolo.situation_analysis_clarification_answers
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ids.into_iter().collect())
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    question: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<ClarificationAnswer>, Option<Uuid>), ClarificationAnswerError> {
    if !(1..=100).contains(&limit) {
        return Err(ClarificationAnswerError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let input_revision =
        question_input_revision(&mut tx, workspace, situation, analysis, question).await?;
    let current_revision: i64 = sqlx::query_scalar(
        "SELECT current_revision FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    let rows: Vec<(Uuid, Uuid, String, Uuid, i64)> = sqlx::query_as(
        "SELECT a.answer_id,a.contribution_id,c.contribution_text,a.answered_by_principal_id,a.answer_revision \
         FROM niccolo.situation_analysis_clarification_answers a \
         JOIN niccolo.situation_contributions c \
           ON c.workspace_id=a.workspace_id AND c.situation_id=a.situation_id AND c.contribution_id=a.contribution_id \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3 AND a.question_id=$4 \
           AND ($5::uuid IS NULL OR a.answer_id>$5) ORDER BY a.answer_id LIMIT $6",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(question)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    let more = rows.len() as i64 > limit;
    let answers = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(answer_id, contribution_id, text, answered_by, answer_revision)| {
                ClarificationAnswer {
                    answer_id,
                    question_id: question,
                    analysis_id: analysis,
                    contribution_id,
                    text,
                    answered_by,
                    question_input_revision: input_revision,
                    answer_revision,
                    current_revision,
                    replayed: false,
                }
            },
        )
        .collect::<Vec<_>>();
    let next_after = more.then(|| answers.last().expect("non-empty page").answer_id);
    tx.commit().await?;
    Ok((answers, next_after))
}
