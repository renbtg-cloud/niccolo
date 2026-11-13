//! Deterministic, bounded receipt package for one ingested source document.
//!
//! This package says only that Niccolò received byte content with a particular
//! digest and parsed it into a bounded number of fragments. It deliberately
//! excludes the original bytes, source text, locator, analysis, claims and any
//! assertion of authenticity, truth, completeness or corroboration.
use crate::source_catalog::SourceMetadata;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

const FORMAT: &str = "niccolo.intake-receipt-package";
const MAX_PACKAGE_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntakePackageError {
    #[error("invalid intake package input")]
    Invalid,
    #[error("intake package serialization failed: {0}")]
    Json(String),
    #[error("intake package exceeds the configured byte budget")]
    TooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakeReceipt {
    pub format: String,
    pub version: u32,
    pub state: String,
    pub workspace_id: Uuid,
    pub document_id: Uuid,
    pub source_format: String,
    pub source_sha256: String,
    pub source_byte_length: i64,
    pub parsed_fragment_count: i64,
    pub original_bytes_retained_by_niccolo: bool,
    pub includes_original_upload: bool,
    pub includes_source_text: bool,
    pub includes_analysis: bool,
    pub establishes_authenticity: bool,
    pub establishes_truth: bool,
    pub proves_completeness: bool,
    pub proves_independence: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntakePackage {
    pub receipt: IntakeReceipt,
    pub files: BTreeMap<String, String>,
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn file_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn package_bytes(files: &BTreeMap<String, String>) -> usize {
    files
        .iter()
        .map(|(path, value)| path.len() + value.len())
        .sum()
}

fn finalize_files(
    mut files: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, IntakePackageError> {
    let sums = files
        .iter()
        .map(|(path, value)| format!("{}  {}", file_digest(value), path))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    files.insert("SHA256SUMS".into(), sums);
    if package_bytes(&files) > MAX_PACKAGE_BYTES {
        return Err(IntakePackageError::TooLarge);
    }
    Ok(files)
}

pub fn package_sha256(package: &IntakePackage) -> String {
    let mut digest = Sha256::new();
    for (path, value) in &package.files {
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

pub fn build(
    workspace_id: Uuid,
    source: &SourceMetadata,
) -> Result<IntakePackage, IntakePackageError> {
    if workspace_id.is_nil()
        || source.document_id.is_nil()
        || !matches!(source.format.as_str(), "EML" | "WHATSAPP_EXPORT")
        || !valid_sha256(&source.sha256)
        || !(1..=4 * 1024 * 1024).contains(&source.byte_length)
        || !(1..=10_000).contains(&source.fragment_count)
    {
        return Err(IntakePackageError::Invalid);
    }
    let receipt = IntakeReceipt {
        format: FORMAT.into(),
        version: 1,
        state: "RECEIVED".into(),
        workspace_id,
        document_id: source.document_id,
        source_format: source.format.clone(),
        source_sha256: source.sha256.clone(),
        source_byte_length: source.byte_length,
        parsed_fragment_count: source.fragment_count,
        original_bytes_retained_by_niccolo: source.original_bytes_available,
        includes_original_upload: false,
        includes_source_text: false,
        includes_analysis: false,
        establishes_authenticity: false,
        establishes_truth: false,
        proves_completeness: false,
        proves_independence: false,
    };
    let receipt_json = serde_json::to_string_pretty(&receipt)
        .map_err(|error| IntakePackageError::Json(error.to_string()))?
        + "\n";
    let readme = format!(
        "# Niccolò intake receipt\n\nState: **RECEIVED**\n\nThis package records that Niccolò ingested source document `{}` with SHA-256 `{}`.\n\n- Source format: {}\n- Uploaded byte length: {}\n- Parsed fragment count: {}\n- Original bytes retained by Niccolò: {}\n- Original upload included here: no\n- Source text included here: no\n- Analysis included here: no\n- Authenticity established: no\n- Truth established: no\n- Completeness proved: no\n- Independent corroboration proved: no\n\nA receipt is an operational history artifact. It does not validate who created the source, whether its contents are true, whether collection was complete, or whether later analysis accepted any statement in it.\n",
        source.document_id,
        source.sha256,
        source.format,
        source.byte_length,
        source.fragment_count,
        if source.original_bytes_available { "yes" } else { "no" },
    );
    let mut files = BTreeMap::new();
    files.insert("README.md".into(), readme);
    files.insert("intake.json".into(), receipt_json);
    let files = finalize_files(files)?;
    Ok(IntakePackage { receipt, files })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceMetadata {
        SourceMetadata {
            document_id: Uuid::from_u128(2),
            format: "EML".into(),
            locator: "/very/private/path/client-name.eml".into(),
            sha256: "a".repeat(64),
            byte_length: 321,
            fragment_count: 3,
            original_bytes_available: true,
        }
    }

    #[test]
    fn receipt_is_deterministic_and_contains_no_source_or_locator() {
        let source = source();
        let a = build(Uuid::from_u128(1), &source).unwrap();
        let b = build(Uuid::from_u128(1), &source).unwrap();
        assert_eq!(a, b);
        assert_eq!(package_sha256(&a), package_sha256(&b));
        let joined = a.files.values().cloned().collect::<String>();
        assert!(!joined.contains(&source.locator));
        assert!(!joined.contains("client-name"));
        assert!(a.files.contains_key("SHA256SUMS"));
    }

    #[test]
    fn received_never_means_authentic_true_complete_or_analyzed() {
        let package = build(Uuid::from_u128(1), &source()).unwrap();
        assert_eq!(package.receipt.state, "RECEIVED");
        assert!(!package.receipt.includes_original_upload);
        assert!(!package.receipt.includes_source_text);
        assert!(!package.receipt.includes_analysis);
        assert!(!package.receipt.establishes_authenticity);
        assert!(!package.receipt.establishes_truth);
        assert!(!package.receipt.proves_completeness);
        assert!(!package.receipt.proves_independence);
        assert!(package.files["README.md"].contains("does not validate"));
    }

    #[test]
    fn malformed_source_metadata_is_rejected() {
        let mut bad = source();
        bad.sha256 = "A".repeat(64);
        assert_eq!(
            build(Uuid::from_u128(1), &bad),
            Err(IntakePackageError::Invalid)
        );
        bad = source();
        bad.fragment_count = 0;
        assert_eq!(
            build(Uuid::from_u128(1), &bad),
            Err(IntakePackageError::Invalid)
        );
    }
}
