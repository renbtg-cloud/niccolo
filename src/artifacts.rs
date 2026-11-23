//! Parser-free MVP artifact registry.
//!
//! These operations record external identities and explicit human epistemic
//! material. They never dereference a locator, accept source bytes, or invoke a
//! modality parser.
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid artifact input")]
    Invalid,
    #[error("artifact replay conflicts with the stored record")]
    Conflict,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ArtifactError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ArtifactError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn one_line(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

type StoredArtifact = (
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    Uuid,
);
type ArtifactRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    Uuid,
    String,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactInput {
    pub artifact_id: Uuid,
    pub artifact_type: String,
    pub label: String,
    pub declared_mime_type: Option<String>,
    pub declared_byte_length: Option<i64>,
    pub declared_sha256: Option<String>,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactView {
    pub artifact_id: Uuid,
    pub artifact_type: String,
    pub label: String,
    pub declared_mime_type: Option<String>,
    pub declared_byte_length: Option<i64>,
    pub declared_sha256: Option<String>,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub source_bytes_stored: bool,
    pub artifact_was_inspected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageReferenceInput {
    pub storage_reference_id: Uuid,
    pub access_event_id: Uuid,
    pub storage_type: String,
    pub locator: String,
    pub provider: Option<String>,
    pub environment_label: Option<String>,
    pub resolver_profile: Option<String>,
    pub notes: Option<String>,
    pub access_status: String,
    pub access_note: Option<String>,
    pub recorded_by: Uuid,
}

type StoredReference = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Uuid,
);
type StoredAccessEvent = (String, String, Option<String>, Uuid);
type StoredRepresentation = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Uuid,
);
type StoredRelationship = (Uuid, Uuid, String, Option<String>, Uuid);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationInput {
    pub representation_id: Uuid,
    pub representation_type: String,
    pub producer_type: String,
    pub producer_label: Option<String>,
    pub production_method: Option<String>,
    pub scope_locator: String,
    pub content: String,
    pub completeness_status: String,
    pub epistemic_qualification: Option<String>,
    pub limitations: Option<String>,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRelationshipInput {
    pub relationship_id: Uuid,
    pub subject_artifact_id: Uuid,
    pub object_artifact_id: Uuid,
    pub relationship_type: String,
    pub qualification: Option<String>,
    pub recorded_by: Uuid,
}

fn valid_artifact(input: &ArtifactInput) -> bool {
    one_line(&input.artifact_type, 128)
        && one_line(&input.label, 1024)
        && input
            .declared_mime_type
            .as_deref()
            .is_none_or(|value| one_line(value, 255))
        && input.declared_byte_length.is_none_or(|value| value >= 0)
        && input.declared_sha256.as_deref().is_none_or(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

fn valid_optional(value: Option<&str>, max: usize, one_line_only: bool) -> bool {
    value.is_none_or(|value| {
        !value.trim().is_empty()
            && value.len() <= max
            && (!one_line_only || !value.chars().any(char::is_control))
    })
}

fn contains_durable_credential(locator: &str) -> bool {
    let lower = locator.to_ascii_lowercase();
    let Some(query) = lower.split_once('?').map(|(_, query)| query) else {
        return false;
    };
    query.split('&').any(|parameter| {
        let key = parameter.split_once('=').map_or(parameter, |(key, _)| key);
        matches!(
            key,
            "x-amz-signature"
                | "x-amz-credential"
                | "x-goog-signature"
                | "signature"
                | "sig"
                | "token"
                | "access_token"
                | "credential"
        )
    })
}

fn valid_storage_reference(input: &StorageReferenceInput) -> bool {
    one_line(&input.storage_type, 64)
        && one_line(&input.locator, 4096)
        && !contains_durable_credential(&input.locator)
        && valid_optional(input.provider.as_deref(), 256, true)
        && valid_optional(input.environment_label.as_deref(), 256, true)
        && valid_optional(input.resolver_profile.as_deref(), 256, true)
        && valid_optional(input.notes.as_deref(), 4096, false)
        && matches!(
            input.access_status.as_str(),
            "UNKNOWN"
                | "NONE"
                | "AVAILABLE"
                | "TEMPORARILY_UNAVAILABLE"
                | "ACCESS_DENIED"
                | "NOT_FOUND"
                | "UNVERIFIED"
        )
        && valid_optional(input.access_note.as_deref(), 4096, false)
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.contains('\0')
}

fn valid_representation(input: &RepresentationInput) -> bool {
    one_line(&input.representation_type, 128)
        && matches!(
            input.producer_type.as_str(),
            "USER" | "THIRD_PARTY_HUMAN" | "EXTERNAL_SYSTEM" | "UNKNOWN"
        )
        && valid_optional(input.producer_label.as_deref(), 512, true)
        && valid_optional(input.production_method.as_deref(), 1024, true)
        && one_line(&input.scope_locator, 2048)
        && valid_text(&input.content, 65536)
        && matches!(
            input.completeness_status.as_str(),
            "COMPLETE" | "PARTIAL" | "UNKNOWN"
        )
        && input
            .epistemic_qualification
            .as_deref()
            .is_none_or(|value| valid_text(value, 4096))
        && input
            .limitations
            .as_deref()
            .is_none_or(|value| valid_text(value, 4096))
}

fn valid_relationship(input: &ArtifactRelationshipInput) -> bool {
    input.subject_artifact_id != input.object_artifact_id
        && one_line(&input.relationship_type, 128)
        && input
            .qualification
            .as_deref()
            .is_none_or(|value| valid_text(value, 4096))
}

/// Record an external artifact identity and attach it to a Situation atomically.
/// Returns true for an exact idempotent replay.
pub async fn record_for_situation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    input: &ArtifactInput,
) -> Result<bool, ArtifactError> {
    if !valid_artifact(input) {
        return Err(ArtifactError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.artifacts
         (workspace_id,artifact_id,artifact_type,label,declared_mime_type,
          declared_byte_length,declared_sha256,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT(workspace_id,artifact_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(input.artifact_id)
    .bind(&input.artifact_type)
    .bind(&input.label)
    .bind(&input.declared_mime_type)
    .bind(input.declared_byte_length)
    .bind(&input.declared_sha256)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let stored: Option<StoredArtifact> = sqlx::query_as(
            "SELECT artifact_type,label,declared_mime_type,declared_byte_length,
                        declared_sha256,recorded_by_principal_id
                 FROM niccolo.artifacts WHERE workspace_id=$1 AND artifact_id=$2",
        )
        .bind(workspace)
        .bind(input.artifact_id)
        .fetch_optional(&mut *tx)
        .await?;
        let expected = (
            input.artifact_type.clone(),
            input.label.clone(),
            input.declared_mime_type.clone(),
            input.declared_byte_length,
            input.declared_sha256.clone(),
            input.recorded_by,
        );
        if stored != Some(expected) {
            return Err(ArtifactError::Conflict);
        }
    }
    let attached = sqlx::query(
        "INSERT INTO niccolo.situation_artifacts
         (workspace_id,situation_id,artifact_id,added_by_principal_id)
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(input.artifact_id)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !attached {
        let actor: Option<Uuid> = sqlx::query_scalar(
            "SELECT added_by_principal_id FROM niccolo.situation_artifacts
             WHERE workspace_id=$1 AND situation_id=$2 AND artifact_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(input.artifact_id)
        .fetch_optional(&mut *tx)
        .await?;
        if actor != Some(input.recorded_by) {
            return Err(ArtifactError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(!inserted && !attached)
}

pub async fn list_for_situation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<ArtifactView>, ArtifactError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<ArtifactRow> = sqlx::query_as(
        "SELECT a.artifact_id,a.artifact_type,a.label,a.declared_mime_type,
                a.declared_byte_length,a.declared_sha256,a.recorded_by_principal_id,
                a.recorded_at::text
         FROM niccolo.situation_artifacts sa
         JOIN niccolo.artifacts a USING(workspace_id,artifact_id)
         WHERE sa.workspace_id=$1 AND sa.situation_id=$2
         ORDER BY sa.added_at,a.artifact_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                artifact_id,
                artifact_type,
                label,
                declared_mime_type,
                declared_byte_length,
                declared_sha256,
                recorded_by,
                recorded_at,
            )| ArtifactView {
                artifact_id,
                artifact_type,
                label,
                declared_mime_type,
                declared_byte_length,
                declared_sha256,
                recorded_by,
                recorded_at,
                source_bytes_stored: false,
                artifact_was_inspected: false,
            },
        )
        .collect())
}

/// Add a location declaration and its initial human-declared access state.
/// Locators are persisted as protected metadata and are never dereferenced.
/// Returns true for an exact idempotent replay.
pub async fn record_storage_reference(
    pool: &PgPool,
    workspace: Uuid,
    artifact: Uuid,
    input: &StorageReferenceInput,
) -> Result<bool, ArtifactError> {
    if !valid_storage_reference(input) {
        return Err(ArtifactError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.artifact_storage_references
         (workspace_id,artifact_id,storage_reference_id,storage_type,locator,
          provider,environment_label,resolver_profile,notes,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT(workspace_id,artifact_id,storage_reference_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(artifact)
    .bind(input.storage_reference_id)
    .bind(&input.storage_type)
    .bind(&input.locator)
    .bind(&input.provider)
    .bind(&input.environment_label)
    .bind(&input.resolver_profile)
    .bind(&input.notes)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let stored: Option<StoredReference> = sqlx::query_as(
            "SELECT storage_type,locator,provider,environment_label,resolver_profile,notes,
                    recorded_by_principal_id
             FROM niccolo.artifact_storage_references
             WHERE workspace_id=$1 AND artifact_id=$2 AND storage_reference_id=$3",
        )
        .bind(workspace)
        .bind(artifact)
        .bind(input.storage_reference_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored
            != Some((
                input.storage_type.clone(),
                input.locator.clone(),
                input.provider.clone(),
                input.environment_label.clone(),
                input.resolver_profile.clone(),
                input.notes.clone(),
                input.recorded_by,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    let event_inserted = sqlx::query(
        "INSERT INTO niccolo.artifact_storage_access_events
         (workspace_id,artifact_id,storage_reference_id,access_event_id,access_status,
          status_basis,note,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,'HUMAN_DECLARATION',$6,$7)
         ON CONFLICT(workspace_id,artifact_id,storage_reference_id,access_event_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(artifact)
    .bind(input.storage_reference_id)
    .bind(input.access_event_id)
    .bind(&input.access_status)
    .bind(&input.access_note)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !event_inserted {
        let stored: Option<StoredAccessEvent> = sqlx::query_as(
            "SELECT access_status,status_basis,note,recorded_by_principal_id
             FROM niccolo.artifact_storage_access_events
             WHERE workspace_id=$1 AND artifact_id=$2 AND storage_reference_id=$3
               AND access_event_id=$4",
        )
        .bind(workspace)
        .bind(artifact)
        .bind(input.storage_reference_id)
        .bind(input.access_event_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored
            != Some((
                input.access_status.clone(),
                "HUMAN_DECLARATION".into(),
                input.access_note.clone(),
                input.recorded_by,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(!inserted && !event_inserted)
}

/// Persist text explicitly supplied as an attributed representation of an
/// attached artifact. This operation does not derive the text from the artifact.
/// Returns true for an exact idempotent replay.
pub async fn record_representation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    artifact: Uuid,
    input: &RepresentationInput,
) -> Result<bool, ArtifactError> {
    if !valid_representation(input) {
        return Err(ArtifactError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_artifact_representations
         (workspace_id,situation_id,artifact_id,representation_id,representation_type,
          producer_type,producer_label,production_method,scope_locator,content,
          completeness_status,epistemic_qualification,limitations,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT(workspace_id,situation_id,representation_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(artifact)
    .bind(input.representation_id)
    .bind(&input.representation_type)
    .bind(&input.producer_type)
    .bind(&input.producer_label)
    .bind(&input.production_method)
    .bind(&input.scope_locator)
    .bind(&input.content)
    .bind(&input.completeness_status)
    .bind(&input.epistemic_qualification)
    .bind(&input.limitations)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let stored: Option<StoredRepresentation> = sqlx::query_as(
            "SELECT artifact_id,representation_type,producer_type,producer_label,
                    production_method,scope_locator,content,completeness_status,
                    epistemic_qualification,limitations,recorded_by_principal_id
             FROM niccolo.situation_artifact_representations
             WHERE workspace_id=$1 AND situation_id=$2 AND representation_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(input.representation_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored
            != Some((
                artifact,
                input.representation_type.clone(),
                input.producer_type.clone(),
                input.producer_label.clone(),
                input.production_method.clone(),
                input.scope_locator.clone(),
                input.content.clone(),
                input.completeness_status.clone(),
                input.epistemic_qualification.clone(),
                input.limitations.clone(),
                input.recorded_by,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(!inserted)
}

/// Record a qualified relationship asserted between two artifacts already
/// attached to the same Situation. Returns true for an exact replay.
pub async fn record_relationship(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    input: &ArtifactRelationshipInput,
) -> Result<bool, ArtifactError> {
    if !valid_relationship(input) {
        return Err(ArtifactError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_artifact_relationships
         (workspace_id,situation_id,relationship_id,subject_artifact_id,
          object_artifact_id,relationship_type,qualification,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT(workspace_id,situation_id,relationship_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(input.relationship_id)
    .bind(input.subject_artifact_id)
    .bind(input.object_artifact_id)
    .bind(&input.relationship_type)
    .bind(&input.qualification)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let stored: Option<StoredRelationship> = sqlx::query_as(
            "SELECT subject_artifact_id,object_artifact_id,relationship_type,
                    qualification,recorded_by_principal_id
             FROM niccolo.situation_artifact_relationships
             WHERE workspace_id=$1 AND situation_id=$2 AND relationship_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(input.relationship_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored
            != Some((
                input.subject_artifact_id,
                input.object_artifact_id,
                input.relationship_type.clone(),
                input.qualification.clone(),
                input.recorded_by,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(!inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ArtifactInput {
        ArtifactInput {
            artifact_id: Uuid::nil(),
            artifact_type: "VIDEO".into(),
            label: "meeting.mp4".into(),
            declared_mime_type: Some("video/mp4".into()),
            declared_byte_length: Some(18_000_000_000),
            declared_sha256: Some("a".repeat(64)),
            recorded_by: Uuid::nil(),
        }
    }

    #[test]
    fn artifact_metadata_is_bounded_but_not_bounded_by_file_size() {
        assert!(valid_artifact(&sample()));
        let mut invalid = sample();
        invalid.label = "bad\nlabel".into();
        assert!(!valid_artifact(&invalid));
        invalid = sample();
        invalid.declared_sha256 = Some("A".repeat(64));
        assert!(!valid_artifact(&invalid));
    }

    #[test]
    fn durable_credentials_are_not_artifact_metadata() {
        let base = StorageReferenceInput {
            storage_reference_id: Uuid::nil(),
            access_event_id: Uuid::nil(),
            storage_type: "S3".into(),
            locator: "s3://client/case/video.mp4".into(),
            provider: Some("AWS".into()),
            environment_label: Some("client legal".into()),
            resolver_profile: Some("client-legal-s3-readonly".into()),
            notes: None,
            access_status: "NONE".into(),
            access_note: Some("Niccolò has no credentials".into()),
            recorded_by: Uuid::nil(),
        };
        assert!(valid_storage_reference(&base));
        let mut signed = base.clone();
        signed.locator = "https://example.invalid/x?X-Amz-Signature=secret".into();
        assert!(!valid_storage_reference(&signed));
    }

    #[test]
    fn human_representation_preserves_multiline_text_and_attribution() {
        let input = RepresentationInput {
            representation_id: Uuid::nil(),
            representation_type: "HUMAN_TRANSCRIPTION".into(),
            producer_type: "USER".into(),
            producer_label: Some("Renato".into()),
            production_method: Some("typed while viewing the external artifact".into()),
            scope_locator: "pages 7-9".into(),
            content: "First line\nSecond line".into(),
            completeness_status: "PARTIAL".into(),
            epistemic_qualification: Some("not independently verified".into()),
            limitations: Some("figures omitted".into()),
            recorded_by: Uuid::nil(),
        };
        assert!(valid_representation(&input));
        let mut invalid = input.clone();
        invalid.scope_locator = "page 7\npage 8".into();
        assert!(!valid_representation(&invalid));
    }
}
