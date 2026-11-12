//! Explicit GitHub publication transport for a previously recorded publication intent.
//!
//! This module performs the remote Git object sequence only when called by an
//! explicit higher-level action. Provider credentials are transient inputs:
//! they are never serialized into a domain object, database row, URL or error.
//! PostgreSQL remains authoritative until the caller records a verified remote
//! ref observation after this function returns successfully.
use crate::{
    git_publication::{PublicationIntentView, TargetView},
    presentation_package::{self, PresentationPackage},
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

const MAX_PROVIDER_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_REMOTE_PACKAGE_FILES: usize = 256;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum GitHubPublishError {
    #[error("invalid GitHub publication request")]
    Invalid,
    #[error("local publication package no longer matches the recorded intent")]
    LocalIntegrity,
    #[error("GitHub transport failed")]
    Transport,
    #[error("GitHub rejected the publication request with HTTP {0}")]
    ProviderRejected(u16),
    #[error("GitHub ref verification did not match the pushed commit")]
    VerificationMismatch,
    #[error("GitHub response was structurally invalid")]
    InvalidProviderResponse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedGitHubPublication {
    pub commit_sha: String,
    pub ref_name: String,
    pub repository_full_name: String,
    pub package_sha256: String,
    pub already_present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedGitHubTarget {
    pub repository_full_name: String,
    pub ref_name: String,
    pub head_commit_sha: Option<String>,
    pub repository_private: bool,
    pub requested_ref_exists: bool,
    pub creation_base_ref: Option<String>,
    pub creation_base_commit_sha: Option<String>,
}

pub trait GitHubTransport {
    fn json(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Option<&Value>,
    ) -> Result<Value, GitHubPublishError>;
}

fn valid_token(token: &str) -> bool {
    !token.trim().is_empty() && token.len() <= 4096 && !token.contains(['\r', '\n', '\0'])
}

fn branch_name(ref_name: &str) -> Result<&str, GitHubPublishError> {
    let name = ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name);
    if name.is_empty()
        || name.len() > 240
        || name.starts_with('/')
        || name.starts_with('.')
        || name.ends_with('/')
        || name.ends_with('.')
        || name.ends_with(".lock")
        || name.contains("..")
        || name.contains("@{")
        || name.contains("//")
        || name.bytes().any(|byte| {
            byte.is_ascii_control()
                || matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
    {
        return Err(GitHubPublishError::Invalid);
    }
    Ok(name)
}

fn safe_package_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn response_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, GitHubPublishError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(GitHubPublishError::InvalidProviderResponse)
}

fn total_text_bytes(package: &PresentationPackage) -> Result<u64, GitHubPublishError> {
    package.files.values().try_fold(0_u64, |total, value| {
        total
            .checked_add(value.len() as u64)
            .ok_or(GitHubPublishError::LocalIntegrity)
    })
}

fn validate_local_material(
    target: &TargetView,
    intent: &PublicationIntentView,
    package: &PresentationPackage,
    token: &str,
) -> Result<String, GitHubPublishError> {
    if !valid_token(token)
        || target.provider != "GITHUB"
        || target.authorization_mode != "TRANSIENT_PROVIDER_AUTHORIZATION"
        || target.provider_secret_stored
        || intent.target_id != target.target_id
        || intent.requested_ref != target.ref_name
        || intent.scope_kind != "STANDARD_PRESENTATION"
        || intent.publication_state != "AWAITING_REMOTE_AUTHORIZATION"
        || intent.includes_examined_source_text
        || intent.includes_original_uploads
        || package.analysis_id != intent.analysis_id
        || package.includes_original_source_fragment_contents
        || package.includes_examined_source_fragment_contents
        || package.published_to_git
        || package.files.is_empty()
        || package.files.len() > MAX_REMOTE_PACKAGE_FILES
        || package.files.len() != intent.file_count as usize
        || total_text_bytes(package)? != intent.total_text_bytes
        || package.files.keys().any(|path| !safe_package_path(path))
    {
        return Err(GitHubPublishError::LocalIntegrity);
    }
    let digest = presentation_package::package_sha256(package)
        .map_err(|_| GitHubPublishError::LocalIntegrity)?;
    if digest != intent.package_sha256 {
        return Err(GitHubPublishError::LocalIntegrity);
    }
    branch_name(&target.ref_name).map(str::to_owned)
}

fn repository_visibility_matches(target: &TargetView, private: bool) -> bool {
    match target.repository_visibility.as_str() {
        "PRIVATE" => private,
        "PUBLIC" => !private,
        "UNKNOWN" => true,
        _ => false,
    }
}

fn expected_commit_message(intent: &PublicationIntentView) -> String {
    format!(
        "Niccolo publication {} package {}",
        intent.publication_id, intent.package_sha256
    )
}

pub fn verify_target<T: GitHubTransport>(
    transport: &T,
    target: &TargetView,
    token: &str,
) -> Result<VerifiedGitHubTarget, GitHubPublishError> {
    if !valid_token(token)
        || target.provider != "GITHUB"
        || target.authorization_mode != "TRANSIENT_PROVIDER_AUTHORIZATION"
        || target.provider_secret_stored
    {
        return Err(GitHubPublishError::Invalid);
    }
    let branch = branch_name(&target.ref_name)?;
    let repository = format!("{}/{}", target.repository_owner, target.repository_name);
    let repo_path = format!("/repos/{repository}");
    let repo = transport.json("GET", &repo_path, token, None)?;
    let remote_full_name = response_string(&repo, "/full_name")?;
    let private = repo
        .get("private")
        .and_then(Value::as_bool)
        .ok_or(GitHubPublishError::InvalidProviderResponse)?;
    if !remote_full_name.eq_ignore_ascii_case(&repository)
        || !repository_visibility_matches(target, private)
    {
        return Err(GitHubPublishError::VerificationMismatch);
    }

    match transport.json(
        "GET",
        &format!("{repo_path}/git/ref/heads/{branch}"),
        token,
        None,
    ) {
        Ok(current_ref) => {
            let head_commit_sha = response_string(&current_ref, "/object/sha")?.to_owned();
            if !valid_remote_sha(&head_commit_sha) {
                return Err(GitHubPublishError::InvalidProviderResponse);
            }
            Ok(VerifiedGitHubTarget {
                repository_full_name: remote_full_name.to_owned(),
                ref_name: branch.to_owned(),
                head_commit_sha: Some(head_commit_sha),
                repository_private: private,
                requested_ref_exists: true,
                creation_base_ref: None,
                creation_base_commit_sha: None,
            })
        }
        Err(GitHubPublishError::ProviderRejected(404)) => {
            let default_branch = response_string(&repo, "/default_branch")?;
            let default_branch = branch_name(default_branch)?;
            if default_branch == branch {
                return Err(GitHubPublishError::InvalidProviderResponse);
            }
            let default_ref = transport.json(
                "GET",
                &format!("{repo_path}/git/ref/heads/{default_branch}"),
                token,
                None,
            )?;
            let base_sha = response_string(&default_ref, "/object/sha")?.to_owned();
            if !valid_remote_sha(&base_sha) {
                return Err(GitHubPublishError::InvalidProviderResponse);
            }
            Ok(VerifiedGitHubTarget {
                repository_full_name: remote_full_name.to_owned(),
                ref_name: branch.to_owned(),
                head_commit_sha: None,
                repository_private: private,
                requested_ref_exists: false,
                creation_base_ref: Some(default_branch.to_owned()),
                creation_base_commit_sha: Some(base_sha),
            })
        }
        Err(error) => Err(error),
    }
}

pub fn publish_standard_package<T: GitHubTransport>(
    transport: &T,
    target: &TargetView,
    intent: &PublicationIntentView,
    package: &PresentationPackage,
    token: &str,
) -> Result<VerifiedGitHubPublication, GitHubPublishError> {
    let branch = validate_local_material(target, intent, package, token)?;
    let repository = format!("{}/{}", target.repository_owner, target.repository_name);
    let repo_path = format!("/repos/{repository}");
    let repo = transport.json("GET", &repo_path, token, None)?;
    let remote_full_name = response_string(&repo, "/full_name")?;
    let private = repo
        .get("private")
        .and_then(Value::as_bool)
        .ok_or(GitHubPublishError::InvalidProviderResponse)?;
    if !remote_full_name.eq_ignore_ascii_case(&repository)
        || !repository_visibility_matches(target, private)
    {
        return Err(GitHubPublishError::VerificationMismatch);
    }

    let ref_path = format!("{repo_path}/git/ref/heads/{branch}");
    let (base_commit_sha, create_requested_ref) =
        match transport.json("GET", &ref_path, token, None) {
            Ok(current_ref) => (
                response_string(&current_ref, "/object/sha")?.to_owned(),
                false,
            ),
            Err(GitHubPublishError::ProviderRejected(404)) => {
                let default_branch = response_string(&repo, "/default_branch")?;
                let default_branch = branch_name(default_branch)?;
                if default_branch == branch {
                    return Err(GitHubPublishError::InvalidProviderResponse);
                }
                let default_ref = transport.json(
                    "GET",
                    &format!("{repo_path}/git/ref/heads/{default_branch}"),
                    token,
                    None,
                )?;
                (
                    response_string(&default_ref, "/object/sha")?.to_owned(),
                    true,
                )
            }
            Err(error) => return Err(error),
        };
    let base_commit = transport.json(
        "GET",
        &format!("{repo_path}/git/commits/{base_commit_sha}"),
        token,
        None,
    )?;
    let base_tree_sha = response_string(&base_commit, "/tree/sha")?.to_owned();
    let base_message = base_commit
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let prefix = format!(
        "niccolo/situations/{}/analyses/{}",
        intent.situation_id, intent.analysis_id
    );
    let mut tree_entries = Vec::with_capacity(package.files.len());
    for (path, content) in &package.files {
        let blob = transport.json(
            "POST",
            &format!("{repo_path}/git/blobs"),
            token,
            Some(&json!({"content":content,"encoding":"utf-8"})),
        )?;
        let sha = response_string(&blob, "/sha")?;
        tree_entries.push(json!({
            "path": format!("{prefix}/{path}"),
            "mode": "100644",
            "type": "blob",
            "sha": sha
        }));
    }
    let tree = transport.json(
        "POST",
        &format!("{repo_path}/git/trees"),
        token,
        Some(&json!({"base_tree":base_tree_sha,"tree":tree_entries})),
    )?;
    let tree_sha = response_string(&tree, "/sha")?.to_owned();
    let commit_message = expected_commit_message(intent);

    // A prior attempt may have updated the ref and lost its HTTP response before
    // PostgreSQL could record verification. Recreating blobs/tree is harmless;
    // if the current head already has the exact tree+message, do not make a
    // duplicate commit.
    if !create_requested_ref && tree_sha == base_tree_sha && base_message == commit_message {
        return Ok(VerifiedGitHubPublication {
            commit_sha: base_commit_sha,
            ref_name: branch,
            repository_full_name: remote_full_name.to_owned(),
            package_sha256: intent.package_sha256.clone(),
            already_present: true,
        });
    }

    let commit = transport.json(
        "POST",
        &format!("{repo_path}/git/commits"),
        token,
        Some(&json!({
            "message":commit_message,
            "tree":tree_sha,
            "parents":[base_commit_sha]
        })),
    )?;
    let commit_sha = response_string(&commit, "/sha")?.to_owned();
    if !valid_remote_sha(&commit_sha) {
        return Err(GitHubPublishError::InvalidProviderResponse);
    }

    let updated = if create_requested_ref {
        transport.json(
            "POST",
            &format!("{repo_path}/git/refs"),
            token,
            Some(&json!({"ref":format!("refs/heads/{branch}"),"sha":commit_sha})),
        )?
    } else {
        transport.json(
            "PATCH",
            &format!("{repo_path}/git/refs/heads/{branch}"),
            token,
            Some(&json!({"sha":commit_sha,"force":false})),
        )?
    };
    if response_string(&updated, "/object/sha")? != commit_sha {
        return Err(GitHubPublishError::VerificationMismatch);
    }
    let verified = transport.json("GET", &ref_path, token, None)?;
    if response_string(&verified, "/object/sha")? != commit_sha {
        return Err(GitHubPublishError::VerificationMismatch);
    }
    let verified_commit = transport.json(
        "GET",
        &format!("{repo_path}/git/commits/{commit_sha}"),
        token,
        None,
    )?;
    if response_string(&verified_commit, "/tree/sha")? != tree_sha
        || verified_commit
            .get("message")
            .and_then(Value::as_str)
            .is_none_or(|message| message != commit_message)
    {
        return Err(GitHubPublishError::VerificationMismatch);
    }

    Ok(VerifiedGitHubPublication {
        commit_sha,
        ref_name: branch,
        repository_full_name: remote_full_name.to_owned(),
        package_sha256: intent.package_sha256.clone(),
        already_present: false,
    })
}

fn valid_remote_sha(value: &str) -> bool {
    (40..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn private_temp(prefix: &str) -> Result<PathBuf, GitHubPublishError> {
    let path = std::env::temp_dir().join(format!("niccolo-{prefix}-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .map_err(|_| GitHubPublishError::Transport)?;
    Ok(path)
}

fn remove_private(path: &Path) {
    let _ = fs::remove_file(path);
}

#[derive(Clone, Debug)]
pub struct CurlGitHubTransport {
    curl_bin: String,
    max_time_secs: u64,
}

impl Default for CurlGitHubTransport {
    fn default() -> Self {
        Self {
            curl_bin: "curl".into(),
            max_time_secs: 25,
        }
    }
}

impl CurlGitHubTransport {
    pub fn new(curl_bin: &str, max_time_secs: u64) -> Result<Self, GitHubPublishError> {
        if curl_bin.trim().is_empty()
            || curl_bin.contains(['\r', '\n', '\0'])
            || !(1..=60).contains(&max_time_secs)
        {
            return Err(GitHubPublishError::Invalid);
        }
        Ok(Self {
            curl_bin: curl_bin.into(),
            max_time_secs,
        })
    }
}

impl GitHubTransport for CurlGitHubTransport {
    fn json(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Option<&Value>,
    ) -> Result<Value, GitHubPublishError> {
        if !matches!(method, "GET" | "POST" | "PATCH")
            || !path.starts_with("/repos/")
            || path.contains(['\r', '\n', '\0', ' '])
            || !valid_token(token)
        {
            return Err(GitHubPublishError::Invalid);
        }
        let header_path = private_temp("github-headers")?;
        let body_path = private_temp("github-request")?;
        let response_path = private_temp("github-response")?;
        let cleanup = || {
            remove_private(&header_path);
            remove_private(&body_path);
            remove_private(&response_path);
        };
        let run = (|| {
            fs::write(
                &header_path,
                format!(
                    "Accept: application/vnd.github+json\nAuthorization: Bearer {token}\nX-GitHub-Api-Version: 2026-03-10\nContent-Type: application/json\n"
                ),
            )
            .map_err(|_| GitHubPublishError::Transport)?;
            if let Some(body) = body {
                fs::write(
                    &body_path,
                    serde_json::to_vec(body).map_err(|_| GitHubPublishError::Invalid)?,
                )
                .map_err(|_| GitHubPublishError::Transport)?;
            }
            let header_arg = format!("@{}", header_path.display());
            let body_arg = format!("@{}", body_path.display());
            let url = format!("https://api.github.com{path}");
            let mut command = Command::new(&self.curl_bin);
            command.args([
                "--silent",
                "--show-error",
                "--max-time",
                &self.max_time_secs.to_string(),
                "--connect-timeout",
                &self.max_time_secs.min(10).to_string(),
                "--max-filesize",
                &MAX_PROVIDER_RESPONSE_BYTES.to_string(),
                "--request",
                method,
                "--header",
                &header_arg,
                "--output",
                response_path
                    .to_str()
                    .ok_or(GitHubPublishError::Transport)?,
                "--write-out",
                "%{http_code}",
            ]);
            if body.is_some() {
                command.args(["--data-binary", &body_arg]);
            }
            command.arg(url);
            let output = command
                .output()
                .map_err(|_| GitHubPublishError::Transport)?;
            if !output.status.success() {
                return Err(GitHubPublishError::Transport);
            }
            let status_text =
                std::str::from_utf8(&output.stdout).map_err(|_| GitHubPublishError::Transport)?;
            let status = status_text
                .trim()
                .parse::<u16>()
                .map_err(|_| GitHubPublishError::Transport)?;
            if !(200..300).contains(&status) {
                return Err(GitHubPublishError::ProviderRejected(status));
            }
            let metadata =
                fs::metadata(&response_path).map_err(|_| GitHubPublishError::Transport)?;
            if metadata.len() > MAX_PROVIDER_RESPONSE_BYTES {
                return Err(GitHubPublishError::Transport);
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            OpenOptions::new()
                .read(true)
                .open(&response_path)
                .map_err(|_| GitHubPublishError::Transport)?
                .take(MAX_PROVIDER_RESPONSE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| GitHubPublishError::Transport)?;
            if bytes.len() as u64 > MAX_PROVIDER_RESPONSE_BYTES {
                return Err(GitHubPublishError::Transport);
            }
            serde_json::from_slice(&bytes).map_err(|_| GitHubPublishError::InvalidProviderResponse)
        })();
        cleanup();
        run
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_publication::{PublicationIntentView, TargetView};
    use std::{collections::VecDeque, sync::Mutex};

    struct ScriptedTransport {
        responses: Mutex<VecDeque<Value>>,
        calls: Mutex<Vec<(String, String, Option<Value>)>>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Value>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl GitHubTransport for ScriptedTransport {
        fn json(
            &self,
            method: &str,
            path: &str,
            token: &str,
            body: Option<&Value>,
        ) -> Result<Value, GitHubPublishError> {
            assert_eq!(token, "test-secret-token");
            self.calls
                .lock()
                .unwrap()
                .push((method.into(), path.into(), body.cloned()));
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(GitHubPublishError::Transport)
        }
    }

    fn target() -> TargetView {
        TargetView {
            target_id: Uuid::from_u128(1),
            provider: "GITHUB".into(),
            repository_owner: "owner".into(),
            repository_name: "history".into(),
            ref_name: "main".into(),
            repository_visibility: "PRIVATE".into(),
            sensitive_exports_allowed: false,
            authorization_mode: "TRANSIENT_PROVIDER_AUTHORIZATION".into(),
            created_by_principal_id: Uuid::from_u128(9),
            created_at: "2026-01-01 00:00:00+00".into(),
            remote_connection_verified: false,
            provider_secret_stored: false,
        }
    }

    fn package() -> PresentationPackage {
        PresentationPackage {
            format: "niccolo.presentation".into(),
            format_version: 1,
            analysis_id: Uuid::from_u128(3),
            input_revision: 1,
            files: [
                ("README.md".into(), "hello".into()),
                ("data/a.csv".into(), "a,b\n".into()),
            ]
            .into_iter()
            .collect(),
            authoritative_state_remains_in_postgresql: true,
            is_full_database_backup: false,
            includes_original_source_fragment_contents: false,
            includes_examined_source_fragment_contents: false,
            examined_source_extracts_are_original_uploads: false,
            examined_source_extracts_establish_truth: false,
            reproduces_model_reasoning: false,
            contains_complete_investigation_event_log: false,
            includes_recorded_epistemic_policy_history: false,
            includes_recorded_human_credence: false,
            includes_deterministic_grounded_score_results: false,
            epistemic_control_history_proves_policy_used_for_output: false,
            published_to_git: false,
        }
    }

    fn intent(package: &PresentationPackage) -> PublicationIntentView {
        PublicationIntentView {
            publication_id: Uuid::from_u128(2),
            target_id: Uuid::from_u128(1),
            situation_id: Uuid::from_u128(4),
            analysis_id: package.analysis_id,
            scope_kind: "STANDARD_PRESENTATION".into(),
            package_sha256: presentation_package::package_sha256(package).unwrap(),
            file_count: package.files.len() as u32,
            total_text_bytes: total_text_bytes(package).unwrap(),
            requested_ref: "main".into(),
            includes_examined_source_text: false,
            includes_original_uploads: false,
            publication_state: "AWAITING_REMOTE_AUTHORIZATION".into(),
            requested_by_principal_id: Uuid::from_u128(9),
            created_at: "2026-01-01 00:00:00+00".into(),
            remote_publish_performed: false,
        }
    }

    #[test]
    fn target_preflight_is_read_only_and_verifies_repository_ref_and_visibility() {
        let transport = ScriptedTransport::new(vec![
            json!({"full_name":"owner/history","private":true}),
            json!({"object":{"sha":"a".repeat(40)}}),
        ]);
        let verified = verify_target(&transport, &target(), "test-secret-token").unwrap();
        assert_eq!(verified.repository_full_name, "owner/history");
        assert_eq!(verified.ref_name, "main");
        assert_eq!(verified.head_commit_sha, Some("a".repeat(40)));
        assert!(verified.repository_private);
        assert!(verified.requested_ref_exists);
        assert!(verified.creation_base_ref.is_none());
        let calls = transport.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|call| call.0 == "GET"));
        assert!(!format!("{calls:?}").contains("test-secret-token"));
    }

    #[test]
    fn target_preflight_reports_missing_branch_without_mutating_remote_state() {
        struct MissingRefTransport {
            calls: Mutex<Vec<(String, String, Option<Value>)>>,
            responses: Mutex<VecDeque<Value>>,
        }
        impl GitHubTransport for MissingRefTransport {
            fn json(
                &self,
                method: &str,
                path: &str,
                token: &str,
                body: Option<&Value>,
            ) -> Result<Value, GitHubPublishError> {
                assert_eq!(token, "test-secret-token");
                self.calls
                    .lock()
                    .unwrap()
                    .push((method.into(), path.into(), body.cloned()));
                if path.ends_with("/git/ref/heads/archive") {
                    return Err(GitHubPublishError::ProviderRejected(404));
                }
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(GitHubPublishError::Transport)
            }
        }
        let transport = MissingRefTransport {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(
                vec![
                    json!({"full_name":"owner/history","private":true,"default_branch":"main"}),
                    json!({"object":{"sha":"b".repeat(40)}}),
                ]
                .into(),
            ),
        };
        let mut target = target();
        target.ref_name = "archive".into();
        let verified = verify_target(&transport, &target, "test-secret-token").unwrap();
        assert!(!verified.requested_ref_exists);
        assert_eq!(verified.head_commit_sha, None);
        assert_eq!(verified.creation_base_ref.as_deref(), Some("main"));
        assert_eq!(verified.creation_base_commit_sha, Some("b".repeat(40)));
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .all(|call| call.0 == "GET")
        );
    }

    #[test]
    fn exact_package_is_committed_then_ref_is_verified_without_force() {
        let package = package();
        let intent = intent(&package);
        let transport = ScriptedTransport::new(vec![
            json!({"full_name":"owner/history","private":true}),
            json!({"object":{"sha":"1".repeat(40)}}),
            json!({"tree":{"sha":"2".repeat(40)},"message":"older"}),
            json!({"sha":"3".repeat(40)}),
            json!({"sha":"4".repeat(40)}),
            json!({"sha":"5".repeat(40)}),
            json!({"sha":"6".repeat(40)}),
            json!({"object":{"sha":"6".repeat(40)}}),
            json!({"object":{"sha":"6".repeat(40)}}),
            json!({"tree":{"sha":"5".repeat(40)},"message":expected_commit_message(&intent)}),
        ]);
        let verified = publish_standard_package(
            &transport,
            &target(),
            &intent,
            &package,
            "test-secret-token",
        )
        .unwrap();
        assert_eq!(verified.commit_sha, "6".repeat(40));
        assert!(!verified.already_present);
        let calls = transport.calls.lock().unwrap();
        let patch = calls.iter().find(|call| call.0 == "PATCH").unwrap();
        assert_eq!(patch.2.as_ref().unwrap()["force"], false);
        assert!(!format!("{calls:?}").contains("test-secret-token"));
        assert!(calls.iter().any(|call| {
            call.2
                .as_ref()
                .and_then(|value| value.get("tree"))
                .and_then(Value::as_array)
                .is_some_and(|entries| {
                    entries.iter().all(|entry| {
                        entry["path"]
                            .as_str()
                            .unwrap()
                            .starts_with("niccolo/situations/")
                    })
                })
        }));
    }

    #[test]
    fn missing_requested_branch_is_created_from_default_branch_without_force() {
        let package = package();
        let mut target = target();
        target.ref_name = "niccolo-history".into();
        let mut intent = intent(&package);
        intent.requested_ref = target.ref_name.clone();
        // Exercise the provider-404 path with a transport that reports the
        // requested branch as absent exactly once.
        struct MissingBranchTransport {
            calls: Mutex<Vec<(String, String, Option<Value>)>>,
            responses: Mutex<VecDeque<Value>>,
        }
        impl GitHubTransport for MissingBranchTransport {
            fn json(
                &self,
                method: &str,
                path: &str,
                token: &str,
                body: Option<&Value>,
            ) -> Result<Value, GitHubPublishError> {
                assert_eq!(token, "test-secret-token");
                self.calls
                    .lock()
                    .unwrap()
                    .push((method.into(), path.into(), body.cloned()));
                if path.ends_with("/git/ref/heads/niccolo-history")
                    && self
                        .calls
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|call| call.1 == path)
                        .count()
                        == 1
                {
                    return Err(GitHubPublishError::ProviderRejected(404));
                }
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or(GitHubPublishError::Transport)
            }
        }
        let transport = MissingBranchTransport {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(
                vec![
                    json!({"full_name":"owner/history","private":true,"default_branch":"main"}),
                    json!({"object":{"sha":"1".repeat(40)}}),
                    json!({"tree":{"sha":"2".repeat(40)},"message":"older"}),
                    json!({"sha":"3".repeat(40)}),
                    json!({"sha":"4".repeat(40)}),
                    json!({"sha":"5".repeat(40)}),
                    json!({"sha":"6".repeat(40)}),
                    json!({"object":{"sha":"6".repeat(40)}}),
                    json!({"object":{"sha":"6".repeat(40)}}),
                    json!({"tree":{"sha":"5".repeat(40)},"message":expected_commit_message(&intent)}),
                ]
                .into(),
            ),
        };
        let verified =
            publish_standard_package(&transport, &target, &intent, &package, "test-secret-token")
                .unwrap();
        assert_eq!(verified.ref_name, "niccolo-history");
        let calls = transport.calls.lock().unwrap();
        let create_ref = calls
            .iter()
            .find(|call| call.0 == "POST" && call.1.ends_with("/git/refs"))
            .unwrap();
        assert_eq!(
            create_ref.2.as_ref().unwrap()["ref"],
            "refs/heads/niccolo-history"
        );
        assert_eq!(create_ref.2.as_ref().unwrap()["sha"], "6".repeat(40));
        assert!(!calls.iter().any(|call| call.0 == "PATCH"));
    }

    #[test]
    fn response_loss_retry_does_not_create_a_duplicate_commit() {
        let package = package();
        let intent = intent(&package);
        let message = expected_commit_message(&intent);
        let transport = ScriptedTransport::new(vec![
            json!({"full_name":"owner/history","private":true}),
            json!({"object":{"sha":"6".repeat(40)}}),
            json!({"tree":{"sha":"5".repeat(40)},"message":message}),
            json!({"sha":"3".repeat(40)}),
            json!({"sha":"4".repeat(40)}),
            json!({"sha":"5".repeat(40)}),
        ]);
        let verified = publish_standard_package(
            &transport,
            &target(),
            &intent,
            &package,
            "test-secret-token",
        )
        .unwrap();
        assert!(verified.already_present);
        assert_eq!(verified.commit_sha, "6".repeat(40));
        let calls = transport.calls.lock().unwrap();
        assert!(
            !calls
                .iter()
                .any(|call| call.1.ends_with("/git/commits") && call.0 == "POST")
        );
        assert!(!calls.iter().any(|call| call.0 == "PATCH"));
    }

    #[test]
    fn ref_match_without_matching_commit_tree_is_not_verified() {
        let package = package();
        let intent = intent(&package);
        let transport = ScriptedTransport::new(vec![
            json!({"full_name":"owner/history","private":true}),
            json!({"object":{"sha":"1".repeat(40)}}),
            json!({"tree":{"sha":"2".repeat(40)},"message":"older"}),
            json!({"sha":"3".repeat(40)}),
            json!({"sha":"4".repeat(40)}),
            json!({"sha":"5".repeat(40)}),
            json!({"sha":"6".repeat(40)}),
            json!({"object":{"sha":"6".repeat(40)}}),
            json!({"object":{"sha":"6".repeat(40)}}),
            json!({"tree":{"sha":"9".repeat(40)},"message":expected_commit_message(&intent)}),
        ]);
        assert_eq!(
            publish_standard_package(
                &transport,
                &target(),
                &intent,
                &package,
                "test-secret-token"
            ),
            Err(GitHubPublishError::VerificationMismatch)
        );
    }

    #[test]
    fn excessive_remote_file_fanout_is_rejected_before_network_calls() {
        let mut package = package();
        package.files.clear();
        for index in 0..=MAX_REMOTE_PACKAGE_FILES {
            package
                .files
                .insert(format!("data/{index:03}.txt"), "x".into());
        }
        let intent = intent(&package);
        let transport = ScriptedTransport::new(vec![]);
        assert_eq!(
            publish_standard_package(
                &transport,
                &target(),
                &intent,
                &package,
                "test-secret-token"
            ),
            Err(GitHubPublishError::LocalIntegrity)
        );
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn changed_local_package_is_rejected_before_any_remote_call() {
        let mut package = package();
        let intent = intent(&package);
        package.files.insert("README.md".into(), "changed".into());
        let transport = ScriptedTransport::new(vec![]);
        assert_eq!(
            publish_standard_package(
                &transport,
                &target(),
                &intent,
                &package,
                "test-secret-token"
            ),
            Err(GitHubPublishError::LocalIntegrity)
        );
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn visibility_mismatch_and_unsafe_tokens_fail_closed() {
        let package = package();
        let intent = intent(&package);
        let transport = ScriptedTransport::new(vec![json!({
            "full_name":"owner/history","private":false
        })]);
        assert_eq!(
            publish_standard_package(
                &transport,
                &target(),
                &intent,
                &package,
                "test-secret-token"
            ),
            Err(GitHubPublishError::VerificationMismatch)
        );
        let empty = ScriptedTransport::new(vec![]);
        assert_eq!(
            publish_standard_package(&empty, &target(), &intent, &package, "bad\ntoken"),
            Err(GitHubPublishError::LocalIntegrity)
        );
    }
}
