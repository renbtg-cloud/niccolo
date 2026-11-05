//! Read model separating human context present in a frozen input from context
//! explicitly recorded as used by that analysis.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisContextSummaryError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid analysis context summary request")]
    Invalid,
    #[error("analysis snapshot is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextContributionSummary {
    pub contribution_id: Uuid,
    pub author: Uuid,
    pub kind: String,
    pub text: String,
    pub scope: String,
    pub related_evidence_id: Option<Uuid>,
    pub input_revision: i64,
    pub used_by_analysis: bool,
    pub use_recorded_by: Option<Uuid>,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, AnalysisContextSummaryError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(AnalysisContextSummaryError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<ContextContributionSummary>, Option<Uuid>), AnalysisContextSummaryError> {
    if !(1..=100).contains(&limit) {
        return Err(AnalysisContextSummaryError::Invalid);
    }
    let mut tx = scoped(pool, workspace).await?;
    let input_revision: Option<i64> = sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(input_revision) = input_revision else {
        return Err(AnalysisContextSummaryError::Invisible);
    };

    type Row = (
        Uuid,
        Uuid,
        String,
        String,
        String,
        Option<Uuid>,
        Option<Uuid>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT c.contribution_id,c.author_principal_id,c.kind,c.contribution_text,c.scope,\
                c.related_evidence_id,u.recorded_by_principal_id \
         FROM niccolo.situation_revision_contributions rc \
         JOIN niccolo.situation_contributions c \
           ON c.workspace_id=rc.workspace_id AND c.situation_id=rc.situation_id \
          AND c.contribution_id=rc.contribution_id \
         LEFT JOIN niccolo.situation_analysis_used_contributions u \
           ON u.workspace_id=rc.workspace_id AND u.situation_id=rc.situation_id \
          AND u.analysis_id=$3 AND u.input_revision=rc.revision \
          AND u.contribution_id=rc.contribution_id \
         WHERE rc.workspace_id=$1 AND rc.situation_id=$2 AND rc.revision=$4 \
           AND ($5::uuid IS NULL OR c.contribution_id>$5) \
         ORDER BY c.contribution_id LIMIT $6",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(input_revision)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    let more = rows.len() as i64 > limit;
    let items = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(contribution_id, author, kind, text, scope, related_evidence_id, use_recorded_by)| {
                ContextContributionSummary {
                    contribution_id,
                    author,
                    kind,
                    text,
                    scope,
                    related_evidence_id,
                    input_revision,
                    used_by_analysis: use_recorded_by.is_some(),
                    use_recorded_by,
                }
            },
        )
        .collect::<Vec<_>>();
    let next_after = more.then(|| items.last().expect("non-empty page").contribution_id);
    tx.commit().await?;
    Ok((items, next_after))
}
