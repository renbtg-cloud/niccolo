//! Deterministic, bounded export package for one durable operation checkpoint.
//!
//! This is a local/exportable progress artifact and deliberately performs no
//! remote publication. A provisional checkpoint remains provisional after export.
use crate::{
    bff_operations::OperationScope, checkpoint_milestone::MilestoneView,
    operation_checkpoint::CheckpointView,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

const FORMAT: &str = "niccolo.operation-checkpoint-package";
const MAX_PACKAGE_BYTES: usize = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CheckpointPackageError {
    #[error("invalid checkpoint package input")]
    Invalid,
    #[error("checkpoint package serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("checkpoint package exceeds the configured byte budget")]
    TooLarge,
    #[error("checkpoint summary integrity mismatch")]
    Integrity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPackage {
    pub format: String,
    pub version: u32,
    pub scope_kind: String,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub operation_id: Uuid,
    pub checkpoint_id: Uuid,
    pub checkpoint_seq: i64,
    pub analysis_id: Option<Uuid>,
    pub checkpoint_kind: String,
    pub provisional: bool,
    pub terminal: bool,
    pub checkpoint_summary_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone_reason: Option<String>,
    pub files: BTreeMap<String, String>,
    pub includes_original_uploads: bool,
    pub includes_examined_source_text: bool,
    pub published_to_git: bool,
    pub establishes_truth: bool,
    pub proves_completeness: bool,
    pub authorizes_real_world_action: bool,
}

#[derive(Serialize)]
struct CheckpointDocument<'a> {
    format: &'static str,
    version: u32,
    workspace_id: Uuid,
    situation_id: Uuid,
    operation_id: Uuid,
    checkpoint: &'a CheckpointView,
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(fields) => {
            let sorted = fields
                .iter()
                .map(|(key, value)| (key.clone(), canonical(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        scalar => scalar.clone(),
    }
}

fn summary_digest(summary: &Value) -> Result<String, serde_json::Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical(summary))?)
    ))
}

fn file_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn package_bytes(files: &BTreeMap<String, String>) -> usize {
    files
        .iter()
        .map(|(path, value)| path.len() + value.len())
        .sum()
}

fn finalize_files(
    mut files: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, CheckpointPackageError> {
    files.remove("SHA256SUMS");
    let sums = files
        .iter()
        .map(|(path, value)| format!("{}  {}", file_digest(value), path))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    files.insert("SHA256SUMS".to_owned(), sums);
    if package_bytes(&files) > MAX_PACKAGE_BYTES {
        return Err(CheckpointPackageError::TooLarge);
    }
    Ok(files)
}

pub fn package_sha256(package: &CheckpointPackage) -> String {
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
    scope: OperationScope,
    checkpoint: &CheckpointView,
) -> Result<CheckpointPackage, CheckpointPackageError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.operation.is_nil()
        || checkpoint.checkpoint_id.is_nil()
        || checkpoint.checkpoint_seq <= 0
        || checkpoint.recorded_by_principal_id.is_nil()
        || !valid_sha256(&checkpoint.summary_sha256)
        || checkpoint.establishes_truth
        || checkpoint.proves_completeness
        || checkpoint.authorizes_real_world_action
    {
        return Err(CheckpointPackageError::Invalid);
    }
    if summary_digest(&checkpoint.summary)? != checkpoint.summary_sha256 {
        return Err(CheckpointPackageError::Integrity);
    }

    let mut normalized = checkpoint.clone();
    normalized.summary = canonical(&normalized.summary);
    let document = CheckpointDocument {
        format: FORMAT,
        version: 1,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        operation_id: scope.operation,
        checkpoint: &normalized,
    };
    let checkpoint_json = serde_json::to_string_pretty(&document)? + "\n";
    let readme = format!(
        "# Niccolò operation checkpoint\n\nThis package preserves checkpoint {} (sequence {}) from operation {}.\n\n- Kind: {}\n- Provisional: {}\n- Terminal: {}\n- Establishes truth: no\n- Proves completeness: no\n- Authorizes real-world action: no\n- Remote publication performed: no\n\nThe checkpoint summary is an immutable progress observation. Exporting it does not upgrade its evidentiary status or imply that the investigation is complete.\n",
        checkpoint.checkpoint_id,
        checkpoint.checkpoint_seq,
        scope.operation,
        checkpoint.checkpoint_kind,
        checkpoint.provisional,
        checkpoint.terminal,
    );
    let mut files = BTreeMap::new();
    files.insert("README.md".to_owned(), readme);
    files.insert("checkpoint.json".to_owned(), checkpoint_json);
    let files = finalize_files(files)?;

    Ok(CheckpointPackage {
        format: FORMAT.to_owned(),
        version: 1,
        scope_kind: "OPERATION_CHECKPOINT".to_owned(),
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        operation_id: scope.operation,
        checkpoint_id: checkpoint.checkpoint_id,
        checkpoint_seq: checkpoint.checkpoint_seq,
        analysis_id: checkpoint.analysis_id,
        checkpoint_kind: checkpoint.checkpoint_kind.clone(),
        provisional: checkpoint.provisional,
        terminal: checkpoint.terminal,
        checkpoint_summary_sha256: checkpoint.summary_sha256.clone(),
        milestone_id: None,
        milestone_kind: None,
        milestone_reason: None,
        files,
        includes_original_uploads: false,
        includes_examined_source_text: false,
        published_to_git: false,
        establishes_truth: false,
        proves_completeness: false,
        authorizes_real_world_action: false,
    })
}

pub fn build_milestone(
    scope: OperationScope,
    checkpoint: &CheckpointView,
    milestone: &MilestoneView,
) -> Result<CheckpointPackage, CheckpointPackageError> {
    if milestone.workspace_id != scope.workspace
        || milestone.situation_id != scope.situation
        || milestone.operation_id != scope.operation
        || milestone.checkpoint_id != checkpoint.checkpoint_id
        || milestone.checkpoint_seq != checkpoint.checkpoint_seq
        || milestone.checkpoint_kind != checkpoint.checkpoint_kind
        || milestone.checkpoint_provisional != checkpoint.provisional
        || milestone.checkpoint_terminal != checkpoint.terminal
        || milestone.checkpoint_summary_sha256 != checkpoint.summary_sha256
        || milestone.milestone_id.is_nil()
        || milestone.milestone_kind.is_empty()
        || milestone.reason.trim().is_empty()
        || milestone.reason != milestone.reason.trim()
        || milestone.remote_publish_performed
        || milestone.establishes_truth
        || milestone.proves_completeness
        || milestone.authorizes_real_world_action
    {
        return Err(CheckpointPackageError::Invalid);
    }

    let mut package = build(scope, checkpoint)?;
    package.scope_kind = "OPERATION_MILESTONE".to_owned();
    package.milestone_id = Some(milestone.milestone_id);
    package.milestone_kind = Some(milestone.milestone_kind.clone());
    package.milestone_reason = Some(milestone.reason.clone());

    let milestone_json = serde_json::to_string_pretty(milestone)? + "\n";
    let readme = format!(
        "# Niccolò operation milestone\n\nThis package preserves a human-declared meaningful milestone for checkpoint {} (sequence {}) from operation {}.\n\n- Milestone kind: {}\n- Why it matters: {}\n- Checkpoint kind: {}\n- Provisional: {}\n- Terminal: {}\n- Establishes truth: no\n- Proves completeness: no\n- Authorizes real-world action: no\n- Remote publication performed by package construction: no\n\nThe milestone declaration records why a human chose to preserve this checkpoint. It does not upgrade the checkpoint's evidentiary or epistemic status.\n",
        checkpoint.checkpoint_id,
        checkpoint.checkpoint_seq,
        scope.operation,
        milestone.milestone_kind,
        milestone.reason,
        checkpoint.checkpoint_kind,
        checkpoint.provisional,
        checkpoint.terminal,
    );
    package.files.insert("README.md".to_owned(), readme);
    package
        .files
        .insert("milestone.json".to_owned(), milestone_json);
    package.files = finalize_files(package.files)?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope() -> OperationScope {
        OperationScope {
            workspace: Uuid::from_u128(1),
            situation: Uuid::from_u128(2),
            operation: Uuid::from_u128(3),
        }
    }

    fn checkpoint(summary: Value) -> CheckpointView {
        let sha = summary_digest(&summary).unwrap();
        CheckpointView {
            checkpoint_id: Uuid::from_u128(4),
            checkpoint_seq: 7,
            analysis_id: Some(Uuid::from_u128(5)),
            checkpoint_kind: "STEP_COMPLETED".into(),
            step_id: Some(Uuid::from_u128(6)),
            summary,
            summary_sha256: sha,
            provisional: true,
            terminal: false,
            recorded_by_principal_id: Uuid::from_u128(7),
            created_at: "2026-11-12 12:00:00+00".into(),
            establishes_truth: false,
            proves_completeness: false,
            authorizes_real_world_action: false,
        }
    }

    #[test]
    fn package_is_deterministic_across_object_key_order() {
        let a = checkpoint(json!({"z":2,"a":{"y":4,"b":3}}));
        let b = checkpoint(json!({"a":{"b":3,"y":4},"z":2}));
        let pa = build(scope(), &a).unwrap();
        let pb = build(scope(), &b).unwrap();
        assert_eq!(pa.files, pb.files);
        assert_eq!(package_sha256(&pa), package_sha256(&pb));
    }

    #[test]
    fn package_keeps_provisional_safety_boundary_explicit() {
        let package = build(scope(), &checkpoint(json!({"progress":"bounded"}))).unwrap();
        assert_eq!(package.scope_kind, "OPERATION_CHECKPOINT");
        assert!(package.provisional);
        assert!(!package.published_to_git);
        assert!(!package.establishes_truth);
        assert!(!package.proves_completeness);
        assert!(!package.authorizes_real_world_action);
        assert!(!package.includes_original_uploads);
        assert!(!package.includes_examined_source_text);
        assert!(package.files["README.md"].contains("Remote publication performed: no"));
    }

    fn milestone(item: &CheckpointView) -> MilestoneView {
        MilestoneView {
            milestone_id: Uuid::from_u128(8),
            workspace_id: Uuid::from_u128(1),
            situation_id: Uuid::from_u128(2),
            operation_id: Uuid::from_u128(3),
            checkpoint_id: item.checkpoint_id,
            checkpoint_seq: item.checkpoint_seq,
            checkpoint_kind: item.checkpoint_kind.clone(),
            checkpoint_provisional: item.provisional,
            checkpoint_terminal: item.terminal,
            checkpoint_summary_sha256: item.summary_sha256.clone(),
            milestone_kind: "FIRST_PASS".into(),
            reason: "First coherent pass worth preserving".into(),
            declared_by_principal_id: Uuid::from_u128(9),
            created_at: "2026-11-12 12:05:00+00".into(),
            remote_publish_performed: false,
            establishes_truth: false,
            proves_completeness: false,
            authorizes_real_world_action: false,
        }
    }

    #[test]
    fn milestone_package_preserves_reason_without_upgrading_checkpoint() {
        let item = checkpoint(json!({"progress":"bounded"}));
        let package = build_milestone(scope(), &item, &milestone(&item)).unwrap();
        assert_eq!(package.scope_kind, "OPERATION_MILESTONE");
        assert_eq!(package.milestone_id, Some(Uuid::from_u128(8)));
        assert_eq!(package.milestone_kind.as_deref(), Some("FIRST_PASS"));
        assert!(package.provisional);
        assert!(!package.establishes_truth);
        assert!(!package.proves_completeness);
        assert!(!package.authorizes_real_world_action);
        assert!(package.files.contains_key("milestone.json"));
        assert!(package.files["README.md"].contains("does not upgrade"));
        assert!(package.files["SHA256SUMS"].contains("milestone.json"));
    }

    #[test]
    fn altered_summary_is_rejected() {
        let mut item = checkpoint(json!({"progress":"bounded"}));
        item.summary = json!({"progress":"changed"});
        assert!(matches!(
            build(scope(), &item),
            Err(CheckpointPackageError::Integrity)
        ));
    }
}
