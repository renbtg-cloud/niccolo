//! User-contributed investigative questions and durable response events.
//! Explicitly preserves REJECTED vs INSUFFICIENT_EVIDENCE vs UNKNOWN.
//! A person's feedback is not automatically evidence of an event or truth.
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("privileged database role refused")]
    Privileged,
    #[error("invalid question, reason, or pagination")]
    Invalid,
    #[error("identifier replay does not match its existing record")]
    Conflict,
    #[error("question or workspace invisible")]
    Invisible,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanResponse {
    Rejected,
    InsufficientEvidence,
    Unknown,
}
impl HumanResponse {
    pub fn sql(self) -> &'static str {
        match self {
            Self::Rejected => "REJECTED",
            Self::InsufficientEvidence => "INSUFFICIENT_EVIDENCE",
            Self::Unknown => "UNKNOWN",
        }
    }
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "rejected" => Some(Self::Rejected),
            "insufficient_evidence" => Some(Self::InsufficientEvidence),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackEvent {
    pub event_id: Uuid,
    pub event_no: i64,
    pub question_id: Uuid,
    pub response: String,
    pub human_reason: String,
    pub actor_principal_id: Uuid,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub question_id: Uuid,
    pub text: String,
    pub status: String,
}
fn valid(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, FeedbackError> {
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
        return Err(FeedbackError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
/// A question's ID is owned by one workspace; identical retries are no-ops.
pub async fn create_question(
    pool: &PgPool,
    workspace: Uuid,
    id: Uuid,
    text: &str,
) -> Result<bool, FeedbackError> {
    if workspace.is_nil() || id.is_nil() || !valid(text, 4096) {
        return Err(FeedbackError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let visible: Option<Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1")
            .bind(workspace)
            .fetch_optional(&mut *tx)
            .await?;
    if visible.is_none() {
        return Err(FeedbackError::Invisible);
    }
    let inserted=sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,$3) ON CONFLICT (workspace_id,question_id) DO NOTHING")
        .bind(workspace).bind(id).bind(text).execute(&mut *tx).await?.rows_affected()==1;
    if !inserted {
        let original:Option<String>=sqlx::query_scalar("SELECT question FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2")
            .bind(workspace).bind(id).fetch_optional(&mut *tx).await?;
        if original.as_deref() != Some(text) {
            return Err(FeedbackError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(inserted)
}
pub async fn list_questions(
    pool: &PgPool,
    workspace: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<Question>, Option<Uuid>), FeedbackError> {
    if workspace.is_nil() || after.is_some_and(|id| id.is_nil()) || !(1..=100).contains(&limit) {
        return Err(FeedbackError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let rows:Vec<(Uuid,String,String)>=sqlx::query_as(
        "SELECT question_id,question,status FROM niccolo.investigative_questions WHERE workspace_id=$1 AND ($2::uuid IS NULL OR question_id>$2) ORDER BY question_id LIMIT $3")
        .bind(workspace).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let output: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(|(question_id, text, status)| Question {
            question_id,
            text,
            status,
        })
        .collect();
    let next = if more {
        output.last().map(|r| r.question_id)
    } else {
        None
    };
    Ok((output, next))
}
pub async fn record_feedback(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    event_id: Uuid,
    question_id: Uuid,
    response: HumanResponse,
    reason: &str,
) -> Result<FeedbackEvent, FeedbackError> {
    if workspace.is_nil()
        || actor.is_nil()
        || event_id.is_nil()
        || question_id.is_nil()
        || !valid(reason, 2048)
    {
        return Err(FeedbackError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    // Lock workspace to make independent corrections and retries deterministically serialized.
    let exists: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1 FOR UPDATE",
    )
    .bind(workspace)
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_none() {
        return Err(FeedbackError::Invisible);
    }
    let found:Option<Uuid>=sqlx::query_scalar("SELECT question_id FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2")
        .bind(workspace).bind(question_id).fetch_optional(&mut *tx).await?;
    if found.is_none() {
        return Err(FeedbackError::Invisible);
    }
    let inserted:Option<i64>=sqlx::query_scalar(
        "INSERT INTO niccolo.question_feedback_events(workspace_id,event_id,question_id,response,human_reason,actor_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(workspace_id,event_id) DO NOTHING RETURNING event_no")
        .bind(workspace).bind(event_id).bind(question_id).bind(response.sql()).bind(reason).bind(actor)
        .fetch_optional(&mut *tx).await?;
    let stored:Option<(i64,Uuid,String,String,Uuid)>=sqlx::query_as(
        "SELECT event_no,question_id,response,human_reason,actor_principal_id FROM niccolo.question_feedback_events WHERE workspace_id=$1 AND event_id=$2")
        .bind(workspace).bind(event_id).fetch_optional(&mut *tx).await?;
    let Some((event_no, stored_q, stored_response, stored_reason, stored_actor)) = stored else {
        return Err(FeedbackError::Conflict);
    };
    if (
        stored_q,
        stored_response.as_str(),
        stored_reason.as_str(),
        stored_actor,
    ) != (question_id, response.sql(), reason, actor)
    {
        return Err(FeedbackError::Conflict);
    }
    tx.commit().await?;
    Ok(FeedbackEvent {
        event_id,
        event_no,
        question_id,
        response: stored_response,
        human_reason: stored_reason,
        actor_principal_id: stored_actor,
        replayed: inserted.is_none(),
    })
}
pub async fn feedback_history(
    pool: &PgPool,
    workspace: Uuid,
    question_id: Uuid,
    after: i64,
    limit: i64,
) -> Result<(Vec<FeedbackEvent>, Option<i64>), FeedbackError> {
    if workspace.is_nil() || question_id.is_nil() || after < 0 || !(1..=100).contains(&limit) {
        return Err(FeedbackError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let found:Option<Uuid>=sqlx::query_scalar("SELECT question_id FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2")
        .bind(workspace).bind(question_id).fetch_optional(&mut *tx).await?;
    if found.is_none() {
        return Err(FeedbackError::Invisible);
    }
    let rows:Vec<(Uuid,i64,Uuid,String,String,Uuid)>=sqlx::query_as(
        "SELECT event_id,event_no,question_id,response,human_reason,actor_principal_id FROM niccolo.question_feedback_events WHERE workspace_id=$1 AND question_id=$2 AND event_no>$3 ORDER BY event_no LIMIT $4")
        .bind(workspace).bind(question_id).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let output: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(event_id, event_no, question_id, response, human_reason, actor_principal_id)| {
                FeedbackEvent {
                    event_id,
                    event_no,
                    question_id,
                    response,
                    human_reason,
                    actor_principal_id,
                    replayed: false,
                }
            },
        )
        .collect();
    let next = if more {
        output.last().map(|r| r.event_no)
    } else {
        None
    };
    Ok((output, next))
}

/// Select questions whose every *recorded latest per-actor position* is
/// REJECTED. A conflicting UNKNOWN or INSUFFICIENT_EVIDENCE position keeps
/// the question eligible for review; feedback is never treated as evidence.
pub async fn wholly_rejected_question_ids(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<std::collections::BTreeSet<Uuid>, FeedbackError> {
    if workspace.is_nil() || situation.is_nil() || analysis.is_nil() {
        return Err(FeedbackError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT latest.question_id
           FROM (
              SELECT DISTINCT ON (e.question_id,e.actor_principal_id)
                     e.question_id,e.actor_principal_id,e.response
                FROM niccolo.question_feedback_events e
                JOIN niccolo.situation_analysis_clarification_questions q
                  ON q.workspace_id=e.workspace_id AND q.question_id=e.question_id
               WHERE e.workspace_id=$1 AND q.situation_id=$2 AND q.analysis_id=$3
               ORDER BY e.question_id,e.actor_principal_id,e.event_no DESC
           ) latest
          GROUP BY latest.question_id
         HAVING bool_and(latest.response='REJECTED')",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ids.into_iter().collect())
}

/// Each actor has a separate, most-recent stated position. Contradictions are
/// preserved rather than collapsed into a consensus or truth estimate.
pub async fn positions(
    pool: &PgPool,
    workspace: Uuid,
    question_id: Uuid,
    after_actor: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<FeedbackEvent>, Option<Uuid>), FeedbackError> {
    if workspace.is_nil()
        || question_id.is_nil()
        || after_actor.is_some_and(|id| id.is_nil())
        || !(1..=100).contains(&limit)
    {
        return Err(FeedbackError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let found:Option<Uuid>=sqlx::query_scalar(
        "SELECT question_id FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2")
        .bind(workspace).bind(question_id).fetch_optional(&mut *tx).await?;
    if found.is_none() {
        return Err(FeedbackError::Invisible);
    }
    let rows:Vec<(Uuid,i64,Uuid,String,String,Uuid)>=sqlx::query_as(
        "SELECT DISTINCT ON (actor_principal_id) event_id,event_no,question_id,response,human_reason,actor_principal_id \
         FROM niccolo.question_feedback_events WHERE workspace_id=$1 AND question_id=$2 \
         AND ($3::uuid IS NULL OR actor_principal_id>$3) \
         ORDER BY actor_principal_id,event_no DESC LIMIT $4")
        .bind(workspace).bind(question_id).bind(after_actor).bind(limit+1)
        .fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let result: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(event_id, event_no, question_id, response, human_reason, actor_principal_id)| {
                FeedbackEvent {
                    event_id,
                    event_no,
                    question_id,
                    response,
                    human_reason,
                    actor_principal_id,
                    replayed: false,
                }
            },
        )
        .collect();
    let next = if more {
        result.last().map(|e| e.actor_principal_id)
    } else {
        None
    };
    Ok((result, next))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_types_never_collapse() {
        for (name, variant) in [
            ("rejected", HumanResponse::Rejected),
            ("insufficient_evidence", HumanResponse::InsufficientEvidence),
            ("unknown", HumanResponse::Unknown),
        ] {
            assert_eq!(HumanResponse::parse(name), Some(variant));
        }
        for invalid in ["", "unsure", "dismissed", "REJECTED", "not enough"] {
            assert_eq!(HumanResponse::parse(invalid), None);
        }
    }
    #[tokio::test]
    async fn nil_feedback_identifiers_fail_before_database_access() {
        let pool = PgPool::connect_lazy("postgres://127.0.0.1:1/never").unwrap();
        assert!(matches!(
            create_question(&pool, Uuid::nil(), Uuid::new_v4(), "Question?").await,
            Err(FeedbackError::Invalid)
        ));
        assert!(matches!(
            create_question(&pool, Uuid::new_v4(), Uuid::nil(), "Question?").await,
            Err(FeedbackError::Invalid)
        ));
        assert!(matches!(
            record_feedback(
                &pool,
                Uuid::new_v4(),
                Uuid::nil(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                HumanResponse::Unknown,
                "Unknown"
            )
            .await,
            Err(FeedbackError::Invalid)
        ));
        assert!(matches!(
            record_feedback(
                &pool,
                Uuid::new_v4(),
                Uuid::new_v4(),
                Uuid::nil(),
                Uuid::new_v4(),
                HumanResponse::Unknown,
                "Unknown"
            )
            .await,
            Err(FeedbackError::Invalid)
        ));
    }

    #[tokio::test]
    async fn invalid_feedback_read_scopes_fail_before_database_access() {
        let pool = PgPool::connect_lazy("postgres://127.0.0.1:1/never").unwrap();
        assert!(matches!(
            list_questions(&pool, Uuid::nil(), None, 10).await,
            Err(FeedbackError::Invalid)
        ));
        assert!(matches!(
            feedback_history(&pool, Uuid::new_v4(), Uuid::nil(), 0, 10).await,
            Err(FeedbackError::Invalid)
        ));
        assert!(matches!(
            wholly_rejected_question_ids(
                &pool,
                Uuid::new_v4(),
                Uuid::nil(),
                Uuid::new_v4()
            )
            .await,
            Err(FeedbackError::Invalid)
        ));
        assert!(matches!(
            positions(
                &pool,
                Uuid::new_v4(),
                Uuid::new_v4(),
                Some(Uuid::nil()),
                10
            )
            .await,
            Err(FeedbackError::Invalid)
        ));
    }

    #[test]
    fn invalid_text_fails_closed() {
        for s in ["", "  ", "hello\nInjected", "hello\0world"] {
            assert!(!valid(s, 100));
        }
        assert!(valid("Need a second original source?", 100));
    }
}
