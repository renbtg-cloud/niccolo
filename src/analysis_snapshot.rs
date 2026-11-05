//! Immutable binding between a requested analysis and the exact Situation revision it may use.
use crate::situation_store::{self, SituationSnapshot};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisSnapshotError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid analysis goal")]
    Invalid,
    #[error("analysis identifier replay conflicts with original request")]
    Conflict,
    #[error("situation or analysis snapshot is not visible")]
    Invisible,
    #[error("stored analysis snapshot does not match its referenced Situation revision")]
    Integrity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisInputSnapshot {
    pub analysis_id: Uuid,
    pub situation_id: Uuid,
    pub input_revision: i64,
    pub input_fingerprint: String,
    pub goal: String,
    pub requested_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedAnalysisInput {
    pub analysis: AnalysisInputSnapshot,
    pub input: SituationSnapshot,
    pub current_revision: i64,
    pub input_is_current: bool,
}

fn clean_goal(input: &str) -> Result<String, AnalysisSnapshotError> {
    let value = input.trim();
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(AnalysisSnapshotError::Invalid);
    }
    Ok(value.to_owned())
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AnalysisSnapshotError> {
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
        return Err(AnalysisSnapshotError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn begin(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    goal: &str,
) -> Result<AnalysisInputSnapshot, AnalysisSnapshotError> {
    let goal = clean_goal(goal)?;
    let mut tx = scoped(pool, workspace, false).await?;

    // Idempotent retry must return the original frozen input even if the
    // Situation has advanced since the first request.
    let stored: Option<(i64, String, String, Uuid)> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint,goal,requested_by_principal_id \
         FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((input_revision, input_fingerprint, stored_goal, requested_by)) = stored {
        if stored_goal != goal || requested_by != actor {
            return Err(AnalysisSnapshotError::Conflict);
        }
        tx.commit().await?;
        return Ok(AnalysisInputSnapshot {
            analysis_id: analysis,
            situation_id: situation,
            input_revision,
            input_fingerprint,
            goal: stored_goal,
            requested_by,
            replayed: true,
        });
    }

    // SHARE prevents current_revision from changing while the exact revision
    // and fingerprint are copied into the immutable analysis snapshot.
    let current: Option<(i64,)> = sqlx::query_as(
        "SELECT current_revision FROM niccolo.situations \
         WHERE workspace_id=$1 AND situation_id=$2 FOR SHARE",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input_revision,)) = current else {
        return Err(AnalysisSnapshotError::Invisible);
    };
    let input_fingerprint: Option<String> = sqlx::query_scalar(
        "SELECT fingerprint FROM niccolo.situation_revisions \
         WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(input_revision)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(input_fingerprint) = input_fingerprint else {
        return Err(AnalysisSnapshotError::Invisible);
    };
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_snapshots \
         (workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace_id,situation_id,analysis_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(input_revision)
    .bind(&input_fingerprint)
    .bind(&goal)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        // A concurrent identical request won the race. Re-read and apply the
        // same immutable replay contract instead of binding to a newer revision.
        let stored: Option<(i64, String, String, Uuid)> = sqlx::query_as(
            "SELECT input_revision,input_fingerprint,goal,requested_by_principal_id \
             FROM niccolo.situation_analysis_snapshots \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((revision, fingerprint, stored_goal, requested_by)) = stored else {
            return Err(AnalysisSnapshotError::Conflict);
        };
        if stored_goal != goal || requested_by != actor {
            return Err(AnalysisSnapshotError::Conflict);
        }
        tx.commit().await?;
        return Ok(AnalysisInputSnapshot {
            analysis_id: analysis,
            situation_id: situation,
            input_revision: revision,
            input_fingerprint: fingerprint,
            goal: stored_goal,
            requested_by,
            replayed: true,
        });
    }
    tx.commit().await?;
    Ok(AnalysisInputSnapshot {
        analysis_id: analysis,
        situation_id: situation,
        input_revision,
        input_fingerprint,
        goal,
        requested_by: actor,
        replayed: false,
    })
}

pub async fn get(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<AnalysisInputSnapshot, AnalysisSnapshotError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let stored: Option<(i64, String, String, Uuid)> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint,goal,requested_by_principal_id \
         FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input_revision, input_fingerprint, goal, requested_by)) = stored else {
        return Err(AnalysisSnapshotError::Invisible);
    };
    tx.commit().await?;
    Ok(AnalysisInputSnapshot {
        analysis_id: analysis,
        situation_id: situation,
        input_revision,
        input_fingerprint,
        goal,
        requested_by,
        replayed: false,
    })
}

pub async fn materialize(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<MaterializedAnalysisInput, AnalysisSnapshotError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let stored: Option<(i64, String, String, Uuid)> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint,goal,requested_by_principal_id \
         FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input_revision, input_fingerprint, goal, requested_by)) = stored else {
        return Err(AnalysisSnapshotError::Invisible);
    };
    let current_revision: Option<i64> = sqlx::query_scalar(
        "SELECT current_revision FROM niccolo.situations \
         WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current_revision) = current_revision else {
        return Err(AnalysisSnapshotError::Invisible);
    };
    let input = situation_store::snapshot_in_tx(&mut tx, workspace, situation, input_revision)
        .await
        .map_err(|error| match error {
            situation_store::SituationStoreError::Database(error) => {
                AnalysisSnapshotError::Database(error)
            }
            situation_store::SituationStoreError::Invisible => AnalysisSnapshotError::Invisible,
            _ => AnalysisSnapshotError::Integrity,
        })?;
    if input.fingerprint != input_fingerprint {
        return Err(AnalysisSnapshotError::Integrity);
    }
    tx.commit().await?;
    Ok(MaterializedAnalysisInput {
        analysis: AnalysisInputSnapshot {
            analysis_id: analysis,
            situation_id: situation,
            input_revision,
            input_fingerprint,
            goal,
            requested_by,
            replayed: false,
        },
        input,
        current_revision,
        input_is_current: current_revision == input_revision,
    })
}
