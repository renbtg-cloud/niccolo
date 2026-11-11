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

async fn rows_for_situation(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Value>, ForensicExportError> {
    let table = safe_ident(table)?;
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND situation_id=$2 ORDER BY to_jsonb(t)::text"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(situation)
        .fetch_all(&mut **tx)
        .await?;
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
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND document_id=ANY($2::uuid[]) ORDER BY to_jsonb(t)::text"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(documents)
        .fetch_all(&mut **tx)
        .await?;
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
    let sql = format!(
        "SELECT to_jsonb(t)::text FROM niccolo.{table} t \
         WHERE workspace_id=$1 AND person_id=ANY($2::uuid[]) ORDER BY to_jsonb(t)::text"
    );
    let rows: Vec<String> = sqlx::query_scalar(&sql)
        .bind(workspace)
        .bind(people)
        .fetch_all(&mut **tx)
        .await?;
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
        .collect()
}

async fn evidence_rows(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    documents: &[Uuid],
    situation: Uuid,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>, Vec<Value>), ForensicExportError> {
    let evidence: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE seed(evidence_id) AS ( \
           SELECT f.evidence_id FROM niccolo.source_fragments f \
             WHERE f.workspace_id=$1 AND f.document_id=ANY($2::uuid[]) \
           UNION \
           SELECT cf.evidence_id FROM niccolo.situation_analysis_claim_fragments cf \
             WHERE cf.workspace_id=$1 AND cf.situation_id=$3 \
         ), closure(evidence_id) AS ( \
           SELECT evidence_id FROM seed \
           UNION \
           SELECT d.parent_id FROM niccolo.evidence_derivation d \
             JOIN closure c ON c.evidence_id=d.child_id \
             WHERE d.workspace_id=$1 \
         ) \
         SELECT to_jsonb(e)::text FROM niccolo.evidence e \
         WHERE e.workspace_id=$1 AND e.evidence_id IN (SELECT evidence_id FROM closure) \
         ORDER BY to_jsonb(e)::text",
    )
    .bind(workspace)
    .bind(documents)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;

    let derivation: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE seed(evidence_id) AS ( \
           SELECT f.evidence_id FROM niccolo.source_fragments f \
             WHERE f.workspace_id=$1 AND f.document_id=ANY($2::uuid[]) \
           UNION \
           SELECT cf.evidence_id FROM niccolo.situation_analysis_claim_fragments cf \
             WHERE cf.workspace_id=$1 AND cf.situation_id=$3 \
         ), closure(evidence_id) AS ( \
           SELECT evidence_id FROM seed \
           UNION \
           SELECT d.parent_id FROM niccolo.evidence_derivation d \
             JOIN closure c ON c.evidence_id=d.child_id \
             WHERE d.workspace_id=$1 \
         ) \
         SELECT to_jsonb(d)::text FROM niccolo.evidence_derivation d \
         WHERE d.workspace_id=$1 \
           AND d.child_id IN (SELECT evidence_id FROM closure) \
           AND d.parent_id IN (SELECT evidence_id FROM closure) \
         ORDER BY to_jsonb(d)::text",
    )
    .bind(workspace)
    .bind(documents)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;

    let proposition_links: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE seed(evidence_id) AS ( \
           SELECT f.evidence_id FROM niccolo.source_fragments f \
             WHERE f.workspace_id=$1 AND f.document_id=ANY($2::uuid[]) \
           UNION \
           SELECT cf.evidence_id FROM niccolo.situation_analysis_claim_fragments cf \
             WHERE cf.workspace_id=$1 AND cf.situation_id=$3 \
         ), closure(evidence_id) AS ( \
           SELECT evidence_id FROM seed \
           UNION \
           SELECT d.parent_id FROM niccolo.evidence_derivation d \
             JOIN closure c ON c.evidence_id=d.child_id WHERE d.workspace_id=$1 \
         ) \
         SELECT to_jsonb(pe)::text FROM niccolo.proposition_evidence pe \
         WHERE pe.workspace_id=$1 AND pe.evidence_id IN (SELECT evidence_id FROM closure) \
         ORDER BY to_jsonb(pe)::text",
    )
    .bind(workspace)
    .bind(documents)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;

    let propositions: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE seed(evidence_id) AS ( \
           SELECT f.evidence_id FROM niccolo.source_fragments f \
             WHERE f.workspace_id=$1 AND f.document_id=ANY($2::uuid[]) \
           UNION \
           SELECT cf.evidence_id FROM niccolo.situation_analysis_claim_fragments cf \
             WHERE cf.workspace_id=$1 AND cf.situation_id=$3 \
         ), closure(evidence_id) AS ( \
           SELECT evidence_id FROM seed \
           UNION \
           SELECT d.parent_id FROM niccolo.evidence_derivation d \
             JOIN closure c ON c.evidence_id=d.child_id WHERE d.workspace_id=$1 \
         ), proposition_ids AS ( \
           SELECT DISTINCT pe.proposition_id FROM niccolo.proposition_evidence pe \
           WHERE pe.workspace_id=$1 AND pe.evidence_id IN (SELECT evidence_id FROM closure) \
         ) \
         SELECT to_jsonb(p)::text FROM niccolo.propositions p \
         WHERE p.workspace_id=$1 AND p.proposition_id IN (SELECT proposition_id FROM proposition_ids) \
         ORDER BY to_jsonb(p)::text",
    )
    .bind(workspace)
    .bind(documents)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;

    let parse = |rows: Vec<String>| -> Result<Vec<Value>, ForensicExportError> {
        rows.into_iter()
            .map(|row| serde_json::from_str(&row).map_err(ForensicExportError::from))
            .collect()
    };
    Ok((
        parse(evidence)?,
        parse(derivation)?,
        parse(proposition_links)?,
        parse(propositions)?,
    ))
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
    for table in situation_tables {
        let rows = rows_for_situation(&mut tx, &table, workspace, situation).await?;
        if !rows.is_empty() {
            logical.insert(table, rows);
        }
    }

    let documents: Vec<Uuid> = sqlx::query_scalar(
        "SELECT document_id FROM ( \
           SELECT document_id FROM niccolo.situation_sources WHERE workspace_id=$1 AND situation_id=$2 \
           UNION \
           SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 \
         ) d ORDER BY document_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;

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
            if !rows.is_empty() {
                logical.insert(table, rows);
            }
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
    if !identity_subjects.is_empty() {
        logical.insert("identity_subjects".into(), identity_subjects);
    }

    let (evidence, derivation, proposition_links, propositions) =
        evidence_rows(&mut tx, workspace, &documents, situation).await?;
    for (name, rows) in [
        ("evidence", evidence),
        ("evidence_derivation", derivation),
        ("proposition_evidence", proposition_links),
        ("propositions", propositions),
    ] {
        if !rows.is_empty() {
            logical.insert(name.into(), rows);
        }
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
