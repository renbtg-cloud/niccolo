//! Deterministic difference between an analysis' frozen Situation input and the
//! Situation as it exists now. A delta is a change log, not a truth judgement.
use crate::situation_store;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisDeltaError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("analysis snapshot is not visible")]
    Invisible,
    #[error("stored Situation state is inconsistent")]
    Integrity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedContribution {
    pub contribution_id: Uuid,
    pub kind: String,
    pub text: String,
    pub current_status: String,
    pub corrected_by: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisDelta {
    pub analysis_id: Uuid,
    pub input_revision: i64,
    pub current_revision: i64,
    pub input_fingerprint: String,
    pub current_fingerprint: String,
    pub input_is_current: bool,
    pub focus_before: String,
    pub focus_now: String,
    pub added_sources: Vec<Uuid>,
    pub removed_sources: Vec<Uuid>,
    pub added_contributions: Vec<situation_store::SnapshotContribution>,
    pub removed_contributions: Vec<RemovedContribution>,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, AnalysisDeltaError> {
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
        return Err(AnalysisDeltaError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn source_delta(before: &[Uuid], now: &[Uuid]) -> (Vec<Uuid>, Vec<Uuid>) {
    let before: BTreeSet<_> = before.iter().copied().collect();
    let now: BTreeSet<_> = now.iter().copied().collect();
    (
        now.difference(&before).copied().collect(),
        before.difference(&now).copied().collect(),
    )
}

pub async fn get(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<AnalysisDelta, AnalysisDeltaError> {
    let mut tx = scoped(pool, workspace).await?;
    let frozen: Option<(i64, String)> = sqlx::query_as(
        "SELECT input_revision,input_fingerprint FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((input_revision, input_fingerprint)) = frozen else {
        return Err(AnalysisDeltaError::Invisible);
    };
    let before = situation_store::snapshot_in_tx(&mut tx, workspace, situation, input_revision)
        .await
        .map_err(|e| match e {
            situation_store::SituationStoreError::Database(e) => AnalysisDeltaError::Database(e),
            situation_store::SituationStoreError::Invisible => AnalysisDeltaError::Invisible,
            _ => AnalysisDeltaError::Integrity,
        })?;
    if before.fingerprint != input_fingerprint {
        return Err(AnalysisDeltaError::Integrity);
    }
    let now = situation_store::current_snapshot_in_tx(&mut tx, workspace, situation)
        .await
        .map_err(|e| match e {
            situation_store::SituationStoreError::Database(e) => AnalysisDeltaError::Database(e),
            situation_store::SituationStoreError::Invisible => AnalysisDeltaError::Invisible,
            _ => AnalysisDeltaError::Integrity,
        })?;
    let (added_sources, removed_sources) = source_delta(&before.sources, &now.sources);
    let before_map: BTreeMap<_, _> = before
        .contributions
        .iter()
        .cloned()
        .map(|c| (c.id, c))
        .collect();
    let now_map: BTreeMap<_, _> = now
        .contributions
        .iter()
        .cloned()
        .map(|c| (c.id, c))
        .collect();
    let added_contributions = now_map
        .iter()
        .filter(|(id, _)| !before_map.contains_key(id))
        .map(|(_, c)| c.clone())
        .collect::<Vec<_>>();
    let removed_ids = before_map
        .keys()
        .filter(|id| !now_map.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    let mut removed_contributions = Vec::with_capacity(removed_ids.len());
    for id in removed_ids {
        let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
            "SELECT status,corrected_by FROM niccolo.situation_contributions \
             WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((current_status, corrected_by)) = row else {
            return Err(AnalysisDeltaError::Integrity);
        };
        let old = before_map
            .get(&id)
            .expect("removed id came from frozen snapshot");
        removed_contributions.push(RemovedContribution {
            contribution_id: id,
            kind: old.kind.clone(),
            text: old.text.clone(),
            current_status,
            corrected_by,
        });
    }
    tx.commit().await?;
    Ok(AnalysisDelta {
        analysis_id: analysis,
        input_revision,
        current_revision: now.revision,
        input_fingerprint,
        current_fingerprint: now.fingerprint.clone(),
        input_is_current: input_revision == now.revision,
        focus_before: before.focus,
        focus_now: now.focus,
        added_sources,
        removed_sources,
        added_contributions,
        removed_contributions,
    })
}
