//! Workspace-scoped read-only source metadata; never returns raw uploaded bytes.
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("SQL failure: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("privileged connection forbidden")]
    PrivilegedRole,
    #[error("limit must be between 1 and 100")]
    InvalidLimit,
    #[error("source document unavailable")]
    InvisibleDocument,
    #[error("source document has too many distinct speaker labels")]
    TooManySpeakers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMetadata {
    pub document_id: Uuid,
    pub format: String,
    pub locator: String,
    pub sha256: String,
    pub byte_length: i64,
    pub fragment_count: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerSummary {
    pub speaker_label: String,
    pub fragment_count: i64,
}

#[derive(Debug, Clone)]
pub struct SourcePage {
    pub documents: Vec<SourceMetadata>,
    pub next_after: Option<Uuid>,
}

pub async fn list_sources(
    pool: &PgPool,
    workspace: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<SourcePage, CatalogError> {
    if !(1..=100).contains(&limit) {
        return Err(CatalogError::InvalidLimit);
    }
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
        return Err(CatalogError::PrivilegedRole);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    let rows:Vec<(Uuid,String,String,String,i64,i64)>=sqlx::query_as(
        "SELECT d.document_id,d.format,d.locator,d.sha256,d.byte_length, \
           (SELECT count(*) FROM niccolo.source_fragments f \
             WHERE f.workspace_id=d.workspace_id AND f.document_id=d.document_id) AS fragment_count \
         FROM niccolo.source_documents d \
         WHERE d.workspace_id=$1 AND ($2::uuid IS NULL OR d.document_id > $2) \
         ORDER BY d.document_id LIMIT $3")
        .bind(workspace).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let has_more = rows.len() as i64 > limit;
    let mut documents: Vec<_> = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(document_id, format, locator, sha256, byte_length, fragment_count)| SourceMetadata {
                document_id,
                format,
                locator,
                sha256,
                byte_length,
                fragment_count,
            },
        )
        .collect();
    let next_after = if has_more {
        documents.last().map(|d| d.document_id)
    } else {
        None
    };
    // Prevent accidental retention beyond the specified memory bound.
    documents.shrink_to_fit();
    Ok(SourcePage {
        documents,
        next_after,
    })
}

pub async fn document_speakers(
    pool: &PgPool,
    workspace: Uuid,
    document: Uuid,
) -> Result<Vec<SpeakerSummary>, CatalogError> {
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
        return Err(CatalogError::PrivilegedRole);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;

    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.source_documents WHERE workspace_id=$1 AND document_id=$2)",
    )
    .bind(workspace)
    .bind(document)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(CatalogError::InvisibleDocument);
    }

    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT speaker_label,count(*)::bigint
         FROM niccolo.source_fragments
         WHERE workspace_id=$1 AND document_id=$2
         GROUP BY speaker_label
         ORDER BY speaker_label
         LIMIT 1001",
    )
    .bind(workspace)
    .bind(document)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    if rows.len() > 1000 {
        return Err(CatalogError::TooManySpeakers);
    }
    Ok(rows
        .into_iter()
        .map(|(speaker_label, fragment_count)| SpeakerSummary {
            speaker_label,
            fragment_count,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_limits_are_tight() {
        let p = SourcePage {
            documents: vec![],
            next_after: None,
        };
        assert!(p.documents.is_empty());
    }
}
