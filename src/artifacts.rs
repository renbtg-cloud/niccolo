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
}
