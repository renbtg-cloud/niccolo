//! Append-only attributed declarations about processing performed upstream of Niccolò.
//!
//! A declaration records what an integration principal says happened. It is not
//! verification that the declared processing was correct, complete, unbiased,
//! exhaustive, or faithfully executed, and it is not evidence about the case.
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const MAX_PAGE: i64 = 100;

#[derive(Debug, thiserror::Error)]
pub enum UpstreamProcessingError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid upstream processing declaration")]
    Invalid,
    #[error("referenced source document is not visible")]
    InvisibleDocument,
    #[error("declaration identifier conflicts with stored state")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProcessingDeclaration {
    pub declaration_id: Uuid,
    pub event_no: i64,
    pub processing_kind: String,
    pub scope_locator: String,
    pub declaration_text: String,
    pub source_document_id: Option<Uuid>,
    pub recorded_by: Uuid,
    pub replayed: bool,
    pub declaration_is_verified: bool,
    pub declaration_is_evidence: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewProcessingDeclaration<'a> {
    pub processing_kind: &'a str,
    pub scope_locator: &'a str,
    pub declaration_text: &'a str,
    pub source_document_id: Option<Uuid>,
}

fn clean_kind(value: &str) -> Result<String, UpstreamProcessingError> {
    match value.trim() {
        "FILTER" | "DEDUPLICATION" | "SAMPLING" | "SOURCE_SELECTION" | "EXCLUSION"
        | "TRANSFORMATION" | "AGGREGATION" => Ok(value.trim().to_owned()),
        _ => Err(UpstreamProcessingError::Invalid),
    }
}

fn clean_bounded(
    value: &str,
    max: usize,
    reject_controls: bool,
) -> Result<String, UpstreamProcessingError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max
        || (reject_controls && value.chars().any(char::is_control))
    {
        return Err(UpstreamProcessingError::Invalid);
    }
    Ok(value.to_owned())
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, UpstreamProcessingError> {
    let mut tx = pool.begin().await?;
    if read_only {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(UpstreamProcessingError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn view(
    row: (Uuid, i64, String, String, String, Option<Uuid>, Uuid),
    replayed: bool,
) -> ProcessingDeclaration {
    ProcessingDeclaration {
        declaration_id: row.0,
        event_no: row.1,
        processing_kind: row.2,
        scope_locator: row.3,
        declaration_text: row.4,
        source_document_id: row.5,
        recorded_by: row.6,
        replayed,
        declaration_is_verified: false,
        declaration_is_evidence: false,
    }
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    declaration_id: Uuid,
    actor: Uuid,
    input: NewProcessingDeclaration<'_>,
) -> Result<ProcessingDeclaration, UpstreamProcessingError> {
    if declaration_id.is_nil() {
        return Err(UpstreamProcessingError::Invalid);
    }
    let processing_kind = clean_kind(input.processing_kind)?;
    let scope_locator = clean_bounded(input.scope_locator, 1024, true)?;
    let declaration_text = clean_bounded(input.declaration_text, 4096, false)?;
    let mut tx = scoped(pool, workspace, false).await?;

    if let Some(document_id) = input.source_document_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM niccolo.source_documents
                WHERE workspace_id=$1 AND document_id=$2
             )",
        )
        .bind(workspace)
        .bind(document_id)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(UpstreamProcessingError::InvisibleDocument);
        }
    }

    type Row = (Uuid, i64, String, String, String, Option<Uuid>, Uuid);
    let stored: Option<Row> = sqlx::query_as(
        "SELECT declaration_id,event_no,processing_kind,scope_locator,declaration_text,
                source_document_id,recorded_by_principal_id
           FROM niccolo.upstream_processing_declarations
          WHERE workspace_id=$1 AND declaration_id=$2",
    )
    .bind(workspace)
    .bind(declaration_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(row) = stored {
        if row.2 != processing_kind
            || row.3 != scope_locator
            || row.4 != declaration_text
            || row.5 != input.source_document_id
            || row.6 != actor
        {
            return Err(UpstreamProcessingError::Conflict);
        }
        tx.commit().await?;
        return Ok(view(row, true));
    }

    let row: Row = sqlx::query_as(
        "INSERT INTO niccolo.upstream_processing_declarations
         (workspace_id,declaration_id,processing_kind,scope_locator,declaration_text,
          source_document_id,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7)
         RETURNING declaration_id,event_no,processing_kind,scope_locator,declaration_text,
                   source_document_id,recorded_by_principal_id",
    )
    .bind(workspace)
    .bind(declaration_id)
    .bind(processing_kind)
    .bind(scope_locator)
    .bind(declaration_text)
    .bind(input.source_document_id)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(view(row, false))
}

pub async fn history(
    pool: &PgPool,
    workspace: Uuid,
    after: i64,
    limit: i64,
) -> Result<Vec<ProcessingDeclaration>, UpstreamProcessingError> {
    if after < 0 || !(1..=MAX_PAGE).contains(&limit) {
        return Err(UpstreamProcessingError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    type Row = (Uuid, i64, String, String, String, Option<Uuid>, Uuid);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT declaration_id,event_no,processing_kind,scope_locator,declaration_text,
                source_document_id,recorded_by_principal_id
           FROM niccolo.upstream_processing_declarations
          WHERE workspace_id=$1 AND event_no>$2
          ORDER BY event_no
          LIMIT $3",
    )
    .bind(workspace)
    .bind(after)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(|row| view(row, false)).collect())
}
