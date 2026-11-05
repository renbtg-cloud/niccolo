//! Explicit human-context usage for a frozen analysis input.
//!
//! Presence in the frozen Situation does not imply use by the analysis. Marking
//! a contribution as used does not make it true, verified, or evidence.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisContextUsageError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid analysis context usage request")]
    Invalid,
    #[error("analysis context usage conflict")]
    Conflict,
    #[error("analysis or frozen contribution is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsedContribution {
    pub contribution_id: Uuid,
    pub author: Uuid,
    pub kind: String,
    pub text: String,
    pub scope: String,
    pub related_evidence_id: Option<Uuid>,
    pub input_revision: i64,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AnalysisContextUsageError> {
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
        return Err(AnalysisContextUsageError::Privileged);
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
    reason = "frozen analysis and contribution identities are independent"
)]
pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    contribution: Uuid,
) -> Result<UsedContribution, AnalysisContextUsageError> {
    let mut tx = scoped(pool, workspace, false).await?;
    let input_revision:Option<i64>=sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3"
    ).bind(workspace).bind(situation).bind(analysis).fetch_optional(&mut *tx).await?;
    let Some(input_revision) = input_revision else {
        return Err(AnalysisContextUsageError::Invisible);
    };

    let inserted=sqlx::query(
        "INSERT INTO niccolo.situation_analysis_used_contributions \
         (workspace_id,situation_id,analysis_id,input_revision,contribution_id,recorded_by_principal_id) \
         SELECT $1,$2,$3,$4,$5,$6 FROM niccolo.situation_revision_contributions rc \
         WHERE rc.workspace_id=$1 AND rc.situation_id=$2 AND rc.revision=$4 AND rc.contribution_id=$5 \
         ON CONFLICT(workspace_id,situation_id,analysis_id,contribution_id) DO NOTHING"
    ).bind(workspace).bind(situation).bind(analysis).bind(input_revision).bind(contribution).bind(actor)
     .execute(&mut *tx).await?.rows_affected()==1;

    let row:Option<(Uuid,String,String,String,Option<Uuid>,Uuid)>=sqlx::query_as(
        "SELECT c.author_principal_id,c.kind,c.contribution_text,c.scope,c.related_evidence_id,u.recorded_by_principal_id \
         FROM niccolo.situation_analysis_used_contributions u \
         JOIN niccolo.situation_contributions c \
           ON c.workspace_id=u.workspace_id AND c.situation_id=u.situation_id AND c.contribution_id=u.contribution_id \
         WHERE u.workspace_id=$1 AND u.situation_id=$2 AND u.analysis_id=$3 AND u.contribution_id=$4"
    ).bind(workspace).bind(situation).bind(analysis).bind(contribution).fetch_optional(&mut *tx).await?;
    let Some((author, kind, text, scope, related_evidence_id, recorded_by)) = row else {
        return Err(AnalysisContextUsageError::Invisible);
    };
    if !inserted && recorded_by != actor {
        return Err(AnalysisContextUsageError::Conflict);
    }
    tx.commit().await?;
    Ok(UsedContribution {
        contribution_id: contribution,
        author,
        kind,
        text,
        scope,
        related_evidence_id,
        input_revision,
        recorded_by,
        replayed: !inserted,
    })
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<UsedContribution>, Option<Uuid>), AnalysisContextUsageError> {
    if !(1..=100).contains(&limit) {
        return Err(AnalysisContextUsageError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let input_revision:Option<i64>=sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3"
    ).bind(workspace).bind(situation).bind(analysis).fetch_optional(&mut *tx).await?;
    let Some(input_revision) = input_revision else {
        return Err(AnalysisContextUsageError::Invisible);
    };
    let rows:Vec<(Uuid,Uuid,String,String,String,Option<Uuid>,Uuid)>=sqlx::query_as(
        "SELECT u.contribution_id,c.author_principal_id,c.kind,c.contribution_text,c.scope,c.related_evidence_id,u.recorded_by_principal_id \
         FROM niccolo.situation_analysis_used_contributions u \
         JOIN niccolo.situation_contributions c \
           ON c.workspace_id=u.workspace_id AND c.situation_id=u.situation_id AND c.contribution_id=u.contribution_id \
         WHERE u.workspace_id=$1 AND u.situation_id=$2 AND u.analysis_id=$3 \
           AND ($4::uuid IS NULL OR u.contribution_id>$4) ORDER BY u.contribution_id LIMIT $5"
    ).bind(workspace).bind(situation).bind(analysis).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    let more = rows.len() as i64 > limit;
    let used = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(contribution_id, author, kind, text, scope, related_evidence_id, recorded_by)| {
                UsedContribution {
                    contribution_id,
                    author,
                    kind,
                    text,
                    scope,
                    related_evidence_id,
                    input_revision,
                    recorded_by,
                    replayed: false,
                }
            },
        )
        .collect::<Vec<_>>();
    let next_after = more.then(|| used.last().expect("non-empty page").contribution_id);
    tx.commit().await?;
    Ok((used, next_after))
}
