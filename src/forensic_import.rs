//! Untrusted re-import quarantine for Niccolo forensic Situation exports.
//!
//! Structural verification is a transport/integrity property only. Imported
//! rows are retained as historical material in quarantine tables and are never
//! inserted into native evidence, identity, configuration or analysis tables.
use crate::situation_forensic_export::{self, ForensicSituationExport};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ForensicImportError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("forensic package failed structural verification: {0}")]
    Structure(#[from] situation_forensic_export::ForensicExportError),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("dedicated secure API role required")]
    InsecureRole,
    #[error("invalid forensic import request")]
    Invalid,
    #[error("forensic import replay conflicts with recorded provenance")]
    Conflict,
    #[error("forensic import package manifest is incomplete")]
    ManifestIncomplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportedOriginClass {
    ImportedHumanAssertion,
    ImportedMachineOutput,
    ImportedConfigurationAssertion,
    ImportedSourceArtifact,
    ImportedIdentityAssertion,
    ImportedHistoricalState,
}

impl ImportedOriginClass {
    fn as_db(self) -> &'static str {
        match self {
            Self::ImportedHumanAssertion => "IMPORTED_HUMAN_ASSERTION",
            Self::ImportedMachineOutput => "IMPORTED_MACHINE_OUTPUT",
            Self::ImportedConfigurationAssertion => "IMPORTED_CONFIGURATION_ASSERTION",
            Self::ImportedSourceArtifact => "IMPORTED_SOURCE_ARTIFACT",
            Self::ImportedIdentityAssertion => "IMPORTED_IDENTITY_ASSERTION",
            Self::ImportedHistoricalState => "IMPORTED_HISTORICAL_STATE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreparedImportItem {
    pub source_path: String,
    pub source_kind: String,
    pub source_sha256: String,
    pub source_byte_length: u64,
    pub origin_class: ImportedOriginClass,
    pub source_table: Option<String>,
    pub source_row_index: Option<u64>,
    pub original_document_id: Option<Uuid>,
    pub payload_json: Option<Value>,
    pub payload_bytes: Option<Vec<u8>>,
    pub current_credentials_establish_truth: bool,
    pub documentary_evidence: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreparedUntrustedImport {
    pub source_package_sha256: String,
    pub source_manifest_sha256: String,
    pub source_workspace_id: Uuid,
    pub source_situation_id: Uuid,
    pub source_format: String,
    pub source_format_version: u32,
    pub items: Vec<PreparedImportItem>,
    pub structural_validity_establishes_truth: bool,
    pub current_credentials_upgrade_imported_statements: bool,
    pub imported_model_output_is_documentary_evidence: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    pub import_batch_id: Uuid,
    pub declared_origin: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportBatchView {
    pub import_batch_id: Uuid,
    pub source_package_sha256: String,
    pub source_manifest_sha256: String,
    pub source_workspace_id: Uuid,
    pub source_situation_id: Uuid,
    pub declared_origin: String,
    pub verification_status: String,
    pub import_mode: String,
    pub item_count: u64,
    pub replayed: bool,
    pub structural_validity_establishes_truth: bool,
    pub current_credentials_upgrade_imported_statements: bool,
    pub imported_model_output_is_documentary_evidence: bool,
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ForensicImportError> {
    if !value.len().is_multiple_of(2) {
        return Err(ForensicImportError::Invalid);
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().as_chunks::<2>().0 {
        let nibble = |byte: u8| -> Result<u8, ForensicImportError> {
            match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                _ => Err(ForensicImportError::Invalid),
            }
        };
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Ok(out)
}

fn classify_table(table: &str) -> ImportedOriginClass {
    if matches!(table, "identity_events" | "identity_subjects")
        || table.starts_with("identity_")
    {
        ImportedOriginClass::ImportedIdentityAssertion
    } else if table.contains("model_invocation")
        || table.contains("model_output")
        || table.contains("analysis_output")
    {
        ImportedOriginClass::ImportedMachineOutput
    } else if table.contains("epistemic_policy")
        || table.contains("weight_policy")
        || table.contains("investigation_policy")
    {
        ImportedOriginClass::ImportedConfigurationAssertion
    } else if table.contains("contribution")
        || table.contains("human_feedback")
        || table.contains("clarification_answer")
        || table.contains("manual_hypoth")
    {
        ImportedOriginClass::ImportedHumanAssertion
    } else if matches!(table, "source_documents" | "source_fragments" | "source_payloads") {
        ImportedOriginClass::ImportedSourceArtifact
    } else {
        ImportedOriginClass::ImportedHistoricalState
    }
}

pub fn prepare_untrusted(
    package: &ForensicSituationExport,
) -> Result<PreparedUntrustedImport, ForensicImportError> {
    situation_forensic_export::verify_structure(package)?;
    let package_bytes = serde_json::to_vec(package)
        .map_err(situation_forensic_export::ForensicExportError::from)?;
    let manifest: BTreeMap<&str, _> = package
        .manifest
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();

    let mut items = Vec::with_capacity(
        package.logical_tables.values().map(Vec::len).sum::<usize>()
            + package.original_source_payloads.len(),
    );
    for (table, rows) in &package.logical_tables {
        for (index, row) in rows.iter().enumerate() {
            let path = format!("logical/{table}/{index:08}.json");
            let entry = manifest
                .get(path.as_str())
                .ok_or(ForensicImportError::ManifestIncomplete)?;
            items.push(PreparedImportItem {
                source_path: path,
                source_kind: "LOGICAL_ROW".into(),
                source_sha256: entry.sha256.clone(),
                source_byte_length: entry.byte_length,
                origin_class: classify_table(table),
                source_table: Some(table.clone()),
                source_row_index: Some(index as u64),
                original_document_id: None,
                payload_json: Some(row.clone()),
                payload_bytes: None,
                current_credentials_establish_truth: false,
                documentary_evidence: false,
            });
        }
    }
    for payload in &package.original_source_payloads {
        let path = format!("sources/{}/original", payload.document_id);
        let entry = manifest
            .get(path.as_str())
            .ok_or(ForensicImportError::ManifestIncomplete)?;
        items.push(PreparedImportItem {
            source_path: path,
            source_kind: "ORIGINAL_SOURCE_BYTES".into(),
            source_sha256: entry.sha256.clone(),
            source_byte_length: entry.byte_length,
            origin_class: ImportedOriginClass::ImportedSourceArtifact,
            source_table: None,
            source_row_index: None,
            original_document_id: Some(payload.document_id),
            payload_json: None,
            payload_bytes: Some(decode_hex(&payload.bytes_hex)?),
            current_credentials_establish_truth: false,
            documentary_evidence: false,
        });
    }
    items.sort_by(|a, b| a.source_path.cmp(&b.source_path));

    Ok(PreparedUntrustedImport {
        source_package_sha256: sha256(&package_bytes),
        source_manifest_sha256: package.manifest.root_sha256.clone(),
        source_workspace_id: package.workspace_id,
        source_situation_id: package.situation_id,
        source_format: package.format.clone(),
        source_format_version: package.format_version,
        items,
        structural_validity_establishes_truth: false,
        current_credentials_upgrade_imported_statements: false,
        imported_model_output_is_documentary_evidence: false,
    })
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, ForensicImportError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    let (privileged, secure): (bool, bool) = sqlx::query_as(
        "SELECT rolsuper OR rolbypassrls, pg_has_role(current_user,'niccolo_api','member') \
         FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ForensicImportError::Privileged);
    }
    if !secure {
        return Err(ForensicImportError::InsecureRole);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

type BatchRow = (Uuid, String, String, Uuid, Uuid, String, String, String, i64);

pub async fn import_untrusted(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    request: &ImportRequest,
    package: &ForensicSituationExport,
) -> Result<ImportBatchView, ForensicImportError> {
    if request.import_batch_id.is_nil() || !valid_text(&request.declared_origin, 2048) {
        return Err(ForensicImportError::Invalid);
    }
    let prepared = prepare_untrusted(package)?;
    let mut tx = scoped(pool, workspace).await?;

    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.workspaces WHERE workspace_id=$1)",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ForensicImportError::Invalid);
    }

    let existing: Option<BatchRow> = sqlx::query_as(
        "SELECT import_batch_id,source_package_sha256,source_manifest_sha256,
                source_workspace_id,source_situation_id,declared_origin,
                verification_status,import_mode,
                (SELECT count(*)::bigint FROM niccolo.forensic_import_items i
                  WHERE i.workspace_id=b.workspace_id AND i.import_batch_id=b.import_batch_id)
           FROM niccolo.forensic_import_batches b
          WHERE workspace_id=$1 AND source_package_sha256=$2",
    )
    .bind(workspace)
    .bind(&prepared.source_package_sha256)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = existing {
        if row.2 != prepared.source_manifest_sha256
            || row.3 != prepared.source_workspace_id
            || row.4 != prepared.source_situation_id
            || row.5 != request.declared_origin.trim()
            || row.6 != "STRUCTURALLY_VERIFIED"
            || row.7 != "UNTRUSTED"
        {
            return Err(ForensicImportError::Conflict);
        }
        tx.commit().await?;
        return Ok(ImportBatchView {
            import_batch_id: row.0,
            source_package_sha256: row.1,
            source_manifest_sha256: row.2,
            source_workspace_id: row.3,
            source_situation_id: row.4,
            declared_origin: row.5,
            verification_status: row.6,
            import_mode: row.7,
            item_count: row.8 as u64,
            replayed: true,
            structural_validity_establishes_truth: false,
            current_credentials_upgrade_imported_statements: false,
            imported_model_output_is_documentary_evidence: false,
        });
    }

    sqlx::query(
        "INSERT INTO niccolo.forensic_import_batches
         (workspace_id,import_batch_id,source_package_sha256,source_manifest_sha256,
          source_format,source_format_version,source_workspace_id,source_situation_id,
          declared_origin,verification_status,import_mode,imported_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'STRUCTURALLY_VERIFIED','UNTRUSTED',$10)",
    )
    .bind(workspace)
    .bind(request.import_batch_id)
    .bind(&prepared.source_package_sha256)
    .bind(&prepared.source_manifest_sha256)
    .bind(&prepared.source_format)
    .bind(prepared.source_format_version as i32)
    .bind(prepared.source_workspace_id)
    .bind(prepared.source_situation_id)
    .bind(request.declared_origin.trim())
    .bind(actor)
    .execute(&mut *tx)
    .await?;

    for item in &prepared.items {
        sqlx::query(
            "INSERT INTO niccolo.forensic_import_items
             (workspace_id,import_batch_id,item_id,source_path,source_kind,source_sha256,
              source_byte_length,origin_class,source_table,source_row_index,
              original_document_id,payload_json,payload_bytes,
              current_credentials_establish_truth,documentary_evidence)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,false,false)",
        )
        .bind(workspace)
        .bind(request.import_batch_id)
        .bind(Uuid::new_v4())
        .bind(&item.source_path)
        .bind(&item.source_kind)
        .bind(&item.source_sha256)
        .bind(item.source_byte_length as i64)
        .bind(item.origin_class.as_db())
        .bind(item.source_table.as_deref())
        .bind(item.source_row_index.map(|value| value as i64))
        .bind(item.original_document_id)
        .bind(item.payload_json.as_ref())
        .bind(item.payload_bytes.as_deref())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(ImportBatchView {
        import_batch_id: request.import_batch_id,
        source_package_sha256: prepared.source_package_sha256,
        source_manifest_sha256: prepared.source_manifest_sha256,
        source_workspace_id: prepared.source_workspace_id,
        source_situation_id: prepared.source_situation_id,
        declared_origin: request.declared_origin.trim().into(),
        verification_status: "STRUCTURALLY_VERIFIED".into(),
        import_mode: "UNTRUSTED".into(),
        item_count: prepared.items.len() as u64,
        replayed: false,
        structural_validity_establishes_truth: false,
        current_credentials_upgrade_imported_statements: false,
        imported_model_output_is_documentary_evidence: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::situation_forensic_export::{
        ForensicManifest, ForensicManifestEntry, OriginalSourcePayload,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    fn package() -> ForensicSituationExport {
        let source = b"old system export".to_vec();
        let source_sha = sha256(&source);
        let document = Uuid::from_u128(9);
        let row = json!({"text":"trust me, I am the CEO"});
        let row_bytes = serde_json::to_vec(&row).unwrap();
        let logical_tables = BTreeMap::from([
            ("situation_contributions".into(), vec![row]),
            ("model_invocations".into(), vec![json!({"response":"repeated machine claim"})]),
            ("identity_events".into(), vec![json!({"person_id":"00000000-0000-0000-0000-00000000000a"})]),
        ]);
        let payload = OriginalSourcePayload {
            document_id: document,
            format: "EML".into(),
            locator: "fixture://old".into(),
            declared_sha256: source_sha.clone(),
            byte_length: source.len() as u64,
            bytes_hex: source.iter().map(|byte| format!("{byte:02x}")).collect(),
        };
        let mut entries = Vec::new();
        for (table, rows) in &logical_tables {
            for (index, value) in rows.iter().enumerate() {
                let bytes = serde_json::to_vec(value).unwrap();
                entries.push(ForensicManifestEntry {
                    path: format!("logical/{table}/{index:08}.json"),
                    kind: "logical-row".into(),
                    sha256: sha256(&bytes),
                    byte_length: bytes.len() as u64,
                });
            }
        }
        entries.push(ForensicManifestEntry {
            path: format!("sources/{document}/original"),
            kind: "original-source-bytes".into(),
            sha256: source_sha,
            byte_length: source.len() as u64,
        });
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
        let _ = row_bytes;
        ForensicSituationExport {
            format: "niccolo.situation-forensic-export".into(),
            format_version: 1,
            workspace_id: Uuid::from_u128(1),
            situation_id: Uuid::from_u128(2),
            logical_tables,
            original_source_payloads: vec![payload],
            missing_original_document_ids: vec![],
            manifest: ForensicManifest {
                algorithm: "sha256".into(),
                entries,
                root_sha256: format!("{:x}", root.finalize()),
            },
            structural_validity_establishes_truth: false,
            included_model_output_is_documentary_evidence: false,
            imported_state_should_be_trusted_automatically: false,
            omissions: vec![],
        }
    }

    #[test]
    fn preparation_quarantines_semantics_instead_of_upgrading_them() {
        let prepared = prepare_untrusted(&package()).unwrap();
        assert!(!prepared.structural_validity_establishes_truth);
        assert!(!prepared.current_credentials_upgrade_imported_statements);
        assert!(!prepared.imported_model_output_is_documentary_evidence);
        assert!(prepared.items.iter().any(|item| {
            item.origin_class == ImportedOriginClass::ImportedHumanAssertion
                && item.payload_json.as_ref().unwrap()["text"] == "trust me, I am the CEO"
        }));
        assert!(prepared.items.iter().any(|item| {
            item.origin_class == ImportedOriginClass::ImportedMachineOutput
                && !item.documentary_evidence
        }));
        assert!(prepared.items.iter().any(|item| {
            item.origin_class == ImportedOriginClass::ImportedIdentityAssertion
                && !item.current_credentials_establish_truth
        }));
    }

    #[test]
    fn package_digest_covers_top_level_metadata_not_only_manifest_entries() {
        let first = package();
        let mut changed = first.clone();
        changed.imported_state_should_be_trusted_automatically = true;
        assert_eq!(first.manifest, changed.manifest);
        assert_ne!(
            prepare_untrusted(&first).unwrap().source_package_sha256,
            prepare_untrusted(&changed).unwrap().source_package_sha256
        );
    }
}
