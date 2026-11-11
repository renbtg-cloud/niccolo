//! Deterministic, read-only forensic export of one Situation.
//!
//! This is deliberately not a PostgreSQL dump and not an epistemic upgrade.
//! Structural validity proves only that the package is internally consistent.
//! Machine outputs, human assertions and imported configuration remain what
//! they were in the recorded state; inclusion never turns them into evidence.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const MAX_SOURCE_DOCUMENTS: usize = 64;
const MAX_ROWS_PER_TABLE: i64 = 100_000;
const MAX_TOTAL_LOGICAL_ROWS: usize = 200_000;
const MAX_LOGICAL_TABLE_BYTES: i64 = 16 * 1024 * 1024;
const MAX_TOTAL_LOGICAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_DECLARED_SOURCE_BYTES: i64 = 32 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ForensicExportError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("forensic export serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("privileged database connection forbidden")]
    PrivilegedRole,
    #[error("dedicated secure API role required")]
    InsecureRole,
    #[error("situation unavailable")]
    Invisible,
    #[error("stored original source bytes do not match source metadata")]
    CorruptOriginal,
    #[error("unexpected database identifier in catalog")]
    InvalidCatalogIdentifier,
    #[error("forensic export exceeds bounded in-memory package limits: {0}")]
    TooLarge(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicManifestEntry {
    pub path: String,
    pub kind: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicManifest {
    pub algorithm: String,
    pub entries: Vec<ForensicManifestEntry>,
    pub root_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalSourcePayload {
    pub document_id: Uuid,
    pub format: String,
    pub locator: String,
    pub declared_sha256: String,
    pub byte_length: u64,
    /// Lowercase hexadecimal representation of the exact retained upload bytes.
    /// The manifest digest is calculated over the raw bytes, not this encoding.
    pub bytes_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicSituationExport {
    pub format: String,
    pub format_version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    /// Recorded rows whose schema ties them directly to this Situation, plus
    /// bounded related source/identity/evidence rows selected below.
    pub logical_tables: BTreeMap<String, Vec<Value>>,
    pub original_source_payloads: Vec<OriginalSourcePayload>,
    pub missing_original_document_ids: Vec<Uuid>,
    pub manifest: ForensicManifest,
    pub structural_validity_establishes_truth: bool,
    pub included_model_output_is_documentary_evidence: bool,
    pub imported_state_should_be_trusted_automatically: bool,
    pub omissions: Vec<String>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn safe_ident(value: &str) -> Result<&str, ForensicExportError> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        Ok(value)
    } else {
        Err(ForensicExportError::InvalidCatalogIdentifier)
    }
}

fn check_table_budget(rows: i64, bytes: i64) -> Result<(), ForensicExportError> {
    if rows > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge(
            "per-table logical row budget",
        ));
    }
    if bytes > MAX_LOGICAL_TABLE_BYTES {
        return Err(ForensicExportError::TooLarge(
            "per-table logical byte budget",
        ));
    }
    Ok(())
}

async fn rows_for_situation(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Value>, ForensicExportError> {
    let table = safe_ident(table)?;
    let budget_sql = format!(
        "SELECT count(*)::bigint, COALESCE(sum(octet_length(to_jsonb(t)::text)),0)::bigint \
         FROM niccolo.{table} t WHERE workspace_id=$1 AND situation_id=$2"
    );
    let (row_count, byte_count): (i64, i64) = sqlx::query_as(&budget_sql)
        .bind(workspace)
        .bind(situation)
        .fetch_one(&mut **tx)
        .await?;
    check_table_budget(row_count, byte_count)?;
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND situation_id=$2 ORDER BY to_jsonb(t)::text LIMIT $3"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(situation)
        .bind(MAX_ROWS_PER_TABLE + 1)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() as i64 > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge("per-table logical row budget"));
    }
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn rows_for_documents(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    workspace: Uuid,
    documents: &[Uuid],
) -> Result<Vec<Value>, ForensicExportError> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }
    let table = safe_ident(table)?;
    let budget_sql = format!(
        "SELECT count(*)::bigint, COALESCE(sum(octet_length(to_jsonb(t)::text)),0)::bigint \
         FROM niccolo.{table} t WHERE workspace_id=$1 AND document_id=ANY($2::uuid[])"
    );
    let (row_count, byte_count): (i64, i64) = sqlx::query_as(&budget_sql)
        .bind(workspace)
        .bind(documents)
        .fetch_one(&mut **tx)
        .await?;
    check_table_budget(row_count, byte_count)?;
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND document_id=ANY($2::uuid[]) ORDER BY to_jsonb(t)::text LIMIT $3"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(documents)
        .bind(MAX_ROWS_PER_TABLE + 1)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() as i64 > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge("per-table logical row budget"));
    }
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn rows_for_people(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    workspace: Uuid,
    people: &[Uuid],
) -> Result<Vec<Value>, ForensicExportError> {
    if people.is_empty() {
        return Ok(Vec::new());
    }
    let table = safe_ident(table)?;
    let budget_sql = format!(
        "SELECT count(*)::bigint, COALESCE(sum(octet_length(to_jsonb(t)::text)),0)::bigint \
         FROM niccolo.{table} t WHERE workspace_id=$1 AND person_id=ANY($2::uuid[])"
    );
    let (row_count, byte_count): (i64, i64) = sqlx::query_as(&budget_sql)
        .bind(workspace)
        .bind(people)
        .fetch_one(&mut **tx)
        .await?;
    check_table_budget(row_count, byte_count)?;
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND person_id=ANY($2::uuid[]) ORDER BY to_jsonb(t)::text LIMIT $3"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(people)
        .bind(MAX_ROWS_PER_TABLE + 1)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() as i64 > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge("per-table logical row budget"));
    }
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn rows_for_uuid_membership(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    column: &str,
    workspace: Uuid,
    ids: &[Uuid],
) -> Result<Vec<Value>, ForensicExportError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let table = safe_ident(table)?;
    let column = safe_ident(column)?;
    let budget_sql = format!(
        "SELECT count(*)::bigint, COALESCE(sum(octet_length(to_jsonb(t)::text)),0)::bigint \
         FROM niccolo.{table} t WHERE workspace_id=$1 AND {column}=ANY($2::uuid[])"
    );
    let (row_count, byte_count): (i64, i64) = sqlx::query_as(&budget_sql)
        .bind(workspace)
        .bind(ids)
        .fetch_one(&mut **tx)
        .await?;
    check_table_budget(row_count, byte_count)?;
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND {column}=ANY($2::uuid[]) \
         ORDER BY to_jsonb(t)::text LIMIT $3"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(ids)
        .bind(MAX_ROWS_PER_TABLE + 1)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() as i64 > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge(
            "per-table UUID membership row budget",
        ));
    }
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn rows_for_derivation_closure(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    evidence_ids: &[Uuid],
) -> Result<Vec<Value>, ForensicExportError> {
    if evidence_ids.is_empty() {
        return Ok(Vec::new());
    }
    let (row_count, byte_count): (i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint, COALESCE(sum(octet_length(to_jsonb(d)::text)),0)::bigint \
         FROM niccolo.evidence_derivation d \
         WHERE d.workspace_id=$1 \
           AND d.child_id=ANY($2::uuid[]) AND d.parent_id=ANY($2::uuid[])",
    )
    .bind(workspace)
    .bind(evidence_ids)
    .fetch_one(&mut **tx)
    .await?;
    check_table_budget(row_count, byte_count)?;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT to_jsonb(d)::text FROM niccolo.evidence_derivation d \
         WHERE d.workspace_id=$1 \
           AND d.child_id=ANY($2::uuid[]) AND d.parent_id=ANY($2::uuid[]) \
         ORDER BY to_jsonb(d)::text LIMIT $3",
    )
    .bind(workspace)
    .bind(evidence_ids)
    .bind(MAX_ROWS_PER_TABLE + 1)
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() as i64 > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge(
            "per-table evidence derivation row budget",
        ));
    }
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn situation_evidence_ids(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Uuid>, ForensicExportError> {
    let references: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname, e.attname \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid=c.relnamespace \
         JOIN pg_attribute e ON e.attrelid=c.oid \
         JOIN pg_type et ON et.oid=e.atttypid \
         WHERE n.nspname='niccolo' AND c.relkind='r' \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='workspace_id' AND NOT a.attisdropped) \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='situation_id' AND NOT a.attisdropped) \
           AND e.attname LIKE '%evidence_id' AND NOT e.attisdropped AND et.typname='uuid' \
         ORDER BY c.relname,e.attname",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut ids = BTreeSet::new();
    for (table, column) in references {
        let table = safe_ident(&table)?;
        let column = safe_ident(&column)?;
        let sql = format!(
            "SELECT DISTINCT {column} FROM niccolo.{table} \
             WHERE workspace_id=$1 AND situation_id=$2 AND {column} IS NOT NULL \
             ORDER BY {column} LIMIT $3"
        );
        let found: Vec<Uuid> = sqlx::query_scalar(&sql)
            .bind(workspace)
            .bind(situation)
            .bind(MAX_ROWS_PER_TABLE + 1)
            .fetch_all(&mut **tx)
            .await?;
        if found.len() as i64 > MAX_ROWS_PER_TABLE {
            return Err(ForensicExportError::TooLarge(
                "per-table evidence-reference budget",
            ));
        }
        ids.extend(found);
        if ids.len() > MAX_TOTAL_LOGICAL_ROWS {
            return Err(ForensicExportError::TooLarge(
                "total evidence-reference budget",
            ));
        }
    }
    Ok(ids.into_iter().collect())
}

async fn situation_question_ids(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Uuid>, ForensicExportError> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname='niccolo' AND c.relkind='r' \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='workspace_id' AND NOT a.attisdropped) \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='situation_id' AND NOT a.attisdropped) \
           AND EXISTS(SELECT 1 FROM pg_attribute a JOIN pg_type t ON t.oid=a.atttypid \
                      WHERE a.attrelid=c.oid AND a.attname='question_id' AND NOT a.attisdropped AND t.typname='uuid') \
         ORDER BY c.relname",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut ids = BTreeSet::new();
    for table in tables {
        let table = safe_ident(&table)?;
        let sql = format!(
            "SELECT DISTINCT question_id FROM niccolo.{table} \
             WHERE workspace_id=$1 AND situation_id=$2 AND question_id IS NOT NULL \
             ORDER BY question_id LIMIT $3"
        );
        let found: Vec<Uuid> = sqlx::query_scalar(&sql)
            .bind(workspace)
            .bind(situation)
            .bind(MAX_ROWS_PER_TABLE + 1)
            .fetch_all(&mut **tx)
            .await?;
        if found.len() as i64 > MAX_ROWS_PER_TABLE {
            return Err(ForensicExportError::TooLarge(
                "per-table question-reference budget",
            ));
        }
        ids.extend(found);
        if ids.len() > MAX_TOTAL_LOGICAL_ROWS {
            return Err(ForensicExportError::TooLarge(
                "total question-reference budget",
            ));
        }
    }
    Ok(ids.into_iter().collect())
}

async fn rows_for_questions(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    workspace: Uuid,
    questions: &[Uuid],
) -> Result<Vec<Value>, ForensicExportError> {
    if questions.is_empty() {
        return Ok(Vec::new());
    }
    let table = safe_ident(table)?;
    let budget_sql = format!(
        "SELECT count(*)::bigint, COALESCE(sum(octet_length(to_jsonb(t)::text)),0)::bigint \
         FROM niccolo.{table} t WHERE workspace_id=$1 AND question_id=ANY($2::uuid[])"
    );
    let (row_count, byte_count): (i64, i64) = sqlx::query_as(&budget_sql)
        .bind(workspace)
        .bind(questions)
        .fetch_one(&mut **tx)
        .await?;
    check_table_budget(row_count, byte_count)?;
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND question_id=ANY($2::uuid[]) \
         ORDER BY to_jsonb(t)::text LIMIT $3"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(questions)
        .bind(MAX_ROWS_PER_TABLE + 1)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() as i64 > MAX_ROWS_PER_TABLE {
        return Err(ForensicExportError::TooLarge(
            "per-table question row budget",
        ));
    }
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn situation_document_ids(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Uuid>, ForensicExportError> {
    let references: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname, d.attname \
         FROM pg_class c \
         JOIN pg_namespace n ON n.oid=c.relnamespace \
         JOIN pg_attribute d ON d.attrelid=c.oid \
         JOIN pg_type dt ON dt.oid=d.atttypid \
         WHERE n.nspname='niccolo' AND c.relkind='r' \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='workspace_id' AND NOT a.attisdropped) \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='situation_id' AND NOT a.attisdropped) \
           AND d.attname LIKE '%document_id' AND NOT d.attisdropped AND dt.typname='uuid' \
         ORDER BY c.relname,d.attname",
    )
    .fetch_all(&mut **tx)
    .await?;
    let mut ids = BTreeSet::new();
    for (table, column) in references {
        let table = safe_ident(&table)?;
        let column = safe_ident(&column)?;
        let sql = format!(
            "SELECT DISTINCT {column} FROM niccolo.{table} \
             WHERE workspace_id=$1 AND situation_id=$2 AND {column} IS NOT NULL \
             ORDER BY {column} LIMIT $3"
        );
        let found: Vec<Uuid> = sqlx::query_scalar(&sql)
            .bind(workspace)
            .bind(situation)
            .bind((MAX_SOURCE_DOCUMENTS + 1) as i64)
            .fetch_all(&mut **tx)
            .await?;
        if found.len() > MAX_SOURCE_DOCUMENTS {
            return Err(ForensicExportError::TooLarge(
                "per-table source-document reference budget",
            ));
        }
        ids.extend(found);
        if ids.len() > MAX_SOURCE_DOCUMENTS {
            return Err(ForensicExportError::TooLarge(
                "Situation-linked source-document budget",
            ));
        }
    }
    Ok(ids.into_iter().collect())
}

async fn evidence_ancestor_ids(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    seed_evidence: &[Uuid],
) -> Result<Vec<Uuid>, ForensicExportError> {
    if seed_evidence.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE closure(evidence_id) AS ( \
           SELECT unnest($2::uuid[]) \
           UNION \
           SELECT d.parent_id FROM niccolo.evidence_derivation d \
             JOIN closure c ON c.evidence_id=d.child_id WHERE d.workspace_id=$1 \
         ) SELECT evidence_id FROM closure ORDER BY evidence_id LIMIT $3",
    )
    .bind(workspace)
    .bind(seed_evidence)
    .bind((MAX_TOTAL_LOGICAL_ROWS + 1) as i64)
    .fetch_all(&mut **tx)
    .await?;
    if ids.len() > MAX_TOTAL_LOGICAL_ROWS {
        return Err(ForensicExportError::TooLarge(
            "evidence ancestry budget",
        ));
    }
    Ok(ids)
}

async fn expand_source_scope(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<(Vec<Uuid>, Vec<Uuid>), ForensicExportError> {
    let mut documents = situation_document_ids(tx, workspace, situation)
        .await?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut evidence = situation_evidence_ids(tx, workspace, situation)
        .await?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let question_ids = situation_question_ids(tx, workspace, situation).await?;
    if !question_ids.is_empty() {
        let question_evidence: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT evidence_id FROM niccolo.question_evidence_references \
             WHERE workspace_id=$1 AND question_id=ANY($2::uuid[]) \
             ORDER BY evidence_id LIMIT $3",
        )
        .bind(workspace)
        .bind(&question_ids)
        .bind((MAX_TOTAL_LOGICAL_ROWS + 1) as i64)
        .fetch_all(&mut **tx)
        .await?;
        if question_evidence.len() > MAX_TOTAL_LOGICAL_ROWS {
            return Err(ForensicExportError::TooLarge(
                "question evidence-reference budget",
            ));
        }
        evidence.extend(question_evidence);
    }

    loop {
        let before = (documents.len(), evidence.len());

        if !documents.is_empty() {
            let document_ids = documents.iter().copied().collect::<Vec<_>>();
            let found: Vec<Uuid> = sqlx::query_scalar(
                "SELECT DISTINCT evidence_id FROM niccolo.source_fragments \
                 WHERE workspace_id=$1 AND document_id=ANY($2::uuid[]) \
                 ORDER BY evidence_id LIMIT $3",
            )
            .bind(workspace)
            .bind(&document_ids)
            .bind((MAX_TOTAL_LOGICAL_ROWS + 1) as i64)
            .fetch_all(&mut **tx)
            .await?;
            if found.len() > MAX_TOTAL_LOGICAL_ROWS {
                return Err(ForensicExportError::TooLarge("evidence seed budget"));
            }
            evidence.extend(found);
        }

        if evidence.len() > MAX_TOTAL_LOGICAL_ROWS {
            return Err(ForensicExportError::TooLarge("evidence seed budget"));
        }
        let evidence_ids = evidence.iter().copied().collect::<Vec<_>>();
        evidence.extend(evidence_ancestor_ids(tx, workspace, &evidence_ids).await?);
        if evidence.len() > MAX_TOTAL_LOGICAL_ROWS {
            return Err(ForensicExportError::TooLarge("evidence seed budget"));
        }

        if !evidence.is_empty() {
            let evidence_ids = evidence.iter().copied().collect::<Vec<_>>();
            let found: Vec<Uuid> = sqlx::query_scalar(
                "SELECT DISTINCT document_id FROM niccolo.source_fragments \
                 WHERE workspace_id=$1 AND evidence_id=ANY($2::uuid[]) \
                 ORDER BY document_id LIMIT $3",
            )
            .bind(workspace)
            .bind(&evidence_ids)
            .bind((MAX_SOURCE_DOCUMENTS + 1) as i64)
            .fetch_all(&mut **tx)
            .await?;
            if found.len() > MAX_SOURCE_DOCUMENTS {
                return Err(ForensicExportError::TooLarge(
                    "Situation-linked source-document budget",
                ));
            }
            documents.extend(found);
        }

        if documents.len() > MAX_SOURCE_DOCUMENTS {
            return Err(ForensicExportError::TooLarge(
                "Situation-linked source-document budget",
            ));
        }
        if before == (documents.len(), evidence.len()) {
            break;
        }
    }

    Ok((
        documents.into_iter().collect(),
        evidence.into_iter().collect(),
    ))
}

async fn evidence_rows(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    seed_evidence: &[Uuid],
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>, Vec<Value>), ForensicExportError> {
    if seed_evidence.is_empty() {
        return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
    }

    let closure = evidence_ancestor_ids(tx, workspace, seed_evidence).await?;
    let evidence =
        rows_for_uuid_membership(tx, "evidence", "evidence_id", workspace, &closure).await?;
    let derivation = rows_for_derivation_closure(tx, workspace, &closure).await?;
    let proposition_links = rows_for_uuid_membership(
        tx,
        "proposition_evidence",
        "evidence_id",
        workspace,
        &closure,
    )
    .await?;

    let proposition_ids: Vec<Uuid> = if closure.is_empty() {
        Vec::new()
    } else {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT proposition_id FROM niccolo.proposition_evidence \
             WHERE workspace_id=$1 AND evidence_id=ANY($2::uuid[]) \
             ORDER BY proposition_id LIMIT $3",
        )
        .bind(workspace)
        .bind(&closure)
        .bind(MAX_ROWS_PER_TABLE + 1)
        .fetch_all(&mut **tx)
        .await?;
        if ids.len() as i64 > MAX_ROWS_PER_TABLE {
            return Err(ForensicExportError::TooLarge(
                "per-table proposition reference budget",
            ));
        }
        ids
    };
    let propositions = rows_for_uuid_membership(
        tx,
        "propositions",
        "proposition_id",
        workspace,
        &proposition_ids,
    )
    .await?;

    Ok((evidence, derivation, proposition_links, propositions))
}

fn insert_logical_rows(
    logical: &mut BTreeMap<String, Vec<Value>>,
    total_rows: &mut usize,
    total_bytes: &mut usize,
    table: String,
    rows: Vec<Value>,
) -> Result<(), ForensicExportError> {
    if rows.is_empty() {
        return Ok(());
    }
    *total_rows = total_rows
        .checked_add(rows.len())
        .ok_or(ForensicExportError::TooLarge("total logical row budget"))?;
    if *total_rows > MAX_TOTAL_LOGICAL_ROWS {
        return Err(ForensicExportError::TooLarge("total logical row budget"));
    }
    let added_bytes = rows.iter().try_fold(0usize, |sum, row| {
        let bytes = serde_json::to_vec(row)?.len();
        sum.checked_add(bytes)
            .ok_or(ForensicExportError::TooLarge("total logical byte budget"))
    })?;
    *total_bytes = total_bytes
        .checked_add(added_bytes)
        .ok_or(ForensicExportError::TooLarge("total logical byte budget"))?;
    if *total_bytes > MAX_TOTAL_LOGICAL_BYTES {
        return Err(ForensicExportError::TooLarge("total logical byte budget"));
    }
    logical.insert(table, rows);
    Ok(())
}

fn build_manifest(
    logical: &BTreeMap<String, Vec<Value>>,
    payloads: &[OriginalSourcePayload],
) -> Result<ForensicManifest, ForensicExportError> {
    let mut entries = Vec::new();
    for (table, rows) in logical {
        for (index, row) in rows.iter().enumerate() {
            let bytes = serde_json::to_vec(row)?;
            entries.push(ForensicManifestEntry {
                path: format!("logical/{table}/{index:08}.json"),
                kind: "logical-row".into(),
                sha256: digest(&bytes),
                byte_length: bytes.len() as u64,
            });
        }
    }
    for payload in payloads {
        entries.push(ForensicManifestEntry {
            path: format!("sources/{}/original", payload.document_id),
            kind: "original-source-bytes".into(),
            sha256: payload.declared_sha256.clone(),
            byte_length: payload.byte_length,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut root = Sha256::new();
    for entry in &entries {
        root.update(entry.path.as_bytes());
        root.update([0]);
        root.update(entry.kind.as_bytes());
        root.update([0]);
        root.update(entry.sha256.as_bytes());
        root.update([0]);
        root.update(entry.byte_length.to_string().as_bytes());
        root.update([0xff]);
    }
    Ok(ForensicManifest {
        algorithm: "sha256".into(),
        entries,
        root_sha256: format!("{:x}", root.finalize()),
    })
}

pub async fn build(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<ForensicSituationExport, ForensicExportError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let (privileged, secure): (bool, bool) = sqlx::query_as(
        "SELECT rolsuper OR rolbypassrls, pg_has_role(current_user,'niccolo_api','member') \
         FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ForensicExportError::PrivilegedRole);
    }
    if !secure {
        return Err(ForensicExportError::InsecureRole);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;

    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ForensicExportError::Invisible);
    }

    let situation_tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname='niccolo' AND c.relkind='r' \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='workspace_id' AND NOT a.attisdropped) \
           AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='situation_id' AND NOT a.attisdropped) \
         ORDER BY c.relname",
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut logical = BTreeMap::new();
    let mut logical_row_count = 0usize;
    let mut logical_byte_count = 0usize;
    for table in situation_tables {
        let rows = rows_for_situation(&mut tx, &table, workspace, situation).await?;
        insert_logical_rows(
            &mut logical,
            &mut logical_row_count,
            &mut logical_byte_count,
            table,
            rows,
        )?;
    }

    let question_ids = situation_question_ids(&mut tx, workspace, situation).await?;
    for table in [
        "investigative_questions",
        "question_feedback_events",
        "question_evidence_references",
    ] {
        let rows = rows_for_questions(&mut tx, table, workspace, &question_ids).await?;
        insert_logical_rows(
            &mut logical,
            &mut logical_row_count,
            &mut logical_byte_count,
            table.into(),
            rows,
        )?;
    }

    let (documents, evidence_seed) =
        expand_source_scope(&mut tx, workspace, situation).await?;

    if !documents.is_empty() {
        let document_tables: Vec<String> = sqlx::query_scalar(
            "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname='niccolo' AND c.relkind='r' AND c.relname<>'source_payloads' \
               AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='workspace_id' AND NOT a.attisdropped) \
               AND EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='document_id' AND NOT a.attisdropped) \
               AND NOT EXISTS(SELECT 1 FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attname='situation_id' AND NOT a.attisdropped) \
             ORDER BY c.relname",
        )
        .fetch_all(&mut *tx)
        .await?;
        for table in document_tables {
            let rows = rows_for_documents(&mut tx, &table, workspace, &documents).await?;
            insert_logical_rows(
            &mut logical,
            &mut logical_row_count,
            &mut logical_byte_count,
            table,
            rows,
        )?;
        }
    }

    let people: Vec<Uuid> = if documents.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar(
            "SELECT DISTINCT person_id FROM niccolo.identity_events \
             WHERE workspace_id=$1 AND document_id=ANY($2::uuid[]) AND person_id IS NOT NULL \
             ORDER BY person_id",
        )
        .bind(workspace)
        .bind(&documents)
        .fetch_all(&mut *tx)
        .await?
    };
    let identity_subjects =
        rows_for_people(&mut tx, "identity_subjects", workspace, &people).await?;
    insert_logical_rows(
        &mut logical,
        &mut logical_row_count,
        &mut logical_byte_count,
        "identity_subjects".into(),
        identity_subjects,
    )?;

    let (evidence, derivation, proposition_links, propositions) =
        evidence_rows(&mut tx, workspace, &evidence_seed).await?;
    for (name, rows) in [
        ("evidence", evidence),
        ("evidence_derivation", derivation),
        ("proposition_evidence", proposition_links),
        ("propositions", propositions),
    ] {
        insert_logical_rows(
            &mut logical,
            &mut logical_row_count,
            &mut logical_byte_count,
            name.into(),
            rows,
        )?;
    }

    let declared_source_bytes: i64 = if documents.is_empty() {
        0
    } else {
        sqlx::query_scalar(
            "SELECT COALESCE(sum(byte_length),0)::bigint FROM niccolo.source_documents \
             WHERE workspace_id=$1 AND document_id=ANY($2::uuid[])",
        )
        .bind(workspace)
        .bind(&documents)
        .fetch_one(&mut *tx)
        .await?
    };
    if declared_source_bytes > MAX_DECLARED_SOURCE_BYTES {
        return Err(ForensicExportError::TooLarge("retained source-byte budget"));
    }

    let payload_rows: Vec<(Uuid, String, String, String, i64, Vec<u8>)> = if documents.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT d.document_id,d.format,d.locator,d.sha256,d.byte_length,p.original_bytes \
             FROM niccolo.source_documents d JOIN niccolo.source_payloads p USING(workspace_id,document_id) \
             WHERE d.workspace_id=$1 AND d.document_id=ANY($2::uuid[]) ORDER BY d.document_id",
        )
        .bind(workspace)
        .bind(&documents)
        .fetch_all(&mut *tx)
        .await?
    };
    let mut original_source_payloads = Vec::new();
    let mut with_payload = BTreeSet::new();
    for (document_id, format, locator, declared_sha256, byte_length, bytes) in payload_rows {
        if digest(&bytes) != declared_sha256 || bytes.len() as i64 != byte_length {
            return Err(ForensicExportError::CorruptOriginal);
        }
        with_payload.insert(document_id);
        original_source_payloads.push(OriginalSourcePayload {
            document_id,
            format,
            locator,
            declared_sha256,
            byte_length: byte_length as u64,
            bytes_hex: hex(&bytes),
        });
    }
    let missing_original_document_ids = documents
        .iter()
        .copied()
        .filter(|id| !with_payload.contains(id))
        .collect::<Vec<_>>();

    tx.commit().await?;
    let manifest = build_manifest(&logical, &original_source_payloads)?;
    Ok(ForensicSituationExport {
        format: "niccolo.situation-forensic-export".into(),
        format_version: 1,
        workspace_id: workspace,
        situation_id: situation,
        logical_tables: logical,
        original_source_payloads,
        missing_original_document_ids,
        manifest,
        structural_validity_establishes_truth: false,
        included_model_output_is_documentary_evidence: false,
        imported_state_should_be_trusted_automatically: false,
        omissions: vec![
            "workspace-scoped records that cannot be safely attributed to this Situation are not exported".into(),
            "database roles, bearer credentials, raw token hashes and other secrets are never exported".into(),
            "absence from this package is not evidence that a record never existed outside retained Niccolo state".into(),
        ],
    })
}
