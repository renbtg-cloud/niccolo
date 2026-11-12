//! Immutable publication planning and verified remote publication for exact
//! operation-checkpoint packages.
//!
//! GIT-002 records the exact package digest, target and ref before any provider
//! request. Explicit publication then uses transient authorization, a non-force
//! ref update and an append-only verification ledger so retries stay auditable.
use crate::{
    authz,
    bff_operations::OperationScope,
    checkpoint_milestone::{self, MilestoneError, MilestoneView},
    checkpoint_package::{self, CheckpointPackageError},
    git_publication::{
        self, GitPublicationError, RemoteEventInput, RemoteEventKind, RemotePublicationEventView,
        RemotePublicationStatus, TargetView,
    },
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
    #[error("checkpoint milestone unavailable: {0}")]
    Milestone(#[from] MilestoneError),
    #[error("git target unavailable: {0}")]
    Git(#[from] GitPublicationError),
    #[error("invalid checkpoint publication request")]
    Invalid,
    #[error("checkpoint publication identity conflicts with recorded scope")]
    Conflict,
    #[error("checkpoint publication intent unavailable")]
    Invisible,
    #[error("checkpoint publication intent no longer matches the immutable package")]
    Stale,
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
    pub milestone_id: Option<Uuid>,
    pub milestone_kind: Option<String>,
    pub milestone_reason: Option<String>,
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
    pub milestone_id: Option<Uuid>,
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

async fn package_for_scope(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint: &operation_checkpoint::CheckpointView,
    milestone_id: Option<Uuid>,
) -> Result<
    (checkpoint_package::CheckpointPackage, Option<MilestoneView>),
    CheckpointGitPublicationError,
> {
    match milestone_id {
        Some(id) => {
            if id.is_nil() {
                return Err(CheckpointGitPublicationError::Invalid);
            }
            let milestone =
                checkpoint_milestone::get(pool, scope, checkpoint.checkpoint_id).await?;
            if milestone.milestone_id != id {
                return Err(CheckpointGitPublicationError::Conflict);
            }
            let package = checkpoint_package::build_milestone(scope, checkpoint, &milestone)?;
            Ok((package, Some(milestone)))
        }
        None => Ok((checkpoint_package::build(scope, checkpoint)?, None)),
    }
}

pub async fn preview(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    target_id: Uuid,
    milestone_id: Option<Uuid>,
) -> Result<CheckpointPublicationPreview, CheckpointGitPublicationError> {
    if checkpoint_id.is_nil() || target_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let checkpoint = operation_checkpoint::get(pool, scope, checkpoint_id).await?;
    let (package, milestone) = package_for_scope(pool, scope, &checkpoint, milestone_id).await?;
    let target = git_publication::get_target(pool, scope.workspace, target_id).await?;
    // Checkpoint summaries may contain sensitive human/investigative context even
    // though the bounded package excludes original uploads and examined source text.
    // Keep this first remote-publication slice private-by-declaration.
    if target.repository_visibility != "PRIVATE" {
        return Err(CheckpointGitPublicationError::Invalid);
    }
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
        milestone_id: milestone.as_ref().map(|value| value.milestone_id),
        milestone_kind: milestone.as_ref().map(|value| value.milestone_kind.clone()),
        milestone_reason: milestone.as_ref().map(|value| value.reason.clone()),
        scope_kind: package.scope_kind.clone(),
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
    milestone_id: Option<Uuid>,
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
        milestone_id: row.try_get("milestone_id")?,
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
        milestone_id: row.milestone_id,
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
    milestone_id: Option<Uuid>,
) -> Result<(CheckpointPublicationIntentView, bool), CheckpointGitPublicationError> {
    if actor.is_nil() || publication_id.is_nil() || target_id.is_nil() || checkpoint_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let preview = preview(pool, scope, checkpoint_id, target_id, milestone_id).await?;
    let mut tx = scoped(pool, scope.workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.checkpoint_git_publication_intents
         (workspace_id,publication_id,target_id,situation_id,operation_id,checkpoint_id,
          checkpoint_seq,checkpoint_kind,provisional,terminal,checkpoint_summary_sha256,
          milestone_id,package_sha256,file_count,total_text_bytes,requested_ref,scope_kind,
          requested_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
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
    .bind(preview.milestone_id)
    .bind(&preview.package_sha256)
    .bind(i32::try_from(preview.file_count).map_err(|_| CheckpointGitPublicationError::Invalid)?)
    .bind(
        i64::try_from(preview.total_text_bytes)
            .map_err(|_| CheckpointGitPublicationError::Invalid)?,
    )
    .bind(&preview.requested_ref)
    .bind(&preview.scope_kind)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row = sqlx::query(
        "SELECT publication_id,target_id,situation_id,operation_id,checkpoint_id,
                checkpoint_seq,checkpoint_kind,provisional,terminal,checkpoint_summary_sha256,
                milestone_id,package_sha256,file_count,total_text_bytes,requested_ref,scope_kind,
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
        || row.milestone_id != preview.milestone_id
        || row.package_sha256 != preview.package_sha256
        || row.file_count
            != i32::try_from(preview.file_count)
                .map_err(|_| CheckpointGitPublicationError::Invalid)?
        || row.total_text_bytes
            != i64::try_from(preview.total_text_bytes)
                .map_err(|_| CheckpointGitPublicationError::Invalid)?
        || row.requested_ref != preview.requested_ref
        || row.scope_kind != preview.scope_kind
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

#[derive(Clone, Debug)]
pub struct CheckpointPublicationMaterial {
    pub target: TargetView,
    pub intent: CheckpointPublicationIntentView,
    pub package: checkpoint_package::CheckpointPackage,
}

async fn load_intent(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    publication_id: Uuid,
) -> Result<CheckpointPublicationIntentView, CheckpointGitPublicationError> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let row = sqlx::query(
        "SELECT publication_id,target_id,situation_id,operation_id,checkpoint_id,
                checkpoint_seq,checkpoint_kind,provisional,terminal,checkpoint_summary_sha256,
                milestone_id,package_sha256,file_count,total_text_bytes,requested_ref,scope_kind,
                includes_examined_source_text,includes_original_uploads,publication_state,
                requested_by_principal_id,created_at::text AS created_at
           FROM niccolo.checkpoint_git_publication_intents
          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
            AND checkpoint_id=$4 AND publication_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(checkpoint_id)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CheckpointGitPublicationError::Invisible)?;
    let row = decode_intent_row(row)?;
    tx.commit().await?;
    intent_from_row(row)
}

pub async fn publication_material(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    publication_id: Uuid,
) -> Result<CheckpointPublicationMaterial, CheckpointGitPublicationError> {
    if checkpoint_id.is_nil() || publication_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let intent = load_intent(pool, scope, checkpoint_id, publication_id).await?;
    let checkpoint = operation_checkpoint::get(pool, scope, checkpoint_id).await?;
    let (package, milestone) =
        package_for_scope(pool, scope, &checkpoint, intent.milestone_id).await?;
    let target = git_publication::get_target(pool, scope.workspace, intent.target_id).await?;
    let (file_count, total_text_bytes) = package_shape(&package)?;
    let digest = checkpoint_package::package_sha256(&package);
    if target.repository_visibility != "PRIVATE"
        || intent.scope_kind != package.scope_kind
        || intent.milestone_id != milestone.as_ref().map(|value| value.milestone_id)
        || intent.publication_state != "AWAITING_REMOTE_AUTHORIZATION"
        || intent.includes_examined_source_text
        || intent.includes_original_uploads
        || intent.requested_ref != target.ref_name
        || intent.checkpoint_seq != checkpoint.checkpoint_seq
        || intent.checkpoint_kind != checkpoint.checkpoint_kind
        || intent.provisional != checkpoint.provisional
        || intent.terminal != checkpoint.terminal
        || intent.checkpoint_summary_sha256 != checkpoint.summary_sha256
        || intent.package_sha256 != digest
        || intent.file_count != file_count
        || intent.total_text_bytes != total_text_bytes
    {
        return Err(CheckpointGitPublicationError::Stale);
    }
    Ok(CheckpointPublicationMaterial {
        target,
        intent,
        package,
    })
}

fn valid_remote_sha(value: &str) -> bool {
    (40..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 244
        && !value.starts_with('/')
        && !value.starts_with('.')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.ends_with(".lock")
        && !value.contains("..")
        && !value.contains("@{")
        && !value.contains("//")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
}

fn validate_remote_event(input: &RemoteEventInput) -> Result<(), CheckpointGitPublicationError> {
    if input.event_id.is_nil() || input.publication_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    match input.kind {
        RemoteEventKind::PushAccepted => {
            if input
                .remote_commit_sha
                .as_deref()
                .is_none_or(|sha| !valid_remote_sha(sha))
                || input.observed_ref.is_some()
                || input.failure_class.is_some()
            {
                return Err(CheckpointGitPublicationError::Invalid);
            }
        }
        RemoteEventKind::RefVerified => {
            if input
                .remote_commit_sha
                .as_deref()
                .is_none_or(|sha| !valid_remote_sha(sha))
                || input
                    .observed_ref
                    .as_deref()
                    .is_none_or(|value| !valid_ref(value))
                || input.failure_class.is_some()
            {
                return Err(CheckpointGitPublicationError::Invalid);
            }
        }
        RemoteEventKind::PublishFailed => {
            if input.remote_commit_sha.is_some()
                || input.observed_ref.is_some()
                || !matches!(
                    input.failure_class.as_deref(),
                    Some(
                        "TRANSPORT"
                            | "PROVIDER_REJECTED"
                            | "VERIFICATION_MISMATCH"
                            | "LOCAL_INTEGRITY"
                    )
                )
            {
                return Err(CheckpointGitPublicationError::Invalid);
            }
        }
    }
    Ok(())
}

fn event_kind(kind: RemoteEventKind) -> &'static str {
    match kind {
        RemoteEventKind::PushAccepted => "PUSH_ACCEPTED",
        RemoteEventKind::RefVerified => "REF_VERIFIED",
        RemoteEventKind::PublishFailed => "PUBLISH_FAILED",
    }
}

type RemoteEventRow = (
    Uuid,
    Uuid,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Uuid,
    String,
);

fn remote_event_from_row(row: RemoteEventRow) -> RemotePublicationEventView {
    RemotePublicationEventView {
        event_id: row.0,
        publication_id: row.1,
        event_kind: row.2,
        remote_commit_sha: row.3,
        observed_ref: row.4,
        failure_class: row.5,
        recorded_by_principal_id: row.6,
        created_at: row.7,
    }
}

pub async fn record_remote_event(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    input: &RemoteEventInput,
) -> Result<(RemotePublicationEventView, bool), CheckpointGitPublicationError> {
    if actor.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    validate_remote_event(input)?;
    let kind = event_kind(input.kind);
    let mut tx = scoped(pool, workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.checkpoint_git_publication_events
         (workspace_id,publication_id,event_id,event_kind,remote_commit_sha,observed_ref,
          failure_class,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(input.publication_id)
    .bind(input.event_id)
    .bind(kind)
    .bind(input.remote_commit_sha.as_deref())
    .bind(input.observed_ref.as_deref())
    .bind(input.failure_class.as_deref())
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: RemoteEventRow = sqlx::query_as(
        "SELECT event_id,publication_id,event_kind,remote_commit_sha,observed_ref,
                failure_class,recorded_by_principal_id,created_at::text
           FROM niccolo.checkpoint_git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2 AND event_id=$3 AND event_kind=$4",
    )
    .bind(workspace)
    .bind(input.publication_id)
    .bind(input.event_id)
    .bind(kind)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CheckpointGitPublicationError::Conflict)?;
    if row.2 != kind
        || row.3 != input.remote_commit_sha
        || row.4 != input.observed_ref
        || row.5 != input.failure_class
        || row.6 != actor
    {
        return Err(CheckpointGitPublicationError::Conflict);
    }
    let view = remote_event_from_row(row);
    tx.commit().await?;
    Ok((view, !inserted))
}

async fn ensure_intent_visible(
    tx: &mut Transaction<'_, Postgres>,
    scope: OperationScope,
    checkpoint_id: Uuid,
    publication_id: Uuid,
) -> Result<(), CheckpointGitPublicationError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM niccolo.checkpoint_git_publication_intents
              WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
                AND checkpoint_id=$4 AND publication_id=$5
         )",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(checkpoint_id)
    .bind(publication_id)
    .fetch_one(&mut **tx)
    .await?;
    if !exists {
        return Err(CheckpointGitPublicationError::Invisible);
    }
    Ok(())
}

pub async fn list_remote_events(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    publication_id: Uuid,
    limit: i64,
) -> Result<Vec<RemotePublicationEventView>, CheckpointGitPublicationError> {
    if checkpoint_id.is_nil() || publication_id.is_nil() || !(1..=100).contains(&limit) {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace).await?;
    ensure_intent_visible(&mut tx, scope, checkpoint_id, publication_id).await?;
    let rows: Vec<RemoteEventRow> = sqlx::query_as(
        "SELECT event_id,publication_id,event_kind,remote_commit_sha,observed_ref,
                failure_class,recorded_by_principal_id,created_at::text
           FROM niccolo.checkpoint_git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2
          ORDER BY created_at DESC,event_id DESC,event_kind DESC
          LIMIT $3",
    )
    .bind(scope.workspace)
    .bind(publication_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(remote_event_from_row).collect())
}

pub async fn remote_status(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    publication_id: Uuid,
) -> Result<RemotePublicationStatus, CheckpointGitPublicationError> {
    if checkpoint_id.is_nil() || publication_id.is_nil() {
        return Err(CheckpointGitPublicationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace).await?;
    ensure_intent_visible(&mut tx, scope, checkpoint_id, publication_id).await?;
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM niccolo.checkpoint_git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2",
    )
    .bind(scope.workspace)
    .bind(publication_id)
    .fetch_one(&mut *tx)
    .await?;
    let verified: Option<(String, String)> = sqlx::query_as(
        "SELECT remote_commit_sha,observed_ref
           FROM niccolo.checkpoint_git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2 AND event_kind='REF_VERIFIED'
          ORDER BY created_at DESC,event_id DESC LIMIT 1",
    )
    .bind(scope.workspace)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?;
    let last: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT event_kind,failure_class,created_at::text
           FROM niccolo.checkpoint_git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2
          ORDER BY created_at DESC,event_id DESC,event_kind DESC LIMIT 1",
    )
    .bind(scope.workspace)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let (remote_publish_verified, verified_commit_sha, verified_ref, state) =
        if let Some((commit, observed_ref)) = verified {
            (true, Some(commit), Some(observed_ref), "VERIFIED_PUBLISHED")
        } else {
            let state = match last.as_ref().map(|row| row.0.as_str()) {
                Some("PUSH_ACCEPTED") => "PUSHED_UNVERIFIED",
                Some("PUBLISH_FAILED") => "REMOTE_ATTEMPT_FAILED",
                _ => "AWAITING_REMOTE_AUTHORIZATION",
            };
            (false, None, None, state)
        };
    let last_failure_class = last
        .as_ref()
        .and_then(|row| (row.0 == "PUBLISH_FAILED").then(|| row.1.clone()).flatten());
    Ok(RemotePublicationStatus {
        publication_id,
        state: state.into(),
        remote_publish_verified,
        verified_commit_sha,
        verified_ref,
        event_count: u32::try_from(event_count)
            .map_err(|_| CheckpointGitPublicationError::Conflict)?,
        last_event_kind: last.as_ref().map(|row| row.0.clone()),
        last_event_at: last.as_ref().map(|row| row.2.clone()),
        last_failure_class,
    })
}
