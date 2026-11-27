//! An attributed, within-workspace handoff of already recorded material.
use super::{ArtifactError, checkpoint, scoped, valid_text};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    source_situation_id: Uuid,
    source_representation_id: Uuid,
    note: String,
}

#[derive(Serialize)]
pub struct Transfer {
    transfer_id: Uuid,
    source_situation_id: Uuid,
    target_situation_id: Uuid,
    representation_id: Uuid,
    artifact_id: Uuid,
    note: String,
    recorded_by: Uuid,
    recorded_at: String,
}

async fn find(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    target: Uuid,
    transfer: Uuid,
) -> Result<Option<Transfer>, ArtifactError> {
    let row: Option<(Uuid, Uuid, Uuid, String, Uuid, String)> = sqlx::query_as(
        "SELECT source_situation_id,representation_id,artifact_id,note,recorded_by,recorded_at::text
         FROM niccolo.artifact_representation_transfers
         WHERE workspace_id=$1 AND target_situation_id=$2 AND transfer_id=$3",
    )
    .bind(workspace)
    .bind(target)
    .bind(transfer)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(
        |(source, representation_id, artifact_id, note, recorded_by, recorded_at)| Transfer {
            transfer_id: transfer,
            source_situation_id: source,
            target_situation_id: target,
            representation_id,
            artifact_id,
            note,
            recorded_by,
            recorded_at,
        },
    ))
}

pub async fn get(
    pool: &PgPool,
    workspace: Uuid,
    target: Uuid,
    transfer_id: Uuid,
) -> Result<Transfer, ArtifactError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let transfer = find(&mut tx, workspace, target, transfer_id)
        .await?
        .ok_or(ArtifactError::Invisible)?;
    tx.commit().await?;
    Ok(transfer)
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    target: Uuid,
    transfer_id: Uuid,
    actor: Uuid,
    input: Input,
) -> Result<(Transfer, bool), ArtifactError> {
    let note = input.note.trim();
    let source = input.source_situation_id;
    let representation = input.source_representation_id;
    if target == source || transfer_id.is_nil() || !valid_text(note, 4096) {
        return Err(ArtifactError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    // Serialize receiving the same representation against the target revision.
    let target_revision: Option<i64> = sqlx::query_scalar(
        "SELECT current_revision FROM niccolo.situations
         WHERE workspace_id=$1 AND situation_id=$2 FOR UPDATE",
    )
    .bind(workspace)
    .bind(target)
    .fetch_optional(&mut *tx)
    .await?;
    if target_revision.is_none() {
        return Err(ArtifactError::Invisible);
    }
    if let Some(stored) = find(&mut tx, workspace, target, transfer_id).await? {
        if stored.source_situation_id != source
            || stored.representation_id != representation
            || stored.note != note
            || stored.recorded_by != actor
        {
            return Err(ArtifactError::Conflict);
        }
        tx.commit().await?;
        return Ok((stored, false));
    }
    let artifact: Uuid = sqlx::query_scalar(
        "SELECT artifact_id FROM niccolo.situation_artifact_representations
         WHERE workspace_id=$1 AND situation_id=$2 AND representation_id=$3",
    )
    .bind(workspace)
    .bind(source)
    .bind(representation)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ArtifactError::Invisible)?;
    sqlx::query(
        "INSERT INTO niccolo.situation_artifacts
           (workspace_id,situation_id,artifact_id,added_by_principal_id)
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(target)
    .bind(artifact)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_artifact_representations
           (workspace_id,situation_id,artifact_id,representation_id,representation_type,
            producer_type,producer_label,production_method,scope_locator,content,
            completeness_status,epistemic_qualification,limitations,recorded_by_principal_id)
         SELECT workspace_id,$2,artifact_id,representation_id,representation_type,
                producer_type,producer_label,production_method,scope_locator,content,
                completeness_status,epistemic_qualification,limitations,$5
         FROM niccolo.situation_artifact_representations
         WHERE workspace_id=$1 AND situation_id=$3 AND representation_id=$4
         ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(target)
    .bind(source)
    .bind(representation)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted != 1 {
        return Err(ArtifactError::Conflict);
    }
    sqlx::query(
        "INSERT INTO niccolo.artifact_representation_transfers
           (workspace_id,target_situation_id,transfer_id,source_situation_id,
            representation_id,artifact_id,note,recorded_by)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(workspace)
    .bind(target)
    .bind(transfer_id)
    .bind(source)
    .bind(representation)
    .bind(artifact)
    .bind(note)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    checkpoint(
        &mut tx,
        workspace,
        target,
        actor,
        "ARTIFACT_REPRESENTATION_ADDED",
    )
    .await?;
    let transfer = find(&mut tx, workspace, target, transfer_id)
        .await?
        .ok_or(ArtifactError::Invisible)?;
    tx.commit().await?;
    Ok((transfer, true))
}
