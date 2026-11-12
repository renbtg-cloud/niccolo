//! One-shot loopback handoff used only by the local first-run browser opener.
//!
//! The browser receives a random short-lived capability, never the bearer token.
//! A successful redemption reads the already-protected token file exactly once.
use crate::authz;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const DEFAULT_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum LocalHandoffError {
    #[error("local handoff unavailable")]
    NotFound,
    #[error("local handoff secret unavailable")]
    SecretUnavailable,
}

#[derive(Clone, Debug, Default)]
pub struct LocalHandoff {
    inner: Arc<Mutex<Option<Entry>>>,
}

#[derive(Debug)]
struct Entry {
    code_sha256: String,
    token_file: PathBuf,
    expires_at: Instant,
}

fn valid_code(code: &str) -> bool {
    code.len() == 64
        && code
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn code_digest(code: &str) -> String {
    format!("{:x}", Sha256::digest(code.as_bytes()))
}

fn read_bearer(path: &PathBuf) -> Result<String, LocalHandoffError> {
    let raw = fs::read_to_string(path).map_err(|_| LocalHandoffError::SecretUnavailable)?;
    let token = raw.strip_suffix('\n').unwrap_or(&raw);
    let token = token.strip_suffix('\r').unwrap_or(token);
    if token.contains(['\r', '\n']) || authz::credential_digest(token).is_err() {
        return Err(LocalHandoffError::SecretUnavailable);
    }
    Ok(token.to_owned())
}

impl LocalHandoff {
    pub fn disabled() -> Self {
        Self::default()
    }

    pub fn from_env() -> Self {
        let Ok(code) = std::env::var("NICCOLO_LOCAL_HANDOFF_CODE") else {
            return Self::disabled();
        };
        let Ok(token_file) = std::env::var("NICCOLO_LOCAL_HANDOFF_TOKEN_FILE") else {
            return Self::disabled();
        };
        Self::configured(&code, PathBuf::from(token_file), DEFAULT_TTL)
            .unwrap_or_else(|_| Self::disabled())
    }

    pub fn configured(
        code: &str,
        token_file: PathBuf,
        ttl: Duration,
    ) -> Result<Self, LocalHandoffError> {
        if !valid_code(code) || !token_file.is_absolute() || ttl.is_zero() {
            return Err(LocalHandoffError::NotFound);
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(Some(Entry {
                code_sha256: code_digest(code),
                token_file,
                expires_at: Instant::now() + ttl,
            }))),
        })
    }

    pub fn consume(&self, code: &str) -> Result<String, LocalHandoffError> {
        if !valid_code(code) {
            return Err(LocalHandoffError::NotFound);
        }
        let mut slot = self
            .inner
            .lock()
            .map_err(|_| LocalHandoffError::SecretUnavailable)?;
        let Some(entry) = slot.as_ref() else {
            return Err(LocalHandoffError::NotFound);
        };
        if Instant::now() >= entry.expires_at {
            *slot = None;
            return Err(LocalHandoffError::NotFound);
        }
        if code_digest(code) != entry.code_sha256 {
            return Err(LocalHandoffError::NotFound);
        }
        // Do not consume the one-shot code if the local secret file is temporarily
        // unreadable. The operator can repair permissions and retry before expiry.
        let token = read_bearer(&entry.token_file)?;
        *slot = None;
        Ok(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use uuid::Uuid;

    fn token_file() -> PathBuf {
        let path = std::env::temp_dir().join(format!("niccolo-handoff-{}.token", Uuid::new_v4()));
        fs::write(
            &path,
            format!(
                "ncl_{}{}\n",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            ),
        )
        .unwrap();
        path
    }

    #[test]
    fn wrong_code_does_not_consume_and_correct_code_is_one_shot() {
        let path = token_file();
        let code = "a".repeat(64);
        let handoff =
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap();
        assert_eq!(
            handoff.consume(&"b".repeat(64)),
            Err(LocalHandoffError::NotFound)
        );
        let token = handoff.consume(&code).unwrap();
        assert!(authz::credential_digest(&token).is_ok());
        assert_eq!(handoff.consume(&code), Err(LocalHandoffError::NotFound));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn expired_capability_fails_closed() {
        let path = token_file();
        let code = "f".repeat(64);
        let handoff =
            LocalHandoff::configured(&code, path.clone(), Duration::from_millis(1)).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(handoff.consume(&code), Err(LocalHandoffError::NotFound));
        assert_eq!(handoff.consume(&code), Err(LocalHandoffError::NotFound));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_secret_does_not_burn_the_capability() {
        let path = std::env::temp_dir().join(format!("niccolo-handoff-{}.token", Uuid::new_v4()));
        fs::write(&path, "not-a-token\n").unwrap();
        let code = "c".repeat(64);
        let handoff =
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap();
        assert_eq!(
            handoff.consume(&code),
            Err(LocalHandoffError::SecretUnavailable)
        );
        fs::write(
            &path,
            format!(
                "ncl_{}{}\n",
                Uuid::new_v4().simple(),
                Uuid::new_v4().simple()
            ),
        )
        .unwrap();
        assert!(handoff.consume(&code).is_ok());
        let _ = fs::remove_file(path);
    }
}
