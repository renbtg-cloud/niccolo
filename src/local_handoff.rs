//! One-shot loopback handoff used only by the local first-run browser opener.
//!
//! The browser receives a random short-lived capability, never the bearer token.
//! A successful redemption reads the already-protected token file exactly once.
use crate::authz;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
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
    // The handoff file is a locally generated secret, not a generic filesystem
    // reference. Refuse symlinks, directories, FIFOs and devices before opening.
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalHandoffError::SecretUnavailable)?;
    if !metadata.file_type().is_file() {
        return Err(LocalHandoffError::SecretUnavailable);
    }
    // A local path may still point to a file larger than a credential. Read at
    // most 71 bytes, then fail closed. Valid credentials are 68 bytes,
    // optionally followed by LF or CRLF.
    let mut raw = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(71).read_to_string(&mut raw))
        .map_err(|_| LocalHandoffError::SecretUnavailable)?;
    if raw.len() > 70 {
        return Err(LocalHandoffError::SecretUnavailable);
    }
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
    fn concurrent_redemption_is_atomic_across_clones() {
        let path = token_file();
        let code = "d".repeat(64);
        let handoff =
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
        let mut workers = Vec::new();
        for _ in 0..8 {
            let worker_handoff = handoff.clone();
            let worker_code = code.clone();
            let worker_barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                worker_barrier.wait();
                worker_handoff.consume(&worker_code)
            }));
        }
        barrier.wait();
        let winners = workers
            .into_iter()
            .map(|worker| worker.join().unwrap().is_ok())
            .filter(|success| *success)
            .count();
        assert_eq!(
            winners, 1,
            "a one-shot handoff must have exactly one winner"
        );
        assert_eq!(handoff.consume(&code), Err(LocalHandoffError::NotFound));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn malformed_capabilities_and_unsafe_configuration_fail_closed() {
        let path = token_file();
        let code = "e".repeat(64);
        let handoff =
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap();
        for malformed in ["E".repeat(64), "e".repeat(63), "g".repeat(64)] {
            assert_eq!(
                handoff.consume(&malformed),
                Err(LocalHandoffError::NotFound)
            );
            assert!(matches!(
                LocalHandoff::configured(&malformed, path.clone(), Duration::from_secs(30)),
                Err(LocalHandoffError::NotFound)
            ));
        }
        assert!(matches!(
            LocalHandoff::configured(
                &code,
                PathBuf::from("relative.token"),
                Duration::from_secs(30)
            ),
            Err(LocalHandoffError::NotFound)
        ));
        assert!(matches!(
            LocalHandoff::configured(&code, path.clone(), Duration::ZERO),
            Err(LocalHandoffError::NotFound)
        ));
        assert!(handoff.consume(&code).is_ok());
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
    fn oversized_token_file_is_rejected_without_consuming_handoff() {
        let path = std::env::temp_dir().join(format!("niccolo-handoff-{}.token", Uuid::new_v4()));
        fs::write(&path, "ncl_".to_owned() + &"a".repeat(1024)).unwrap();
        let code = "b".repeat(64);
        let handoff =
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap();
        assert_eq!(
            handoff.consume(&code),
            Err(LocalHandoffError::SecretUnavailable)
        );
        fs::write(&path, format!("ncl_{}\r\n", "a".repeat(64))).unwrap();
        assert!(handoff.consume(&code).is_ok());
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
