//! v0.38: recompute and compare immutable analysis manifests.
use sqlx::PgPool;
use uuid::Uuid;

use crate::analysis_manifest::{self, ManifestError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegrityStatus {
    Verified {
        sha256: String,
    },
    MissingManifest {
        recomputed_sha256: String,
    },
    Mismatch {
        stored_sha256: String,
        recomputed_sha256: String,
    },
}

pub async fn verify(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<IntegrityStatus, ManifestError> {
    let recomputed = analysis_manifest::compute_hash(pool, workspace, situation, analysis).await?;
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ManifestError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT manifest_sha256 FROM niccolo.situation_analysis_manifests \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(match stored {
        None => IntegrityStatus::MissingManifest {
            recomputed_sha256: recomputed,
        },
        Some(s) if s == recomputed => IntegrityStatus::Verified { sha256: s },
        Some(s) => IntegrityStatus::Mismatch {
            stored_sha256: s,
            recomputed_sha256: recomputed,
        },
    })
}
