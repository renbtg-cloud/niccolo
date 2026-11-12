//! GIT-001 first slice: local, auditable publication planning for a user-owned
//! GitHub repository. This module deliberately performs no network request and
//! accepts no provider secret. "Planned" is never reported as "published".
use crate::{authz, presentation_package};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum GitPublicationError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("presentation unavailable: {0}")]
    Presentation(#[from] presentation_package::PresentationError),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid git publication request")]
    Invalid,
    #[error("git publication target unavailable")]
    Invisible,
    #[error("git publication identity conflicts with recorded scope")]
    Conflict,
    #[error("git publication intent no longer matches the current deterministic package")]
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetInput {
    pub target_id: Uuid,
    pub repository_owner: String,
    pub repository_name: String,
    pub ref_name: String,
    pub repository_visibility: String,
    pub sensitive_exports_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetView {
    pub target_id: Uuid,
    pub provider: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub ref_name: String,
    pub repository_visibility: String,
    pub sensitive_exports_allowed: bool,
    pub authorization_mode: String,
    pub created_by_principal_id: Uuid,
    pub created_at: String,
    pub remote_connection_verified: bool,
    pub provider_secret_stored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationPreview {
    pub target: TargetView,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationIntentView {
    pub publication_id: Uuid,
    pub target_id: Uuid,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub scope_kind: String,
    pub package_sha256: String,
    pub file_count: u32,
    pub total_text_bytes: u64,
    pub requested_ref: String,
    pub includes_examined_source_text: bool,
    pub includes_original_uploads: bool,
    pub publication_state: String,
    pub requested_by_principal_id: Uuid,
    pub created_at: String,
    pub remote_publish_performed: bool,
}

fn valid_repo_component(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
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
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-'))
}

pub fn validate_target(input: &TargetInput) -> Result<(), GitPublicationError> {
    if input.target_id.is_nil()
        || !valid_repo_component(&input.repository_owner, 100)
        || !valid_repo_component(&input.repository_name, 100)
        || input.repository_name.ends_with(".git")
        || !valid_ref(&input.ref_name)
        || !matches!(
            input.repository_visibility.as_str(),
            "PRIVATE" | "PUBLIC" | "UNKNOWN"
        )
        || (input.sensitive_exports_allowed && input.repository_visibility != "PRIVATE")
    {
        return Err(GitPublicationError::Invalid);
    }
    Ok(())
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, GitPublicationError> {
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(GitPublicationError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

type TargetRow = (
    Uuid,
    String,
    String,
    String,
    String,
    String,
    bool,
    String,
    Uuid,
    String,
);

fn target_from_row(row: TargetRow) -> TargetView {
    TargetView {
        target_id: row.0,
        provider: row.1,
        repository_owner: row.2,
        repository_name: row.3,
        ref_name: row.4,
        repository_visibility: row.5,
        sensitive_exports_allowed: row.6,
        authorization_mode: row.7,
        created_by_principal_id: row.8,
        created_at: row.9,
        remote_connection_verified: false,
        provider_secret_stored: false,
    }
}

pub async fn register_target(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    input: &TargetInput,
) -> Result<(TargetView, bool), GitPublicationError> {
    validate_target(input)?;
    let mut tx = scoped(pool, workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.git_publication_targets
         (workspace_id,target_id,provider,repository_owner,repository_name,ref_name,
          repository_visibility,sensitive_exports_allowed,created_by_principal_id)
         VALUES($1,$2,'GITHUB',$3,$4,$5,$6,$7,$8)
         ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(input.target_id)
    .bind(&input.repository_owner)
    .bind(&input.repository_name)
    .bind(&input.ref_name)
    .bind(&input.repository_visibility)
    .bind(input.sensitive_exports_allowed)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: TargetRow = sqlx::query_as(
        "SELECT target_id,provider,repository_owner,repository_name,ref_name,
                repository_visibility,sensitive_exports_allowed,authorization_mode,
                created_by_principal_id,created_at::text
           FROM niccolo.git_publication_targets
          WHERE workspace_id=$1 AND target_id=$2",
    )
    .bind(workspace)
    .bind(input.target_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GitPublicationError::Conflict)?;
    if row.2 != input.repository_owner
        || row.3 != input.repository_name
        || row.4 != input.ref_name
        || row.5 != input.repository_visibility
        || row.6 != input.sensitive_exports_allowed
        || row.8 != actor
    {
        return Err(GitPublicationError::Conflict);
    }
    let view = target_from_row(row);
    tx.commit().await?;
    Ok((view, !inserted))
}

pub async fn list_targets(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Vec<TargetView>, GitPublicationError> {
    let mut tx = scoped(pool, workspace).await?;
    let rows: Vec<TargetRow> = sqlx::query_as(
        "SELECT target_id,provider,repository_owner,repository_name,ref_name,
                repository_visibility,sensitive_exports_allowed,authorization_mode,
                created_by_principal_id,created_at::text
           FROM niccolo.git_publication_targets
          WHERE workspace_id=$1
          ORDER BY created_at,target_id
          LIMIT 100",
    )
    .bind(workspace)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(target_from_row).collect())
}

pub async fn preview(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    target_id: Uuid,
) -> Result<PublicationPreview, GitPublicationError> {
    if target_id.is_nil() {
        return Err(GitPublicationError::Invalid);
    }
    let package = presentation_package::build(pool, workspace, situation, analysis).await?;
    let package_sha256 = presentation_package::package_sha256(&package)?;
    let file_count =
        u32::try_from(package.files.len()).map_err(|_| GitPublicationError::Invalid)?;
    let total_text_bytes = package.files.values().try_fold(0_u64, |acc, value| {
        acc.checked_add(value.len() as u64)
            .ok_or(GitPublicationError::Invalid)
    })?;
    let mut tx = scoped(pool, workspace).await?;
    let row: TargetRow = sqlx::query_as(
        "SELECT target_id,provider,repository_owner,repository_name,ref_name,
                repository_visibility,sensitive_exports_allowed,authorization_mode,
                created_by_principal_id,created_at::text
           FROM niccolo.git_publication_targets
          WHERE workspace_id=$1 AND target_id=$2",
    )
    .bind(workspace)
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GitPublicationError::Invisible)?;
    tx.commit().await?;
    let target = target_from_row(row);
    Ok(PublicationPreview {
        requested_ref: target.ref_name.clone(),
        target,
        situation_id: situation,
        analysis_id: analysis,
        scope_kind: "STANDARD_PRESENTATION".into(),
        package_sha256,
        file_count,
        total_text_bytes,
        includes_examined_source_text: false,
        includes_original_uploads: false,
        remote_publish_performed: false,
        requires_transient_provider_authorization: true,
        ready_for_remote_publish: false,
    })
}

type IntentRow = (
    Uuid,
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    i32,
    i64,
    String,
    bool,
    bool,
    String,
    Uuid,
    String,
);

fn intent_from_row(row: IntentRow) -> Result<PublicationIntentView, GitPublicationError> {
    Ok(PublicationIntentView {
        publication_id: row.0,
        target_id: row.1,
        situation_id: row.2,
        analysis_id: row.3,
        scope_kind: row.4,
        package_sha256: row.5,
        file_count: u32::try_from(row.6).map_err(|_| GitPublicationError::Conflict)?,
        total_text_bytes: u64::try_from(row.7).map_err(|_| GitPublicationError::Conflict)?,
        requested_ref: row.8,
        includes_examined_source_text: row.9,
        includes_original_uploads: row.10,
        publication_state: row.11,
        requested_by_principal_id: row.12,
        created_at: row.13,
        remote_publish_performed: false,
    })
}

pub async fn record_intent(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    actor: Uuid,
    publication_id: Uuid,
    target_id: Uuid,
) -> Result<(PublicationIntentView, bool), GitPublicationError> {
    if publication_id.is_nil() || target_id.is_nil() {
        return Err(GitPublicationError::Invalid);
    }
    let preview = preview(pool, workspace, situation, analysis, target_id).await?;
    let mut tx = scoped(pool, workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.git_publication_intents
         (workspace_id,publication_id,target_id,situation_id,analysis_id,scope_kind,
          package_sha256,file_count,total_text_bytes,requested_ref,
          includes_examined_source_text,includes_original_uploads,requested_by_principal_id)
         VALUES($1,$2,$3,$4,$5,'STANDARD_PRESENTATION',$6,$7,$8,$9,FALSE,FALSE,$10)
         ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(publication_id)
    .bind(target_id)
    .bind(situation)
    .bind(analysis)
    .bind(&preview.package_sha256)
    .bind(i32::try_from(preview.file_count).map_err(|_| GitPublicationError::Invalid)?)
    .bind(i64::try_from(preview.total_text_bytes).map_err(|_| GitPublicationError::Invalid)?)
    .bind(&preview.requested_ref)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: IntentRow = sqlx::query_as(
        "SELECT publication_id,target_id,situation_id,analysis_id,scope_kind,
                package_sha256,file_count,total_text_bytes,requested_ref,
                includes_examined_source_text,includes_original_uploads,publication_state,
                requested_by_principal_id,created_at::text
           FROM niccolo.git_publication_intents
          WHERE workspace_id=$1 AND publication_id=$2",
    )
    .bind(workspace)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GitPublicationError::Conflict)?;
    if row.1 != target_id
        || row.2 != situation
        || row.3 != analysis
        || row.4 != "STANDARD_PRESENTATION"
        || row.5 != preview.package_sha256
        || row.6 != i32::try_from(preview.file_count).map_err(|_| GitPublicationError::Invalid)?
        || row.7
            != i64::try_from(preview.total_text_bytes).map_err(|_| GitPublicationError::Invalid)?
        || row.8 != preview.requested_ref
        || row.9
        || row.10
        || row.11 != "AWAITING_REMOTE_AUTHORIZATION"
        || row.12 != actor
    {
        return Err(GitPublicationError::Conflict);
    }
    let view = intent_from_row(row)?;
    tx.commit().await?;
    Ok((view, !inserted))
}

pub async fn list_intents(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    limit: i64,
) -> Result<Vec<PublicationIntentView>, GitPublicationError> {
    if !(1..=100).contains(&limit) {
        return Err(GitPublicationError::Invalid);
    }
    // Loading the package first proves the analysis is visible and published.
    let _ = presentation_package::build(pool, workspace, situation, analysis).await?;
    let mut tx = scoped(pool, workspace).await?;
    let rows: Vec<IntentRow> = sqlx::query_as(
        "SELECT publication_id,target_id,situation_id,analysis_id,scope_kind,
                package_sha256,file_count,total_text_bytes,requested_ref,
                includes_examined_source_text,includes_original_uploads,publication_state,
                requested_by_principal_id,created_at::text
           FROM niccolo.git_publication_intents
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
          ORDER BY created_at DESC,publication_id DESC
          LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    rows.into_iter().map(intent_from_row).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationMaterial {
    pub target: TargetView,
    pub intent: PublicationIntentView,
    pub package: presentation_package::PresentationPackage,
}

pub async fn publication_material(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    publication_id: Uuid,
) -> Result<PublicationMaterial, GitPublicationError> {
    if publication_id.is_nil() {
        return Err(GitPublicationError::Invalid);
    }
    let package = presentation_package::build(pool, workspace, situation, analysis).await?;
    let digest = presentation_package::package_sha256(&package)?;
    let file_count = u32::try_from(package.files.len()).map_err(|_| GitPublicationError::Stale)?;
    let total_text_bytes = package.files.values().try_fold(0_u64, |total, value| {
        total
            .checked_add(value.len() as u64)
            .ok_or(GitPublicationError::Stale)
    })?;

    let mut tx = scoped(pool, workspace).await?;
    let intent_row: IntentRow = sqlx::query_as(
        "SELECT publication_id,target_id,situation_id,analysis_id,scope_kind,
                package_sha256,file_count,total_text_bytes,requested_ref,
                includes_examined_source_text,includes_original_uploads,publication_state,
                requested_by_principal_id,created_at::text
           FROM niccolo.git_publication_intents
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND publication_id=$4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GitPublicationError::Invisible)?;
    let intent = intent_from_row(intent_row)?;
    let target_row: TargetRow = sqlx::query_as(
        "SELECT target_id,provider,repository_owner,repository_name,ref_name,
                repository_visibility,sensitive_exports_allowed,authorization_mode,
                created_by_principal_id,created_at::text
           FROM niccolo.git_publication_targets
          WHERE workspace_id=$1 AND target_id=$2",
    )
    .bind(workspace)
    .bind(intent.target_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GitPublicationError::Invisible)?;
    let target = target_from_row(target_row);
    tx.commit().await?;

    if intent.scope_kind != "STANDARD_PRESENTATION"
        || intent.includes_examined_source_text
        || intent.includes_original_uploads
        || intent.package_sha256 != digest
        || intent.file_count != file_count
        || intent.total_text_bytes != total_text_bytes
        || intent.requested_ref != target.ref_name
    {
        return Err(GitPublicationError::Stale);
    }
    Ok(PublicationMaterial {
        target,
        intent,
        package,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteEventKind {
    PushAccepted,
    RefVerified,
    PublishFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEventInput {
    pub event_id: Uuid,
    pub publication_id: Uuid,
    pub kind: RemoteEventKind,
    pub remote_commit_sha: Option<String>,
    pub observed_ref: Option<String>,
    pub failure_class: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePublicationEventView {
    pub event_id: Uuid,
    pub publication_id: Uuid,
    pub event_kind: String,
    pub remote_commit_sha: Option<String>,
    pub observed_ref: Option<String>,
    pub failure_class: Option<String>,
    pub recorded_by_principal_id: Uuid,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePublicationStatus {
    pub publication_id: Uuid,
    pub state: String,
    pub remote_publish_verified: bool,
    pub verified_commit_sha: Option<String>,
    pub verified_ref: Option<String>,
    pub event_count: u32,
    pub last_event_kind: Option<String>,
    pub last_event_at: Option<String>,
    pub last_failure_class: Option<String>,
}

fn valid_remote_sha(value: &str) -> bool {
    (40..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_remote_event(input: &RemoteEventInput) -> Result<(), GitPublicationError> {
    if input.event_id.is_nil() || input.publication_id.is_nil() {
        return Err(GitPublicationError::Invalid);
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
                return Err(GitPublicationError::Invalid);
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
                return Err(GitPublicationError::Invalid);
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
                return Err(GitPublicationError::Invalid);
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
) -> Result<(RemotePublicationEventView, bool), GitPublicationError> {
    if actor.is_nil() {
        return Err(GitPublicationError::Invalid);
    }
    validate_remote_event(input)?;
    let kind = event_kind(input.kind);
    let mut tx = scoped(pool, workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.git_publication_events
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
           FROM niccolo.git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2 AND event_id=$3 AND event_kind=$4",
    )
    .bind(workspace)
    .bind(input.publication_id)
    .bind(input.event_id)
    .bind(kind)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(GitPublicationError::Conflict)?;
    if row.2 != kind
        || row.3 != input.remote_commit_sha
        || row.4 != input.observed_ref
        || row.5 != input.failure_class
        || row.6 != actor
    {
        return Err(GitPublicationError::Conflict);
    }
    let view = remote_event_from_row(row);
    tx.commit().await?;
    Ok((view, !inserted))
}

pub async fn list_remote_events(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    publication_id: Uuid,
    limit: i64,
) -> Result<Vec<RemotePublicationEventView>, GitPublicationError> {
    if publication_id.is_nil() || !(1..=100).contains(&limit) {
        return Err(GitPublicationError::Invalid);
    }
    let mut tx = scoped(pool, workspace).await?;
    let intent_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM niccolo.git_publication_intents
              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND publication_id=$4
         )",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(publication_id)
    .fetch_one(&mut *tx)
    .await?;
    if !intent_exists {
        return Err(GitPublicationError::Invisible);
    }
    let rows: Vec<RemoteEventRow> = sqlx::query_as(
        "SELECT event_id,publication_id,event_kind,remote_commit_sha,observed_ref,
                failure_class,recorded_by_principal_id,created_at::text
           FROM niccolo.git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2
          ORDER BY created_at DESC,event_id DESC,event_kind DESC
          LIMIT $3",
    )
    .bind(workspace)
    .bind(publication_id)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(remote_event_from_row).collect())
}

pub async fn remote_status(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    publication_id: Uuid,
) -> Result<RemotePublicationStatus, GitPublicationError> {
    if publication_id.is_nil() {
        return Err(GitPublicationError::Invalid);
    }
    let mut tx = scoped(pool, workspace).await?;
    let intent_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM niccolo.git_publication_intents
              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND publication_id=$4
         )",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(publication_id)
    .fetch_one(&mut *tx)
    .await?;
    if !intent_exists {
        return Err(GitPublicationError::Invisible);
    }

    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM niccolo.git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2",
    )
    .bind(workspace)
    .bind(publication_id)
    .fetch_one(&mut *tx)
    .await?;

    let verified: Option<(String, String)> = sqlx::query_as(
        "SELECT remote_commit_sha,observed_ref
           FROM niccolo.git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2 AND event_kind='REF_VERIFIED'
          ORDER BY created_at DESC,event_id DESC
          LIMIT 1",
    )
    .bind(workspace)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?;

    let last: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT event_kind,failure_class,created_at::text
           FROM niccolo.git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2
          ORDER BY created_at DESC,event_id DESC,event_kind DESC
          LIMIT 1",
    )
    .bind(workspace)
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
        event_count: u32::try_from(event_count).map_err(|_| GitPublicationError::Conflict)?,
        last_event_kind: last.as_ref().map(|row| row.0.clone()),
        last_event_at: last.as_ref().map(|row| row.2.clone()),
        last_failure_class,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(owner: &str, repo: &str, ref_name: &str) -> TargetInput {
        TargetInput {
            target_id: Uuid::from_u128(1),
            repository_owner: owner.into(),
            repository_name: repo.into(),
            ref_name: ref_name.into(),
            repository_visibility: "PRIVATE".into(),
            sensitive_exports_allowed: false,
        }
    }

    #[test]
    fn remote_publication_event_shapes_are_strict_and_secret_free() {
        let publication_id = Uuid::from_u128(2);
        let event_id = Uuid::from_u128(3);
        let verified = RemoteEventInput {
            event_id,
            publication_id,
            kind: RemoteEventKind::RefVerified,
            remote_commit_sha: Some("a".repeat(40)),
            observed_ref: Some("main".into()),
            failure_class: None,
        };
        assert!(validate_remote_event(&verified).is_ok());
        let mut bad = verified.clone();
        bad.remote_commit_sha = Some("A".repeat(40));
        assert!(validate_remote_event(&bad).is_err());
        let failed = RemoteEventInput {
            event_id,
            publication_id,
            kind: RemoteEventKind::PublishFailed,
            remote_commit_sha: None,
            observed_ref: None,
            failure_class: Some("TRANSPORT".into()),
        };
        assert!(validate_remote_event(&failed).is_ok());
        let mut raw_provider_error = failed;
        raw_provider_error.failure_class = Some("HTTP 403: secret-ish provider body".into());
        assert!(validate_remote_event(&raw_provider_error).is_err());
    }

    #[test]
    fn repository_coordinates_are_narrow_not_urls() {
        assert!(validate_target(&target("renbtg-cloud", "niccolo-history", "main")).is_ok());
        assert!(validate_target(&target("https://evil.example", "repo", "main")).is_err());
        assert!(validate_target(&target("owner", "../repo", "main")).is_err());
        assert!(validate_target(&target("owner", "repo.git", "main")).is_err());
    }

    #[test]
    fn unsafe_git_refs_fail_closed() {
        for bad in [
            "",
            "/main",
            ".hidden",
            "a..b",
            "a@{b",
            "a//b",
            "main.lock",
            "main:bad",
            "main?bad",
        ] {
            assert!(
                validate_target(&target("owner", "repo", bad)).is_err(),
                "{bad}"
            );
        }
        assert!(validate_target(&target("owner", "repo", "refs/heads/work")).is_ok());
    }

    #[test]
    fn sensitive_remote_scope_requires_declared_private_target() {
        let mut input = target("owner", "repo", "main");
        input.sensitive_exports_allowed = true;
        input.repository_visibility = "UNKNOWN".into();
        assert!(validate_target(&input).is_err());
        input.repository_visibility = "PRIVATE".into();
        assert!(validate_target(&input).is_ok());
    }
}
