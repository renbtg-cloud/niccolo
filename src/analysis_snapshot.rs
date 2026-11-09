//! Immutable binding between a requested analysis and the exact Situation revision it may use.
use crate::{
    purpose_history,
    situation_store::{self, SituationSnapshot},
};
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
    pub identity_event_no_max: i64,
    pub actor_kind_event_no_max: i64,
    pub actor_binding_event_no_max: i64,
    pub purpose_event_no_max: i64,
    pub declared_purpose: Option<String>,
    pub goal: String,
    pub requested_by: Uuid,
    pub reanalysis_of: Option<Uuid>,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedAnalysisInput {
    pub analysis: AnalysisInputSnapshot,
    pub input: SituationSnapshot,
    pub current_revision: i64,
    pub input_is_current: bool,
}

type StoredAnalysisSnapshotRow = (
    i64,
    String,
    i64,
    i64,
    i64,
    i64,
    Option<String>,
    String,
    Uuid,
    Option<Uuid>,
);

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
    reanalysis_of: Option<Uuid>,
) -> Result<AnalysisInputSnapshot, AnalysisSnapshotError> {
    let goal = clean_goal(goal)?;
    let mut tx = scoped(pool, workspace, false).await?;

    // Idempotent retry must return the original frozen input even if the
    // Situation has advanced since the first request.
    if reanalysis_of == Some(analysis) {
        return Err(AnalysisSnapshotError::Invalid);
    }
    let stored: Option<StoredAnalysisSnapshotRow> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint,identity_event_no_max,actor_kind_event_no_max,actor_binding_event_no_max,purpose_event_no_max,declared_purpose,goal,requested_by_principal_id,reanalysis_of \
         FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        actor_kind_event_no_max,
        actor_binding_event_no_max,
        purpose_event_no_max,
        declared_purpose,
        stored_goal,
        requested_by,
        stored_reanalysis_of,
    )) = stored
    {
        if stored_goal != goal || requested_by != actor || stored_reanalysis_of != reanalysis_of {
            return Err(AnalysisSnapshotError::Conflict);
        }
        tx.commit().await?;
        return Ok(AnalysisInputSnapshot {
            analysis_id: analysis,
            situation_id: situation,
            input_revision,
            input_fingerprint,
            identity_event_no_max,
            actor_kind_event_no_max,
            actor_binding_event_no_max,
            purpose_event_no_max,
            declared_purpose,
            goal: stored_goal,
            requested_by,
            reanalysis_of: stored_reanalysis_of,
            replayed: true,
        });
    }

    if let Some(parent) = reanalysis_of {
        let parent_visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(parent)
        .fetch_one(&mut *tx)
        .await?;
        if !parent_visible {
            return Err(AnalysisSnapshotError::Invisible);
        }
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
    let identity_event_no_max: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(event_no),0)::bigint FROM niccolo.identity_events WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;
    let actor_kind_event_no_max: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(event_no),0)::bigint FROM niccolo.actor_kind_events WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;
    let actor_binding_event_no_max: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(event_no),0)::bigint FROM niccolo.actor_binding_events WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;
    let (purpose_event_no_max, declared_purpose) =
        purpose_history::latest_in_tx(&mut tx, workspace, situation)
            .await
            .map_err(|error| match error {
                purpose_history::PurposeError::Database(error) => {
                    AnalysisSnapshotError::Database(error)
                }
                _ => AnalysisSnapshotError::Integrity,
            })?
            .map_or((0, None), |(event_no, purpose)| (event_no, Some(purpose)));
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_snapshots \
         (workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,identity_event_no_max,actor_kind_event_no_max,actor_binding_event_no_max,purpose_event_no_max,declared_purpose,goal,requested_by_principal_id,reanalysis_of) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(workspace_id,situation_id,analysis_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(input_revision)
    .bind(&input_fingerprint)
    .bind(identity_event_no_max)
    .bind(actor_kind_event_no_max)
    .bind(actor_binding_event_no_max)
    .bind(purpose_event_no_max)
    .bind(&declared_purpose)
    .bind(&goal)
    .bind(actor)
    .bind(reanalysis_of)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        // A concurrent identical request won the race. Re-read and apply the
        // same immutable replay contract instead of binding to a newer revision.
        let stored: Option<StoredAnalysisSnapshotRow> = sqlx::query_as(
            "SELECT input_revision,input_fingerprint,identity_event_no_max,actor_kind_event_no_max,actor_binding_event_no_max,purpose_event_no_max,declared_purpose,goal,requested_by_principal_id,reanalysis_of \
             FROM niccolo.situation_analysis_snapshots \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((
            revision,
            fingerprint,
            stored_identity_event_no_max,
            stored_actor_kind_event_no_max,
            stored_actor_binding_event_no_max,
            stored_purpose_event_no_max,
            stored_declared_purpose,
            stored_goal,
            requested_by,
            stored_reanalysis_of,
        )) = stored
        else {
            return Err(AnalysisSnapshotError::Conflict);
        };
        if stored_goal != goal || requested_by != actor || stored_reanalysis_of != reanalysis_of {
            return Err(AnalysisSnapshotError::Conflict);
        }
        tx.commit().await?;
        return Ok(AnalysisInputSnapshot {
            analysis_id: analysis,
            situation_id: situation,
            input_revision: revision,
            input_fingerprint: fingerprint,
            identity_event_no_max: stored_identity_event_no_max,
            actor_kind_event_no_max: stored_actor_kind_event_no_max,
            actor_binding_event_no_max: stored_actor_binding_event_no_max,
            purpose_event_no_max: stored_purpose_event_no_max,
            declared_purpose: stored_declared_purpose,
            goal: stored_goal,
            requested_by,
            reanalysis_of: stored_reanalysis_of,
            replayed: true,
        });
    }
    tx.commit().await?;
    Ok(AnalysisInputSnapshot {
        analysis_id: analysis,
        situation_id: situation,
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        actor_kind_event_no_max,
        actor_binding_event_no_max,
        purpose_event_no_max,
        declared_purpose,
        goal,
        requested_by: actor,
        reanalysis_of,
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
    let stored: Option<StoredAnalysisSnapshotRow> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint,identity_event_no_max,actor_kind_event_no_max,actor_binding_event_no_max,purpose_event_no_max,declared_purpose,goal,requested_by_principal_id,reanalysis_of \
         FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        actor_kind_event_no_max,
        actor_binding_event_no_max,
        purpose_event_no_max,
        declared_purpose,
        goal,
        requested_by,
        reanalysis_of,
    )) = stored
    else {
        return Err(AnalysisSnapshotError::Invisible);
    };
    tx.commit().await?;
    Ok(AnalysisInputSnapshot {
        analysis_id: analysis,
        situation_id: situation,
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        actor_kind_event_no_max,
        actor_binding_event_no_max,
        purpose_event_no_max,
        declared_purpose,
        goal,
        requested_by,
        reanalysis_of,
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
    let stored: Option<StoredAnalysisSnapshotRow> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint,identity_event_no_max,actor_kind_event_no_max,actor_binding_event_no_max,purpose_event_no_max,declared_purpose,goal,requested_by_principal_id,reanalysis_of \
         FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        actor_kind_event_no_max,
        actor_binding_event_no_max,
        purpose_event_no_max,
        declared_purpose,
        goal,
        requested_by,
        reanalysis_of,
    )) = stored
    else {
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
            identity_event_no_max,
            actor_kind_event_no_max,
            actor_binding_event_no_max,
            purpose_event_no_max,
            declared_purpose,
            goal,
            requested_by,
            reanalysis_of,
            replayed: false,
        },
        input,
        current_revision,
        input_is_current: current_revision == input_revision,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisHistoryEntry {
    pub analysis_id: Uuid,
    pub input_revision: i64,
    pub input_fingerprint: String,
    pub goal: String,
    pub requested_by: Uuid,
    pub reanalysis_of: Option<Uuid>,
    pub answer_text: Option<String>,
    pub coverage_note: Option<String>,
    pub published_by: Option<Uuid>,
    pub current_revision: i64,
    pub input_is_current: bool,
}

type HistoryRow = (
    Uuid,
    i64,
    String,
    String,
    Uuid,
    Option<Uuid>,
    Option<String>,
    Option<String>,
    Option<Uuid>,
    i64,
);

pub async fn history(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    before: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<AnalysisHistoryEntry>, Option<Uuid>), AnalysisSnapshotError> {
    if !(1..=100).contains(&limit) {
        return Err(AnalysisSnapshotError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let situation_visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    if !situation_visible {
        return Err(AnalysisSnapshotError::Invisible);
    }

    if let Some(cursor) = before {
        let cursor_visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(cursor)
        .fetch_one(&mut *tx)
        .await?;
        if !cursor_visible {
            return Err(AnalysisSnapshotError::Invisible);
        }
    }

    let rows: Vec<HistoryRow> = sqlx::query_as(
        "SELECT s.analysis_id,s.input_revision,s.input_fingerprint,s.goal,\
                s.requested_by_principal_id,s.reanalysis_of,\
                o.answer_text,o.coverage_note,o.published_by_principal_id,z.current_revision \
         FROM niccolo.situation_analysis_snapshots s \
         JOIN niccolo.situations z USING(workspace_id,situation_id) \
         LEFT JOIN niccolo.situation_analysis_outputs o USING(workspace_id,situation_id,analysis_id) \
         WHERE s.workspace_id=$1 AND s.situation_id=$2 \
           AND ($3::uuid IS NULL OR (s.created_at,s.analysis_id) < (\
             SELECT c.created_at,c.analysis_id FROM niccolo.situation_analysis_snapshots c \
             WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3\
           )) \
         ORDER BY s.created_at DESC,s.analysis_id DESC \
         LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(before)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;

    let has_more = rows.len() as i64 > limit;
    let mut entries = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(
                analysis_id,
                input_revision,
                input_fingerprint,
                goal,
                requested_by,
                reanalysis_of,
                answer_text,
                coverage_note,
                published_by,
                current_revision,
            )| AnalysisHistoryEntry {
                analysis_id,
                input_revision,
                input_fingerprint,
                goal,
                requested_by,
                reanalysis_of,
                answer_text,
                coverage_note,
                published_by,
                current_revision,
                input_is_current: current_revision == input_revision,
            },
        )
        .collect::<Vec<_>>();
    let next_before = has_more.then(|| entries.last().expect("non-empty page").analysis_id);
    tx.commit().await?;
    Ok((std::mem::take(&mut entries), next_before))
}
