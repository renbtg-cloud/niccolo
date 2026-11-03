//! Explicit human pointers from questions to existing evidence, never synthetic evidence.
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReferenceError {
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("privileged DB connection forbidden")]
    Privileged,
    #[error("invalid reference content or limit")]
    Invalid,
    #[error("identifier replay conflicts with stored reference")]
    Conflict,
    #[error("question or evidence not visible")]
    Invisible,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceReference {
    pub reference_id: Uuid,
    pub question_id: Uuid,
    pub evidence_id: Uuid,
    pub relation: String,
    pub human_reason: String,
    pub actor_principal_id: Uuid,
    pub replayed: bool,
}
fn relation_allowed(input: &str) -> bool {
    matches!(input, "references" | "challenges" | "follow_up_on")
}
fn normalized(input: &str) -> &'static str {
    match input {
        "references" => "REFERENCES",
        "challenges" => "CHALLENGES",
        "follow_up_on" => "FOLLOW_UP_ON",
        _ => unreachable!(),
    }
}
async fn scope(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, ReferenceError> {
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
        return Err(ReferenceError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
/// Idempotent event with immutable attribution. No inference of factual support.
#[expect(
    clippy::too_many_arguments,
    reason = "explicit provenance and audit fields are independently validated"
)]
pub async fn attach(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    reference_id: Uuid,
    question_id: Uuid,
    evidence_id: Uuid,
    relation: &str,
    reason: &str,
) -> Result<EvidenceReference, ReferenceError> {
    if !relation_allowed(relation)
        || reason.trim().is_empty()
        || reason.len() > 2048
        || reason.chars().any(char::is_control)
    {
        return Err(ReferenceError::Invalid);
    }
    let mut tx = scope(pool, workspace, false).await?;
    let visible: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1 FOR UPDATE",
    )
    .bind(workspace)
    .fetch_optional(&mut *tx)
    .await?;
    if visible.is_none() {
        return Err(ReferenceError::Invisible);
    }
    // Stable, generic 404 for cross-workspace and unknown IDs.
    let found_q:Option<Uuid>=sqlx::query_scalar("SELECT question_id FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2")
        .bind(workspace).bind(question_id).fetch_optional(&mut *tx).await?;
    let found_e: Option<Uuid> = sqlx::query_scalar(
        "SELECT evidence_id FROM niccolo.evidence WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(workspace)
    .bind(evidence_id)
    .fetch_optional(&mut *tx)
    .await?;
    if found_q.is_none() || found_e.is_none() {
        return Err(ReferenceError::Invisible);
    }
    let inserted=sqlx::query("INSERT INTO niccolo.question_evidence_references(workspace_id,reference_id,question_id,evidence_id,relation,human_reason,actor_principal_id) \
        VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace_id,reference_id) DO NOTHING")
        .bind(workspace).bind(reference_id).bind(question_id).bind(evidence_id).bind(normalized(relation))
        .bind(reason).bind(actor).execute(&mut *tx).await?.rows_affected()==1;
    let row:Option<(Uuid,Uuid,String,String,Uuid)>=sqlx::query_as(
        "SELECT question_id,evidence_id,relation,human_reason,actor_principal_id FROM niccolo.question_evidence_references WHERE workspace_id=$1 AND reference_id=$2")
        .bind(workspace).bind(reference_id).fetch_optional(&mut *tx).await?;
    let Some((stored_q, stored_e, stored_relation, stored_reason, stored_actor)) = row else {
        return Err(ReferenceError::Conflict);
    };
    if (
        stored_q,
        stored_e,
        stored_relation.as_str(),
        stored_reason.as_str(),
        stored_actor,
    ) != (
        question_id,
        evidence_id,
        normalized(relation),
        reason,
        actor,
    ) {
        return Err(ReferenceError::Conflict);
    }
    tx.commit().await?;
    Ok(EvidenceReference {
        reference_id,
        question_id,
        evidence_id,
        relation: stored_relation,
        human_reason: stored_reason,
        actor_principal_id: stored_actor,
        replayed: !inserted,
    })
}
pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    question_id: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<EvidenceReference>, Option<Uuid>), ReferenceError> {
    if !(1..=100).contains(&limit) {
        return Err(ReferenceError::Invalid);
    }
    let mut tx = scope(pool, workspace, true).await?;
    let found:Option<Uuid>=sqlx::query_scalar("SELECT question_id FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2")
        .bind(workspace).bind(question_id).fetch_optional(&mut *tx).await?;
    if found.is_none() {
        return Err(ReferenceError::Invisible);
    }
    let rows: Vec<(Uuid, Uuid, Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT reference_id,question_id,evidence_id,relation,human_reason,actor_principal_id \
        FROM niccolo.question_evidence_references WHERE workspace_id=$1 AND question_id=$2 \
        AND ($3::uuid IS NULL OR reference_id>$3) ORDER BY reference_id LIMIT $4",
    )
    .bind(workspace)
    .bind(question_id)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let result: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(
                reference_id,
                question_id,
                evidence_id,
                relation,
                human_reason,
                actor_principal_id,
            )| EvidenceReference {
                reference_id,
                question_id,
                evidence_id,
                relation,
                human_reason,
                actor_principal_id,
                replayed: false,
            },
        )
        .collect();
    let next = if more {
        result.last().map(|r| r.reference_id)
    } else {
        None
    };
    Ok((result, next))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_three_explicit_relations() {
        for valid in ["references", "challenges", "follow_up_on"] {
            assert!(relation_allowed(valid))
        }
        for invalid in ["", "supports", "proves", "fact", "FOLLOW_UP_ON"] {
            assert!(!relation_allowed(invalid))
        }
    }
}
