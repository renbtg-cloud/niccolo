//! v0.37: deterministic user-portable analysis export.
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::analysis_manifest::{self, AnalysisBundle, ManifestError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableAnalysisExport {
    pub format: String,
    pub format_version: u32,
    pub bundle: AnalysisBundle,
    pub stored_manifest_sha256: Option<String>,
}

pub async fn build(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<PortableAnalysisExport, ManifestError> {
    let bundle = analysis_manifest::load_bundle(pool, workspace, situation, analysis).await?;
    let mut tx = pool.begin().await?;
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    let stored_manifest_sha256: Option<String> = sqlx::query_scalar(
        "SELECT manifest_sha256 FROM niccolo.situation_analysis_manifests \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(PortableAnalysisExport {
        format: "niccolo.analysis-export".to_string(),
        format_version: 1,
        bundle,
        stored_manifest_sha256,
    })
}

pub async fn to_json(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<String, ManifestError> {
    Ok(serde_json::to_string(
        &build(pool, workspace, situation, analysis).await?,
    )?)
}
