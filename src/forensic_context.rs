//! Human-selected access to quarantined forensic import material.
//!
//! A binding makes one imported historical item visible as current reasoning
//! context for one analysis. It never upgrades that item to native evidence,
//! truth, trusted configuration, or a verified identity assertion.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const MAX_BINDINGS_PER_ANALYSIS: i64 = 64;
const MAX_ITEM_JSON_BYTES: i64 = 64 * 1024;
const MAX_PACKET_JSON_BYTES: i64 = 256 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ForensicContextError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid forensic context binding")]
    Invalid,
    #[error("analysis or forensic item unavailable")]
    Invisible,
    #[error("forensic context binding conflicts with recorded provenance")]
    Conflict,
    #[error("forensic context binding limit reached")]
    TooMany,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingInput {
    pub binding_id: Uuid,
    pub import_batch_id: Uuid,
    pub item_id: Uuid,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicContextItem {
    pub binding_id: Uuid,
    pub import_batch_id: Uuid,
    pub item_id: Uuid,
    pub source_path: String,
    pub source_kind: String,
    pub source_sha256: String,
    pub origin_class: String,
    pub source_table: Option<String>,
    pub source_row_index: Option<u64>,
    pub original_document_id: Option<Uuid>,
    pub note: Option<String>,
    pub bound_by_principal_id: Uuid,
    pub payload_json: Option<Value>,
    pub payload_json_bytes: u64,
    pub payload_omitted_due_to_size: bool,
    pub original_bytes_retained: bool,
    pub quarantine_provenance: bool,
    pub establishes_truth: bool,
    pub documentary_evidence: bool,
    pub current_credentials_upgrade_imported_statement: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BindingResult {
    pub item: ForensicContextItem,
    pub replayed: bool,
}

type BindingRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<Uuid>,
    Option<String>,
    Uuid,
    Option<Value>,
    i64,
    bool,
);

fn clean_note(note: Option<&str>) -> Result<Option<String>, ForensicContextError> {
    let Some(note) = note else {
        return Ok(None);
    };
    let note = note.trim();
    if note.is_empty() || note.len() > 2048 || note.chars().any(char::is_control) {
        return Err(ForensicContextError::Invalid);
    }
    Ok(Some(note.to_owned()))
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ForensicContextError> {
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
        return Err(ForensicContextError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn view_from_row(row: BindingRow, mut remaining_bytes: i64) -> (ForensicContextItem, i64) {
    let payload_bytes = row.13.max(0);
    let candidate_payload = row.12;
    let had_payload = candidate_payload.is_some();
    let include_payload =
        had_payload && payload_bytes <= MAX_ITEM_JSON_BYTES && payload_bytes <= remaining_bytes;
    let payload_json = if include_payload {
        candidate_payload
    } else {
        None
    };
    if include_payload {
        remaining_bytes -= payload_bytes;
    }
    (
        ForensicContextItem {
            binding_id: row.0,
            import_batch_id: row.1,
            item_id: row.2,
            source_path: row.3,
            source_kind: row.4,
            source_sha256: row.5,
            origin_class: row.6,
            source_table: row.7,
            source_row_index: row.8.map(|value| value as u64),
            original_document_id: row.9,
            note: row.10,
            bound_by_principal_id: row.11,
            payload_json,
            payload_json_bytes: payload_bytes as u64,
            payload_omitted_due_to_size: had_payload && !include_payload,
            original_bytes_retained: row.14,
            quarantine_provenance: true,
            establishes_truth: false,
            documentary_evidence: false,
            current_credentials_upgrade_imported_statement: false,
        },
        remaining_bytes,
    )
}

async fn rows(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
) -> Result<Vec<BindingRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.binding_id,c.import_batch_id,c.item_id,
                i.source_path,i.source_kind,i.source_sha256,i.origin_class,
                i.source_table,i.source_row_index,i.original_document_id,
                c.note,c.bound_by_principal_id,
                CASE WHEN i.payload_json IS NOT NULL
                           AND octet_length(i.payload_json::text) <= $4
                     THEN i.payload_json ELSE NULL END,
                COALESCE(octet_length(i.payload_json::text),0)::bigint,
                i.payload_bytes IS NOT NULL
           FROM niccolo.analysis_forensic_context c
           JOIN niccolo.forensic_import_items i
             ON i.workspace_id=c.workspace_id
            AND i.import_batch_id=c.import_batch_id
            AND i.item_id=c.item_id
          WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3
          ORDER BY c.binding_id
          LIMIT $5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(MAX_ITEM_JSON_BYTES)
    .bind(MAX_BINDINGS_PER_ANALYSIS + 1)
    .fetch_all(&mut **tx)
    .await
}

async fn list_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
) -> Result<Vec<ForensicContextItem>, ForensicContextError> {
    let rows = rows(tx, scope).await?;
    if rows.len() as i64 > MAX_BINDINGS_PER_ANALYSIS {
        return Err(ForensicContextError::TooMany);
    }
    let mut remaining = MAX_PACKET_JSON_BYTES;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        let (view, next_remaining) = view_from_row(row, remaining);
        remaining = next_remaining;
        views.push(view);
    }
    Ok(views)
}

pub async fn list(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<ForensicContextItem>, ForensicContextError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM niccolo.situation_analysis_snapshots
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
         )",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ForensicContextError::Invisible);
    }
    let views = list_in_tx(&mut tx, scope).await?;
    tx.commit().await?;
    Ok(views)
}

pub async fn inherit_for_reanalysis(
    pool: &PgPool,
    parent: AnalysisScope,
    child: AnalysisScope,
) -> Result<u64, ForensicContextError> {
    if parent.workspace != child.workspace
        || parent.situation != child.situation
        || parent.analysis == child.analysis
    {
        return Err(ForensicContextError::Invalid);
    }
    let mut tx = scoped(pool, parent.workspace, false).await?;
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2
            AND analysis_id IN ($3,$4)",
    )
    .bind(parent.workspace)
    .bind(parent.situation)
    .bind(parent.analysis)
    .bind(child.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if visible != 2 {
        return Err(ForensicContextError::Invisible);
    }

    let parent_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM niccolo.analysis_forensic_context
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(parent.workspace)
    .bind(parent.situation)
    .bind(parent.analysis)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO niccolo.analysis_forensic_context
         (workspace_id,situation_id,analysis_id,binding_id,import_batch_id,item_id,note,
          bound_by_principal_id)
         SELECT workspace_id,situation_id,$4,binding_id,import_batch_id,item_id,note,
                bound_by_principal_id
           FROM niccolo.analysis_forensic_context
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
         ON CONFLICT DO NOTHING",
    )
    .bind(parent.workspace)
    .bind(parent.situation)
    .bind(parent.analysis)
    .bind(child.analysis)
    .execute(&mut *tx)
    .await?;

    let verified: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM niccolo.analysis_forensic_context p
           JOIN niccolo.analysis_forensic_context c
             ON c.workspace_id=p.workspace_id
            AND c.situation_id=p.situation_id
            AND c.analysis_id=$4
            AND c.binding_id=p.binding_id
            AND c.import_batch_id=p.import_batch_id
            AND c.item_id=p.item_id
            AND c.note IS NOT DISTINCT FROM p.note
            AND c.bound_by_principal_id=p.bound_by_principal_id
          WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.analysis_id=$3",
    )
    .bind(parent.workspace)
    .bind(parent.situation)
    .bind(parent.analysis)
    .bind(child.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if verified != parent_count {
        return Err(ForensicContextError::Conflict);
    }

    tx.commit().await?;
    Ok(parent_count as u64)
}

pub async fn bind(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    input: &BindingInput,
) -> Result<BindingResult, ForensicContextError> {
    if input.binding_id.is_nil() || input.import_batch_id.is_nil() || input.item_id.is_nil() {
        return Err(ForensicContextError::Invalid);
    }
    let note = clean_note(input.note.as_deref())?;
    let mut tx = scoped(pool, scope.workspace, false).await?;

    let analysis: Option<Uuid> = sqlx::query_scalar(
        "SELECT analysis_id FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_optional(&mut *tx)
    .await?;
    if analysis.is_none() {
        return Err(ForensicContextError::Invisible);
    }

    let item_visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM niccolo.forensic_import_items
             WHERE workspace_id=$1 AND import_batch_id=$2 AND item_id=$3
         )",
    )
    .bind(scope.workspace)
    .bind(input.import_batch_id)
    .bind(input.item_id)
    .fetch_one(&mut *tx)
    .await?;
    if !item_visible {
        return Err(ForensicContextError::Invisible);
    }

    let existing: Option<(Uuid, Uuid, Uuid, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT binding_id,import_batch_id,item_id,note,bound_by_principal_id
           FROM niccolo.analysis_forensic_context
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND (binding_id=$4 OR (import_batch_id=$5 AND item_id=$6))
          ORDER BY (binding_id=$4) DESC
          LIMIT 1",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.binding_id)
    .bind(input.import_batch_id)
    .bind(input.item_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(existing) = existing {
        if existing.1 != input.import_batch_id
            || existing.2 != input.item_id
            || existing.3 != note
            || existing.4 != actor
        {
            return Err(ForensicContextError::Conflict);
        }
        let views = list_in_tx(&mut tx, scope).await?;
        let item = views
            .into_iter()
            .find(|view| view.binding_id == existing.0)
            .ok_or(ForensicContextError::Conflict)?;
        tx.commit().await?;
        return Ok(BindingResult {
            item,
            replayed: true,
        });
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.analysis_forensic_context
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_BINDINGS_PER_ANALYSIS {
        return Err(ForensicContextError::TooMany);
    }

    sqlx::query(
        "INSERT INTO niccolo.analysis_forensic_context
         (workspace_id,situation_id,analysis_id,binding_id,import_batch_id,item_id,note,
          bound_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.binding_id)
    .bind(input.import_batch_id)
    .bind(input.item_id)
    .bind(note.as_deref())
    .bind(actor)
    .execute(&mut *tx)
    .await?;

    let views = list_in_tx(&mut tx, scope).await?;
    let item = views
        .into_iter()
        .find(|view| view.binding_id == input.binding_id)
        .ok_or(ForensicContextError::Conflict)?;
    tx.commit().await?;
    Ok(BindingResult {
        item,
        replayed: false,
    })
}
