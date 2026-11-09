//! Atomic source-fragment ingestion with byte-level provenance and idempotent retries.
//! No principal authentication is provided here. `workspace` MUST come from
//! an upstream trusted authorization decision, never from a client-supplied id.
use crate::source_import::{ParseError, ParsedSource, SourceFormat, parse_source};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("source parser rejected input: {0}")]
    Parse(#[from] ParseError),
    #[error("PostgreSQL error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("privileged database login cannot be used as an application identity")]
    PrivilegedRole,
    #[error("invalid source locator")]
    Locator,
    #[error("workspace is not visible to this database role")]
    WorkspaceInvisible,
    #[error("existing source document differs or previously stored fragments are inconsistent")]
    DocumentConflict,
    #[error("invalid operational event identity")]
    OperationalEventInvalid,
    #[error("operational event identity was reused for different content")]
    OperationalEventConflict,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImportResult {
    pub document_id: Uuid,
    pub sha256: String,
    pub fragment_ids: Vec<Uuid>,
    pub replayed: bool,
    pub graph_revision: i64,
    pub operational_event_id: Option<String>,
    pub operational_duplicate: bool,
    pub delivery_count: i64,
}

/// Byte-exact document, fragments and original payload write path. All changes commit or roll back
/// together; scoped connection state is automatically cleared at transaction end.
/// Source parsing remains deliberately bounded to supported text formats.
pub async fn ingest_source(
    pool: &PgPool,
    workspace: Uuid,
    document_id: Uuid,
    format: SourceFormat,
    locator: &str,
    raw: &[u8],
) -> Result<ImportResult, ImportError> {
    ingest_source_with_operational_id(
        pool,
        workspace,
        document_id,
        format,
        locator,
        raw,
        None,
    )
    .await
}

pub async fn ingest_source_with_operational_id(
    pool: &PgPool,
    workspace: Uuid,
    document_id: Uuid,
    format: SourceFormat,
    locator: &str,
    raw: &[u8],
    operational_event_id: Option<&str>,
) -> Result<ImportResult, ImportError> {
    if locator.is_empty() || locator.len() > 1024 || locator.chars().any(char::is_control) {
        return Err(ImportError::Locator);
    }
    let parsed = parse_source(format, raw)?;
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ImportError::PrivilegedRole);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id', $1, true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    // Lock the tenant row to serialize concurrent attempts at the same document.
    let visible: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1 FOR UPDATE",
    )
    .bind(workspace)
    .fetch_optional(&mut *tx)
    .await?;
    if visible.is_none() {
        return Err(ImportError::WorkspaceInvisible);
    }

    let operational_event_id = operational_event_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if let Some(value) = &operational_event_id {
        if value.len() > 512 || value.chars().any(char::is_control) {
            return Err(ImportError::OperationalEventInvalid);
        }
        let lock_key = format!("{workspace}:{value}");
        let _: i64 = sqlx::query_scalar(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        )
        .bind(lock_key)
        .fetch_one(&mut *tx)
        .await?;
        let prior: Option<(Uuid, String, i64, String)> = sqlx::query_as(
            "SELECT e.document_id,e.sha256,e.delivery_count,d.format
               FROM niccolo.source_operational_events e
               JOIN niccolo.source_documents d
                 ON d.workspace_id=e.workspace_id AND d.document_id=e.document_id
              WHERE e.workspace_id=$1 AND e.operational_event_id=$2
              FOR UPDATE OF e",
        )
        .bind(workspace)
        .bind(value)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((stored_document, stored_sha, delivery_count, stored_format)) = prior {
            if stored_sha != parsed.sha256 || stored_format != format.db_value() {
                return Err(ImportError::OperationalEventConflict);
            }
            let fragment_ids: Vec<Uuid> = sqlx::query_scalar(
                "SELECT evidence_id FROM niccolo.source_fragments
                  WHERE workspace_id=$1 AND document_id=$2 ORDER BY ordinal",
            )
            .bind(workspace)
            .bind(stored_document)
            .fetch_all(&mut *tx)
            .await?;
            let next_count = delivery_count + 1;
            sqlx::query(
                "UPDATE niccolo.source_operational_events
                    SET delivery_count=$3,last_seen_at=clock_timestamp()
                  WHERE workspace_id=$1 AND operational_event_id=$2",
            )
            .bind(workspace)
            .bind(value)
            .bind(next_count)
            .execute(&mut *tx)
            .await?;
            let graph_revision: i64 = sqlx::query_scalar(
                "SELECT revision FROM niccolo.graph_revisions WHERE workspace_id=$1",
            )
            .bind(workspace)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
            tx.commit().await?;
            return Ok(ImportResult {
                document_id: stored_document,
                sha256: parsed.sha256,
                fragment_ids,
                replayed: true,
                graph_revision,
                operational_event_id: Some(value.clone()),
                operational_duplicate: true,
                delivery_count: next_count,
            });
        }
    }

    let inserted=sqlx::query(
        "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) \
         VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT(workspace_id,document_id) DO NOTHING"
    )
      .bind(workspace).bind(document_id).bind(format.db_value()).bind(locator)
      .bind(&parsed.sha256).bind(parsed.byte_length)
      .execute(&mut *tx).await?.rows_affected()==1;

    let fragment_ids = if inserted {
        write_fragments(&mut tx, workspace, document_id, locator, &parsed).await?
    } else {
        let meta: Option<(String, String, String, i64)> = sqlx::query_as(
            "SELECT format,locator,sha256,byte_length FROM niccolo.source_documents \
             WHERE workspace_id=$1 AND document_id=$2",
        )
        .bind(workspace)
        .bind(document_id)
        .fetch_optional(&mut *tx)
        .await?;
        if meta.as_ref().is_none_or(|(f, l, h, size)| {
            f != format.db_value()
                || l != locator
                || h != &parsed.sha256
                || *size != parsed.byte_length
        }) {
            return Err(ImportError::DocumentConflict);
        }
        verify_replay(&mut tx, workspace, document_id, &parsed).await?
    };

    // Metadata/fragments and the byte-exact original commit atomically.
    // Older documents may lack original bytes; a validated replay backfills
    // only that exact supplied artifact, never a reconstructed approximation.
    let payload_inserted = sqlx::query(
        "INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) \
         VALUES($1,$2,$3) ON CONFLICT(workspace_id,document_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(document_id)
    .bind(raw)
    .execute(&mut *tx)
    .await?
    .rows_affected() == 1;
    if !payload_inserted {
        let original: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT original_bytes FROM niccolo.source_payloads \
             WHERE workspace_id=$1 AND document_id=$2",
        )
        .bind(workspace)
        .bind(document_id)
        .fetch_optional(&mut *tx)
        .await?;
        if original.as_deref() != Some(raw) {
            return Err(ImportError::DocumentConflict);
        }
    }

    let graph_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM niccolo.graph_revisions WHERE workspace_id=$1")
            .bind(workspace)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
    if let Some(value) = &operational_event_id {
        sqlx::query(
            "INSERT INTO niccolo.source_operational_events
             (workspace_id,operational_event_id,document_id,sha256)
             VALUES($1,$2,$3,$4)",
        )
        .bind(workspace)
        .bind(value)
        .bind(document_id)
        .bind(&parsed.sha256)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(ImportResult {
        document_id,
        sha256: parsed.sha256,
        fragment_ids,
        replayed: !inserted,
        graph_revision,
        operational_event_id,
        operational_duplicate: false,
        delivery_count: 1,
    })
}

async fn write_fragments(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: Uuid,
    document_id: Uuid,
    locator: &str,
    parsed: &ParsedSource,
) -> Result<Vec<Uuid>, ImportError> {
    let mut ids = Vec::with_capacity(parsed.fragments.len());
    for f in &parsed.fragments {
        let evidence_id = Uuid::new_v4();
        let evidence_locator = format!("{locator}#bytes={}-{}", f.byte_start, f.byte_end);
        sqlx::query(
            "INSERT INTO niccolo.evidence \
             (workspace_id,evidence_id,source_locator,content,source_class,admissible) \
             VALUES ($1,$2,$3,$4,'REPORTED_SPEECH',true)",
        )
        .bind(workspace)
        .bind(evidence_id)
        .bind(evidence_locator)
        .bind(&f.content)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO niccolo.source_fragments \
             (workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(workspace)
        .bind(document_id)
        .bind(f.ordinal)
        .bind(evidence_id)
        .bind(f.byte_start)
        .bind(f.byte_end)
        .bind(&f.speaker_label)
        .execute(&mut **tx)
        .await?;
        ids.push(evidence_id);
    }
    Ok(ids)
}
async fn verify_replay(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: Uuid,
    document_id: Uuid,
    parsed: &ParsedSource,
) -> Result<Vec<Uuid>, ImportError> {
    let stored: Vec<(i32, Uuid, i32, i32, String, String)> = sqlx::query_as(
        "SELECT f.ordinal,f.evidence_id,f.byte_start,f.byte_end,f.speaker_label,e.content \
         FROM niccolo.source_fragments f JOIN niccolo.evidence e ON \
         e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id \
         WHERE f.workspace_id=$1 AND f.document_id=$2 ORDER BY f.ordinal",
    )
    .bind(workspace)
    .bind(document_id)
    .fetch_all(&mut **tx)
    .await?;
    if stored.len() != parsed.fragments.len() {
        return Err(ImportError::DocumentConflict);
    }
    let mut ids = Vec::with_capacity(stored.len());
    for ((ordinal, id, start, end, speaker, content), expected) in
        stored.into_iter().zip(&parsed.fragments)
    {
        if (ordinal, start, end, speaker.as_str(), content.as_str())
            != (
                expected.ordinal,
                expected.byte_start,
                expected.byte_end,
                expected.speaker_label.as_str(),
                expected.content.as_str(),
            )
        {
            return Err(ImportError::DocumentConflict);
        }
        ids.push(id);
    }
    Ok(ids)
}
