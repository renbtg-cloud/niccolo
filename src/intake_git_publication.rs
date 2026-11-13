//! Immutable planning for publishing one deterministic RECEIVED-source receipt.
//!
//! This layer records the exact receipt digest, target and ref before any provider
//! request. It never stores provider credentials and never upgrades RECEIVED into
//! authentic, true, complete, corroborated or analyzed.
use crate::{
    authz,
    git_publication::{self, GitPublicationError, TargetView},
    intake_package::{self, IntakePackage, IntakePackageError},
    source_catalog::{self, CatalogError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IntakeGitPublicationError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("source unavailable: {0}")]
    Source(#[from] CatalogError),
    #[error("receipt package unavailable: {0}")]
    Package(#[from] IntakePackageError),
    #[error("git target unavailable: {0}")]
    Git(#[from] GitPublicationError),
    #[error("invalid intake publication request")]
    Invalid,
    #[error("intake publication identity conflicts with recorded scope")]
    Conflict,
    #[error("intake publication intent unavailable")]
    Invisible,
    #[error("intake publication intent no longer matches the immutable receipt")]
    Stale,
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakePublicationPreview {
    pub target: TargetView,
    pub workspace_id: Uuid,
    pub document_id: Uuid,
    pub source_sha256: String,
    pub source_format: String,
    pub package_sha256: String,
    pub file_count: u32,
    pub total_text_bytes: u64,
    pub requested_ref: String,
    pub scope_kind: String,
    pub includes_original_uploads: bool,
    pub includes_source_text: bool,
    pub includes_analysis: bool,
    pub remote_publish_performed: bool,
    pub requires_transient_provider_authorization: bool,
    pub ready_for_remote_publish: bool,
    pub establishes_authenticity: bool,
    pub establishes_truth: bool,
    pub proves_completeness: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakePublicationIntentView {
    pub publication_id: Uuid,
    pub target_id: Uuid,
    pub document_id: Uuid,
    pub source_sha256: String,
    pub package_sha256: String,
    pub file_count: u32,
    pub total_text_bytes: u64,
    pub requested_ref: String,
    pub scope_kind: String,
    pub includes_original_uploads: bool,
    pub includes_source_text: bool,
    pub includes_analysis: bool,
    pub publication_state: String,
    pub requested_by_principal_id: Uuid,
    pub created_at: String,
    pub remote_publish_performed: bool,
}

#[derive(Clone, Debug)]
pub struct IntakePublicationMaterial {
    pub target: TargetView,
    pub intent: IntakePublicationIntentView,
    pub package: IntakePackage,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, IntakeGitPublicationError> {
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(IntakeGitPublicationError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn package_shape(package: &IntakePackage) -> Result<(u32, u64), IntakeGitPublicationError> {
    let file_count =
        u32::try_from(package.files.len()).map_err(|_| IntakeGitPublicationError::Invalid)?;
    let total_text_bytes = package.files.values().try_fold(0_u64, |total, value| {
        total
            .checked_add(value.len() as u64)
            .ok_or(IntakeGitPublicationError::Invalid)
    })?;
    if file_count == 0 || file_count > 16 || total_text_bytes == 0 || total_text_bytes > 65_536 {
        return Err(IntakeGitPublicationError::Invalid);
    }
    Ok((file_count, total_text_bytes))
}

async fn current_package(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
) -> Result<(source_catalog::SourceMetadata, IntakePackage), IntakeGitPublicationError> {
    let source = source_catalog::get_source(pool, workspace, document).await?;
    let package = intake_package::build(workspace, &source)?;
    Ok((source, package))
}

pub async fn preview(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
    target_id: Uuid,
) -> Result<IntakePublicationPreview, IntakeGitPublicationError> {
    if workspace.is_nil() || document.is_nil() || target_id.is_nil() {
        return Err(IntakeGitPublicationError::Invalid);
    }
    let (source, package) = current_package(pool, workspace, document).await?;
    let target = git_publication::get_target(pool, workspace, target_id).await?;
    if target.repository_visibility != "PRIVATE" {
        return Err(IntakeGitPublicationError::Invalid);
    }
    let (file_count, total_text_bytes) = package_shape(&package)?;
    Ok(IntakePublicationPreview {
        requested_ref: target.ref_name.clone(),
        target,
        workspace_id: workspace,
        document_id: document,
        source_sha256: source.sha256,
        source_format: source.format,
        package_sha256: intake_package::package_sha256(&package),
        file_count,
        total_text_bytes,
        scope_kind: "INTAKE_RECEIPT".into(),
        includes_original_uploads: false,
        includes_source_text: false,
        includes_analysis: false,
        remote_publish_performed: false,
        requires_transient_provider_authorization: true,
        ready_for_remote_publish: false,
        establishes_authenticity: false,
        establishes_truth: false,
        proves_completeness: false,
    })
}

struct IntentRow {
    publication_id: Uuid,
    target_id: Uuid,
    document_id: Uuid,
    source_sha256: String,
    package_sha256: String,
    file_count: i32,
    total_text_bytes: i64,
    requested_ref: String,
    scope_kind: String,
    includes_original_uploads: bool,
    includes_source_text: bool,
    includes_analysis: bool,
    publication_state: String,
    requested_by_principal_id: Uuid,
    created_at: String,
}

fn decode_intent_row(row: PgRow) -> Result<IntentRow, sqlx::Error> {
    Ok(IntentRow {
        publication_id: row.try_get("publication_id")?,
        target_id: row.try_get("target_id")?,
        document_id: row.try_get("document_id")?,
        source_sha256: row.try_get("source_sha256")?,
        package_sha256: row.try_get("package_sha256")?,
        file_count: row.try_get("file_count")?,
        total_text_bytes: row.try_get("total_text_bytes")?,
        requested_ref: row.try_get("requested_ref")?,
        scope_kind: row.try_get("scope_kind")?,
        includes_original_uploads: row.try_get("includes_original_uploads")?,
        includes_source_text: row.try_get("includes_source_text")?,
        includes_analysis: row.try_get("includes_analysis")?,
        publication_state: row.try_get("publication_state")?,
        requested_by_principal_id: row.try_get("requested_by_principal_id")?,
        created_at: row.try_get("created_at")?,
    })
}

fn intent_from_row(
    row: IntentRow,
) -> Result<IntakePublicationIntentView, IntakeGitPublicationError> {
    Ok(IntakePublicationIntentView {
        publication_id: row.publication_id,
        target_id: row.target_id,
        document_id: row.document_id,
        source_sha256: row.source_sha256,
        package_sha256: row.package_sha256,
        file_count: u32::try_from(row.file_count).map_err(|_| IntakeGitPublicationError::Conflict)?,
        total_text_bytes: u64::try_from(row.total_text_bytes)
            .map_err(|_| IntakeGitPublicationError::Conflict)?,
        requested_ref: row.requested_ref,
        scope_kind: row.scope_kind,
        includes_original_uploads: row.includes_original_uploads,
        includes_source_text: row.includes_source_text,
        includes_analysis: row.includes_analysis,
        publication_state: row.publication_state,
        requested_by_principal_id: row.requested_by_principal_id,
        created_at: row.created_at,
        remote_publish_performed: false,
    })
}

pub async fn record_intent(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
    actor: Uuid,
    publication_id: Uuid,
    target_id: Uuid,
) -> Result<(IntakePublicationIntentView, bool), IntakeGitPublicationError> {
    if actor.is_nil() || publication_id.is_nil() {
        return Err(IntakeGitPublicationError::Invalid);
    }
    let preview = preview(pool, workspace, document, target_id).await?;
    let mut tx = scoped(pool, workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.intake_git_publication_intents
         (workspace_id,publication_id,target_id,document_id,source_sha256,package_sha256,
          file_count,total_text_bytes,requested_ref,requested_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(publication_id)
    .bind(target_id)
    .bind(document)
    .bind(&preview.source_sha256)
    .bind(&preview.package_sha256)
    .bind(i32::try_from(preview.file_count).map_err(|_| IntakeGitPublicationError::Invalid)?)
    .bind(i64::try_from(preview.total_text_bytes).map_err(|_| IntakeGitPublicationError::Invalid)?)
    .bind(&preview.requested_ref)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row = sqlx::query(
        "SELECT publication_id,target_id,document_id,source_sha256,package_sha256,
                file_count,total_text_bytes,requested_ref,scope_kind,
                includes_original_uploads,includes_source_text,includes_analysis,
                publication_state,requested_by_principal_id,created_at::text AS created_at
           FROM niccolo.intake_git_publication_intents
          WHERE workspace_id=$1 AND publication_id=$2",
    )
    .bind(workspace)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(IntakeGitPublicationError::Conflict)?;
    let row = decode_intent_row(row)?;

    if row.target_id != target_id
        || row.document_id != document
        || row.source_sha256 != preview.source_sha256
        || row.package_sha256 != preview.package_sha256
        || row.file_count
            != i32::try_from(preview.file_count).map_err(|_| IntakeGitPublicationError::Invalid)?
        || row.total_text_bytes
            != i64::try_from(preview.total_text_bytes)
                .map_err(|_| IntakeGitPublicationError::Invalid)?
        || row.requested_ref != preview.requested_ref
        || row.scope_kind != "INTAKE_RECEIPT"
        || row.includes_original_uploads
        || row.includes_source_text
        || row.includes_analysis
        || row.publication_state != "AWAITING_REMOTE_AUTHORIZATION"
        || row.requested_by_principal_id != actor
    {
        return Err(IntakeGitPublicationError::Conflict);
    }
    let view = intent_from_row(row)?;
    tx.commit().await?;
    Ok((view, !inserted))
}

async fn load_intent(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
    publication_id: Uuid,
) -> Result<IntakePublicationIntentView, IntakeGitPublicationError> {
    let mut tx = scoped(pool, workspace).await?;
    let row = sqlx::query(
        "SELECT publication_id,target_id,document_id,source_sha256,package_sha256,
                file_count,total_text_bytes,requested_ref,scope_kind,
                includes_original_uploads,includes_source_text,includes_analysis,
                publication_state,requested_by_principal_id,created_at::text AS created_at
           FROM niccolo.intake_git_publication_intents
          WHERE workspace_id=$1 AND document_id=$2 AND publication_id=$3",
    )
    .bind(workspace)
    .bind(document)
    .bind(publication_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(IntakeGitPublicationError::Invisible)?;
    let row = decode_intent_row(row)?;
    tx.commit().await?;
    intent_from_row(row)
}

pub async fn publication_material(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
    publication_id: Uuid,
) -> Result<IntakePublicationMaterial, IntakeGitPublicationError> {
    if workspace.is_nil() || document.is_nil() || publication_id.is_nil() {
        return Err(IntakeGitPublicationError::Invalid);
    }
    let intent = load_intent(pool, workspace, document, publication_id).await?;
    let (source, package) = current_package(pool, workspace, document).await?;
    let target = git_publication::get_target(pool, workspace, intent.target_id).await?;
    let (file_count, total_text_bytes) = package_shape(&package)?;
    let digest = intake_package::package_sha256(&package);
    if target.repository_visibility != "PRIVATE"
        || intent.document_id != document
        || intent.source_sha256 != source.sha256
        || intent.package_sha256 != digest
        || intent.file_count != file_count
        || intent.total_text_bytes != total_text_bytes
        || intent.requested_ref != target.ref_name
        || intent.scope_kind != "INTAKE_RECEIPT"
        || intent.includes_original_uploads
        || intent.includes_source_text
        || intent.includes_analysis
        || intent.publication_state != "AWAITING_REMOTE_AUTHORIZATION"
    {
        return Err(IntakeGitPublicationError::Stale);
    }
    Ok(IntakePublicationMaterial {
        target,
        intent,
        package,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_shape_is_tightly_bounded() {
        let source = source_catalog::SourceMetadata {
            document_id: Uuid::from_u128(2),
            format: "EML".into(),
            locator: "fixture://private".into(),
            sha256: "a".repeat(64),
            byte_length: 42,
            fragment_count: 1,
            original_bytes_available: true,
        };
        let package = intake_package::build(Uuid::from_u128(1), &source).unwrap();
        let (files, bytes) = package_shape(&package).unwrap();
        assert_eq!(files as usize, package.files.len());
        assert!(bytes > 0);
        assert!(bytes <= 65_536);
    }
}
