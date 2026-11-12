//! Immutable local publication planning for exact operation-checkpoint packages.
//!
//! This GIT-002 slice deliberately performs no provider request. It binds an
//! immutable checkpoint package digest to a user-owned Git target and requested
//! ref so a later explicit publisher can retry without ambiguity.
use crate::{
    authz,
    bff_operations::OperationScope,
    checkpoint_package::{self, CheckpointPackageError},
    git_publication::{self, GitPublicationError, TargetView},
    operation_checkpoint::{self, CheckpointError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CheckpointGitPublicationError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("checkpoint unavailable: {0}")]
    Checkpoint(#[from] CheckpointError),
    #[error("checkpoint package unavailable: {0}")]
    Package(#[from] CheckpointPackageError),
    #[error("git target unavailable: {0}")]
    Git(#[from] GitPublicationError),
    #[error("invalid checkpoint publication request")]
    Invalid,
    #[error("checkpoint publication identity conflicts with recorded scope")]
    Conflict,
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPublicationPreview {
    pub target: TargetView,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub operation_id: Uuid,
    pub checkpoint_id: Uuid,
    pub checkpoint_seq: i64,
    pub checkpoint_kind: String,
    pub provisional: bool,
    pub terminal: bool,
    pub checkpoint_summary_sha256: String,
    pub scope_kind: String,
    pub package_sha256: String,
    pub file_count: u32,
    pub total_text_bytes: u64,
    pub requested_ref: String,
    pub includes_examined_source_text: bool,
    pub includes_original_uploads: bool,
    pub remote_publish_performed: bool,
    pub requires_transient_provider_authorization: bool,
    pub ready_for_remote_publish: bool,
    pub establishes_truth: bool,
    pub proves_completeness: bool,
    pub authorizes_real_world_action: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPublicationIntentView {
    pub publication_id: Uuid,
    pub target_id: Uuid,
    pub situation_id: Uuid,
    pub operation_id: Uuid,
    pub checkpoint_id: Uuid,
    pub checkpoint_seq: i64,
    pub checkpoint_kind: String,
    pub provisional: bool,
    pub terminal: bool,
    pub checkpoint_summary_sha256: String,
    pub package_sha256: String,
    pub file_count: u32,
    pub total_text_bytes: u64,
    pub requested_ref: String,
    pub scope_kind: String,
    pub includes_examined_source_text: bool,
    pub includes_original_uploads: bool,
    pub publication_state: String,
    pub requested_by_principal_id: Uuid,
    pub created_at: String,
    pub remote_publish_performed: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, CheckpointGitPublicationError> {
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(CheckpointGitPublicationError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn package_shape(
    package: &checkpoint_package::CheckpointPackage,
) -> Result<(u32, u64), CheckpointGitPublicationError> {
    let file_count =
        u32::try_from(package.files.len()).map_err(|_| CheckpointGitPublicationError::Invalid)?;
    let total_text_bytes = package.files.values().try_fold(0_u64, |total, value| {
        total
            .checked_add(value.len() as u64)
            .ok_or(CheckpointGitPublicationError::Invalid)
    })?;
    if file_count == 0 || file_count > 32 || total_text_bytes == 0 || total_text_bytes > 524_288 {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    Ok((file_count, total_text_bytes))
}

pub async fn preview(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    target_id: Uuid,
) -> Result<CheckpointPublicationPreview, CheckpointGitPublicationError> {
    if checkpoint_id.is_nil() || target_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let checkpoint = operation_checkpoint::get(pool, scope, checkpoint_id).await?;
    let package = checkpoint_package::build(scope, &checkpoint)?;
    let target = git_publication::get_target(pool, scope.workspace, target_id).await?;
    let (file_count, total_text_bytes) = package_shape(&package)?;
    Ok(CheckpointPublicationPreview {
        requested_ref: target.ref_name.clone(),
        target,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        operation_id: scope.operation,
        checkpoint_id,
        checkpoint_seq: checkpoint.checkpoint_seq,
        checkpoint_kind: checkpoint.checkpoint_kind,
        provisional: checkpoint.provisional,
        terminal: checkpoint.terminal,
        checkpoint_summary_sha256: checkpoint.summary_sha256,
        scope_kind: "OPERATION_CHECKPOINT".into(),
        package_sha256: checkpoint_package::package_sha256(&package),
        file_count,
        total_text_bytes,
        includes_examined_source_text: false,
        includes_original_uploads: false,
        remote_publish_performed: false,
        requires_transient_provider_authorization: true,
        ready_for_remote_publish: false,
        establishes_truth: false,
        proves_completeness: false,
        authorizes_real_world_action: false,
    })
}

struct IntentRow {
    publication_id: Uuid,
    target_id: Uuid,
    situation_id: Uuid,
    operation_id: Uuid,
    checkpoint_id: Uuid,
    checkpoint_seq: i64,
    checkpoint_kind: String,
    provisional: bool,
    terminal: bool,
    checkpoint_summary_sha256: String,
    package_sha256: String,
    file_count: i32,
    total_text_bytes: i64,
    requested_ref: String,
    scope_kind: String,
    includes_examined_source_text: bool,
    includes_original_uploads: bool,
    publication_state: String,
    requested_by_principal_id: Uuid,
    created_at: String,
}

fn decode_intent_row(row: PgRow) -> Result<IntentRow, sqlx::Error> {
    Ok(IntentRow {
        publication_id: row.try_get("publication_id")?,
        target_id: row.try_get("target_id")?,
        situation_id: row.try_get("situation_id")?,
        operation_id: row.try_get("operation_id")?,
        checkpoint_id: row.try_get("checkpoint_id")?,
        checkpoint_seq: row.try_get("checkpoint_seq")?,
        checkpoint_kind: row.try_get("checkpoint_kind")?,
        provisional: row.try_get("provisional")?,
        terminal: row.try_get("terminal")?,
        checkpoint_summary_sha256: row.try_get("checkpoint_summary_sha256")?,
        package_sha256: row.try_get("package_sha256")?,
        file_count: row.try_get("file_count")?,
        total_text_bytes: row.try_get("total_text_bytes")?,
        requested_ref: row.try_get("requested_ref")?,
        scope_kind: row.try_get("scope_kind")?,
        includes_examined_source_text: row.try_get("includes_examined_source_text")?,
        includes_original_uploads: row.try_get("includes_original_uploads")?,
        publication_state: row.try_get("publication_state")?,
        requested_by_principal_id: row.try_get("requested_by_principal_id")?,
        created_at: row.try_get("created_at")?,
    })
}

fn intent_from_row(
    row: IntentRow,
) -> Result<CheckpointPublicationIntentView, CheckpointGitPublicationError> {
    Ok(CheckpointPublicationIntentView {
        publication_id: row.publication_id,
        target_id: row.target_id,
        situation_id: row.situation_id,
        operation_id: row.operation_id,
        checkpoint_id: row.checkpoint_id,
        checkpoint_seq: row.checkpoint_seq,
        checkpoint_kind: row.checkpoint_kind,
        provisional: row.provisional,
        terminal: row.terminal,
        checkpoint_summary_sha256: row.checkpoint_summary_sha256,
        package_sha256: row.package_sha256,
        file_count: u32::try_from(row.file_count)
            .map_err(|_| CheckpointGitPublicationError::Conflict)?,
        total_text_bytes: u64::try_from(row.total_text_bytes)
            .map_err(|_| CheckpointGitPublicationError::Conflict)?,
        requested_ref: row.requested_ref,
        scope_kind: row.scope_kind,
        includes_examined_source_text: row.includes_examined_source_text,
        includes_original_uploads: row.includes_original_uploads,
        publication_state: row.publication_state,
        requested_by_principal_id: row.requested_by_principal_id,
        created_at: row.created_at,
        remote_publish_performed: false,
    })
}

pub async fn record_intent(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    actor: Uuid,
    publication_id: Uuid,
    target_id: Uuid,
) -> Result<(CheckpointPublicationIntentView, bool), CheckpointGitPublicationError> {
    if actor.is_nil() || publication_id.is_nil() || target_id.is_nil() || checkpoint_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let preview = preview(pool, scope, checkpoint_id, target_id).await?;
    let mut tx = scoped(pool, scope.workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.checkpoint_git_publication_intents
         (workspace_id,publication_id,target_id,situation_id,operation_id,checkpoint_id,
          checkpoint_seq,checkpoint_kind,provisional,terminal,checkpoint_summary_sha256,
          package_sha256,file_count,total_text_bytes,requested_ref,requested_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(publication_id)
    .bind(target_id)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(checkpoint_id)
    .bind(preview.checkpoint_seq)
    .bind(&preview.checkpoint_kind)
    .bind(preview.provisional)
    .bind(preview.terminal)
    .bind(&preview.checkpoint_summary_sha256)
    .bind(&preview.package_sha256)
    .bind(i32::try_from(preview.file_count).map_err(|_| CheckpointGitPublicationError::Invalid)?)
    .bind(
        i64::try_from(preview.total_text_bytes)
            .map_err(|_| CheckpointGitPublicationError::Invalid)?,
    )
    .bind(&preview.requested_ref)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row = sqlx::query(
        "SELECT publication_id,target_id,situation_id,operation_id,checkpoint_id,
                checkpoint_seq,checkpoint_kind,provisional,terminal,checkpoint_summary_sha256,
                package_sha256,file_count,total_text_bytes,requested_ref,scope_kind,
                includes_examined_source_text,includes_original_uploads,publication_state,
                requested_by_principal_id,created_at::text AS created_at
           FROM niccolo.checkpoint_git_publication_intents
          WHERE workspace_id=$1 AND publication_id=$2",
    )
    .bind(scope.workspace)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CheckpointGitPublicationError::Conflict)?;
    let row = decode_intent_row(row)?;

    if row.target_id != target_id
        || row.situation_id != scope.situation
        || row.operation_id != scope.operation
        || row.checkpoint_id != checkpoint_id
        || row.checkpoint_seq != preview.checkpoint_seq
        || row.checkpoint_kind != preview.checkpoint_kind
        || row.provisional != preview.provisional
        || row.terminal != preview.terminal
        || row.checkpoint_summary_sha256 != preview.checkpoint_summary_sha256
        || row.package_sha256 != preview.package_sha256
        || row.file_count
            != i32::try_from(preview.file_count)
                .map_err(|_| CheckpointGitPublicationError::Invalid)?
        || row.total_text_bytes
            != i64::try_from(preview.total_text_bytes)
                .map_err(|_| CheckpointGitPublicationError::Invalid)?
        || row.requested_ref != preview.requested_ref
        || row.scope_kind != "OPERATION_CHECKPOINT"
        || row.includes_examined_source_text
        || row.includes_original_uploads
        || row.publication_state != "AWAITING_REMOTE_AUTHORIZATION"
        || row.requested_by_principal_id != actor
    {
        return Err(CheckpointGitPublicationError::Conflict);
    }
    let view = intent_from_row(row)?;
    tx.commit().await?;
    Ok((view, !inserted))
}
