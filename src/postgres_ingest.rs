//! Atomic source-fragment ingestion with byte-level provenance and idempotent retries.
//! No principal authentication is provided here. `workspace` MUST come from
//! an upstream trusted authorization decision, never from a client-supplied id.
use crate::source_import::{
    ParseError, ParsedSource, SourceFormat, parse_source, parse_source_for_replay,
};
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
    ingest_source_with_operational_id(pool, workspace, document_id, format, locator, raw, None)
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
    // Earlier parser rules can verify existing records but cannot admit a new document.
    let (parsed, parse_error) = match parse_source(format, raw) {
        Ok(parsed) => (parsed, None),
        Err(error) => match parse_source_for_replay(format, raw) {
            Ok(parsed) => (parsed, Some(error)),
            Err(_) => return Err(error.into()),
        },
    };
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
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(lock_key)
            .execute(&mut *tx)
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

    let inserted = if parse_error.is_none() {
        sqlx::query(
            "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length)
             VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT(workspace_id,document_id) DO NOTHING",
        )
        .bind(workspace)
        .bind(document_id)
        .bind(format.db_value())
        .bind(locator)
        .bind(&parsed.sha256)
        .bind(parsed.byte_length)
        .execute(&mut *tx)
        .await?
        .rows_affected() == 1
    } else {
        false
    };

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
            return Err(parse_error.map_or(ImportError::DocumentConflict, ImportError::Parse));
        }
        verify_replay(&mut tx, workspace, document_id, &parsed, format, raw).await?
    };

    // Metadata/fragments and the byte-exact original commit atomically.
    // Older documents may lack original bytes; a validated replay backfills
    // only that exact supplied artifact, never a reconstructed approximation.
    let original_exists = if inserted {
        false
    } else {
        match stored_original_matches(&mut tx, workspace, document_id, raw).await? {
            Some(true) => true,
            Some(false) => return Err(ImportError::DocumentConflict),
            None => false,
        }
    };
    if !original_exists {
        let payload_inserted = sqlx::query(
            "INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes)
             VALUES($1,$2,$3) ON CONFLICT(workspace_id,document_id) DO NOTHING",
        )
        .bind(workspace)
        .bind(document_id)
        .bind(raw)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !payload_inserted
            && stored_original_matches(&mut tx, workspace, document_id, raw).await? != Some(true)
        {
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

async fn stored_original_matches(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: Uuid,
    document_id: Uuid,
    raw: &[u8],
) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT original_bytes = $3 FROM niccolo.source_payloads
         WHERE workspace_id=$1 AND document_id=$2",
    )
    .bind(workspace)
    .bind(document_id)
    .bind(raw)
    .fetch_optional(&mut **tx)
    .await
}

async fn write_fragments(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: Uuid,
    document_id: Uuid,
    locator: &str,
    parsed: &ParsedSource,
) -> Result<Vec<Uuid>, ImportError> {
    let count = parsed.fragments.len();
    let mut ids = Vec::with_capacity(count);
    let mut locators = Vec::with_capacity(count);
    let mut contents = Vec::with_capacity(count);
    let mut ordinals = Vec::with_capacity(count);
    let mut starts = Vec::with_capacity(count);
    let mut ends = Vec::with_capacity(count);
    let mut speakers = Vec::with_capacity(count);
    for fragment in &parsed.fragments {
        ids.push(Uuid::new_v4());
        locators.push(format!(
            "{locator}#bytes={}-{}",
            fragment.byte_start, fragment.byte_end
        ));
        contents.push(fragment.content.as_str());
        ordinals.push(fragment.ordinal);
        starts.push(fragment.byte_start);
        ends.push(fragment.byte_end);
        speakers.push(fragment.speaker_label.as_str());
    }
    sqlx::query(
        "INSERT INTO niccolo.evidence
         (workspace_id,evidence_id,source_locator,content,source_class,admissible)
         SELECT $1, fragment.evidence_id, fragment.locator, fragment.content, 'REPORTED_SPEECH', true
         FROM unnest($2::uuid[], $3::text[], $4::text[]) AS fragment(evidence_id,locator,content)",
    )
    .bind(workspace)
    .bind(&ids)
    .bind(&locators)
    .bind(&contents)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO niccolo.source_fragments
         (workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label)
         SELECT $1, $2, fragment.ordinal, fragment.evidence_id, fragment.byte_start, fragment.byte_end, fragment.speaker
         FROM unnest($3::integer[], $4::uuid[], $5::integer[], $6::integer[], $7::text[])
              AS fragment(ordinal,evidence_id,byte_start,byte_end,speaker)",
    )
    .bind(workspace)
    .bind(document_id)
    .bind(&ordinals)
    .bind(&ids)
    .bind(&starts)
    .bind(&ends)
    .bind(&speakers)
    .execute(&mut **tx)
    .await?;
    Ok(ids)
}
async fn verify_replay(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: Uuid,
    document_id: Uuid,
    parsed: &ParsedSource,
    format: SourceFormat,
    raw: &[u8],
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
    let matches = |candidate: &ParsedSource| {
        stored.len() == candidate.fragments.len()
            && stored
                .iter()
                .zip(&candidate.fragments)
                .all(|(row, expected)| {
                    let (ordinal, _, start, end, speaker, content) = row;
                    (*ordinal, *start, *end, speaker.as_str(), content.as_str())
                        == (
                            expected.ordinal,
                            expected.byte_start,
                            expected.byte_end,
                            expected.speaker_label.as_str(),
                            expected.content.as_str(),
                        )
                })
    };
    if !matches(parsed)
        && !parse_source_for_replay(format, raw).is_ok_and(|legacy| matches(&legacy))
    {
        return Err(ImportError::DocumentConflict);
    }
    Ok(stored.into_iter().map(|(_, id, ..)| id).collect())
}
