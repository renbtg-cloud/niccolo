//! No bearer-token material is stored in the DB or logged by this module.
//! The database exposes an EXECUTE-only authorization decision, not membership rows.
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

// A network request carries only a SHA-256 digest of its validated bearer,
// never the raw token, into transaction-local SQL state. Tokio task-local scope
// prevents a digest from leaking to unrelated concurrent HTTP requests.
tokio::task_local! { static REQUEST_DIGEST: String; }
pub async fn with_auth_digest<F: std::future::Future>(digest: String, f: F) -> F::Output {
    REQUEST_DIGEST.scope(digest, f).await
}
/// The SQL migration applies a *restrictive* policy to the dedicated API role:
/// no transaction-local valid digest -> no protected rows, even if workspace_id
/// is tampered with. Legacy direct test roles do not receive the extra policy.
pub async fn bind_auth_digest(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), sqlx::Error> {
    if let Ok(digest) = REQUEST_DIGEST.try_with(Clone::clone) {
        let _: String = sqlx::query_scalar("SELECT set_config('niccolo.auth_digest',$1,true)")
            .bind(digest)
            .fetch_one(&mut **tx)
            .await?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Read,
    Write,
}
impl Capability {
    pub fn db_value(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("malformed bearer credential")]
    InvalidCredential,
    #[error("database authorization failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged database connection forbidden")]
    PrivilegedRole,
}

/// Accept only the explicit opaque-token grammar issued by provision_token.
/// Do not include the credential in errors, logs, or debug output.
pub fn credential_digest(token: &str) -> Result<String, AuthError> {
    if token.len() != 68
        || !token.starts_with("ncl_")
        || !token.as_bytes()[4..].iter().all(u8::is_ascii_hexdigit)
        || token.as_bytes()[4..].iter().any(|b| b.is_ascii_uppercase())
    {
        return Err(AuthError::InvalidCredential);
    }
    Ok(format!("{:x}", Sha256::digest(token.as_bytes())))
}

pub async fn authorize(
    pool: &PgPool,
    token: &str,
    workspace: Uuid,
    capability: Capability,
) -> Result<bool, AuthError> {
    let digest = credential_digest(token)?;
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(AuthError::PrivilegedRole);
    }
    let result: bool = sqlx::query_scalar("SELECT niccolo_auth.authorize($1, $2, $3)")
        .bind(digest)
        .bind(workspace)
        .bind(capability.db_value())
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result)
}

/// Return a verified principal ID for audit attribution, or None for no grant.
/// The client never supplies its own principal ID to the HTTP boundary.
pub async fn authorized_principal(
    pool: &PgPool,
    token: &str,
    workspace: Uuid,
    capability: Capability,
) -> Result<Option<Uuid>, AuthError> {
    let digest = credential_digest(token)?;
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(AuthError::PrivilegedRole);
    }
    let principal: Option<Uuid> =
        sqlx::query_scalar("SELECT niccolo_auth.authorized_principal($1,$2,$3)")
            .bind(digest)
            .bind(workspace)
            .bind(capability.db_value())
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(principal)
}

/// One-time secret issuance for a trusted admin, outside the HTTP server.
/// Callers must deliver the returned token securely; only its hash is persisted.
/// The admin pool is intentionally privileged and must never be used for requests.
pub async fn provision_token(
    admin: &PgPool,
    principal: Uuid,
    workspace: Uuid,
    can_read: bool,
    can_write: bool,
    days: i32,
) -> Result<String, AuthError> {
    if !(1..=365).contains(&days) || (!can_read && !can_write) {
        return Err(AuthError::InvalidCredential);
    }
    let token = format!("ncl_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let digest = credential_digest(&token)?;
    let mut tx = admin.begin().await?;
    sqlx::query(
        "INSERT INTO niccolo_auth.principals(principal_id) VALUES($1) ON CONFLICT DO NOTHING",
    )
    .bind(principal)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO niccolo_auth.memberships(principal_id,workspace_id,can_read,can_write) \
        VALUES ($1,$2,$3,$4) ON CONFLICT(principal_id,workspace_id) DO UPDATE SET \
        can_read=EXCLUDED.can_read,can_write=EXCLUDED.can_write,revoked_at=NULL",
    )
    .bind(principal)
    .bind(workspace)
    .bind(can_read)
    .bind(can_write)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO niccolo_auth.tokens(token_sha256,principal_id,expires_at) \
        VALUES ($1,$2,now()+($3 * interval '1 day'))",
    )
    .bind(digest)
    .bind(principal)
    .bind(days)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// Atomically rotate an existing live bearer token. The old token is revoked
/// in the same database transaction that installs the new hash.
/// Only a privileged provisioning process may call this operation.
pub async fn rotate_token(admin: &PgPool, old_token: &str) -> Result<String, AuthError> {
    let old_digest = credential_digest(old_token)?;
    let replacement = format!("ncl_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let new_digest = credential_digest(&replacement)?;
    let mut tx = admin.begin().await?;
    let current: Option<(Uuid,)> = sqlx::query_as(
        "SELECT principal_id FROM niccolo_auth.tokens WHERE token_sha256=$1 \
        AND revoked_at IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(&old_digest)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((principal,)) = current else {
        return Err(AuthError::InvalidCredential);
    };
    sqlx::query(
        "INSERT INTO niccolo_auth.tokens(token_sha256,principal_id,expires_at) \
        SELECT $1,principal_id,expires_at FROM niccolo_auth.tokens WHERE token_sha256=$2",
    )
    .bind(new_digest)
    .bind(&old_digest)
    .execute(&mut *tx)
    .await?;
    let changed = sqlx::query(
        "UPDATE niccolo_auth.tokens SET revoked_at=now() \
        WHERE token_sha256=$1 AND principal_id=$2 AND revoked_at IS NULL",
    )
    .bind(old_digest)
    .bind(principal)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(AuthError::InvalidCredential);
    }
    tx.commit().await?;
    Ok(replacement)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_are_strict_and_never_passed_to_db() {
        let token = format!("ncl_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let digest = credential_digest(&token).unwrap();
        assert_eq!(digest.len(), 64);
        assert_ne!(token, digest);
        for bad in ["", "Bearer ncl_", "ncl_1234", "ncl_".to_owned().as_str()] {
            assert!(credential_digest(bad).is_err());
        }
        let mut uppercase = token.clone();
        uppercase.replace_range(4..5, "A");
        assert!(credential_digest(&uppercase).is_err());
        assert!(credential_digest(&(token + "\n")).is_err());
    }
    #[test]
    fn capability_is_closed_enum() {
        assert_eq!(Capability::Read.db_value(), "read");
        assert_eq!(Capability::Write.db_value(), "write");
    }
}
