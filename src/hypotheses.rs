//! Durable manual hypotheses about *two* attributable evidence records.
//! Kind APPARENT_CONTRADICTION is a human label, never a derived factual judgment.
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum HypothesisError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid human hypothesis or page limit")]
    Invalid,
    #[error("hypothesis ID replay conflicts with immutable original")]
    Conflict,
    #[error("workspace or evidence not visible")]
    Invisible,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualHypothesis {
    pub id: Uuid,
    pub kind: String,
    pub left: Uuid,
    pub right: Uuid,
    pub human_note: String,
    pub actor: Uuid,
    pub replayed: bool,
}
fn kind(input: &str) -> Option<&'static str> {
    match input {
        "apparent_contradiction" => Some("APPARENT_CONTRADICTION"),
        "attribution_question" => Some("ATTRIBUTION_QUESTION"),
        "timing_question" => Some("TIMING_QUESTION"),
        _ => None,
    }
}
async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, HypothesisError> {
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
        return Err(HypothesisError::Privileged);
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
    reason = "every immutable evidence and human-attribution input is explicit"
)]
pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    id: Uuid,
    kind_input: &str,
    left: Uuid,
    right: Uuid,
    note: &str,
) -> Result<ManualHypothesis, HypothesisError> {
    let Some(canonical) = kind(kind_input) else {
        return Err(HypothesisError::Invalid);
    };
    if left == right
        || note.trim().is_empty()
        || note.len() > 2048
        || note.chars().any(char::is_control)
    {
        return Err(HypothesisError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    // Check visible workspace and BOTH pieces of evidence before attempting INSERT.
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.evidence WHERE workspace_id=$1 AND evidence_id IN ($2,$3)",
    )
    .bind(workspace)
    .bind(left)
    .bind(right)
    .fetch_one(&mut *tx)
    .await?;
    if visible != 2 {
        return Err(HypothesisError::Invisible);
    };
    let inserted=sqlx::query("INSERT INTO niccolo.manual_hypotheses(workspace_id,hypothesis_id,kind,left_evidence_id,right_evidence_id,human_note,actor_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace_id,hypothesis_id) DO NOTHING")
        .bind(workspace).bind(id).bind(canonical).bind(left).bind(right).bind(note).bind(actor)
        .execute(&mut *tx).await?.rows_affected()==1;
    let stored:Option<(String,Uuid,Uuid,String,Uuid)>=sqlx::query_as("SELECT kind,left_evidence_id,right_evidence_id,human_note,actor_principal_id FROM niccolo.manual_hypotheses WHERE workspace_id=$1 AND hypothesis_id=$2")
        .bind(workspace).bind(id).fetch_optional(&mut *tx).await?;
    let Some((stored_kind, l, r, stored_note, stored_actor)) = stored else {
        return Err(HypothesisError::Conflict);
    };
    if (
        stored_kind.as_str(),
        l,
        r,
        stored_note.as_str(),
        stored_actor,
    ) != (canonical, left, right, note, actor)
    {
        return Err(HypothesisError::Conflict);
    }
    tx.commit().await?;
    Ok(ManualHypothesis {
        id,
        kind: stored_kind,
        left,
        right,
        human_note: stored_note,
        actor,
        replayed: !inserted,
    })
}
pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<ManualHypothesis>, Option<Uuid>), HypothesisError> {
    if !(1..=100).contains(&limit) {
        return Err(HypothesisError::Invalid);
    };
    let mut tx = scoped(pool, workspace, true).await?;
    let records:Vec<(Uuid,String,Uuid,Uuid,String,Uuid)>=sqlx::query_as("SELECT hypothesis_id,kind,left_evidence_id,right_evidence_id,human_note,actor_principal_id FROM niccolo.manual_hypotheses WHERE workspace_id=$1 AND ($2::uuid IS NULL OR hypothesis_id>$2) ORDER BY hypothesis_id LIMIT $3")
        .bind(workspace).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = records.len() as i64 > limit;
    let hypotheses = records
        .into_iter()
        .take(limit as usize)
        .map(
            |(id, kind, left, right, human_note, actor)| ManualHypothesis {
                id,
                kind,
                left,
                right,
                human_note,
                actor,
                replayed: false,
            },
        )
        .collect::<Vec<_>>();
    let next = if more {
        hypotheses.last().map(|x| x.id)
    } else {
        None
    };
    Ok((hypotheses, next))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_kind_is_just_a_label_not_a_factual_inference() {
        assert_eq!(
            kind("apparent_contradiction"),
            Some("APPARENT_CONTRADICTION")
        );
        assert_eq!(kind("attribution_question"), Some("ATTRIBUTION_QUESTION"));
        assert_eq!(kind("timing_question"), Some("TIMING_QUESTION"));
        for invalid in [
            "contradiction",
            "proven",
            "false",
            "APPARENT_CONTRADICTION",
            "",
        ] {
            assert!(kind(invalid).is_none());
        }
    }
}
