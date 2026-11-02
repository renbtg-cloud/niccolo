//! Trusted local operator utility. Intentionally not an HTTP endpoint.
//! Provisioning outputs a bearer secret exactly once: protect stdout and history.
use niccolo_evidence::authz::{credential_digest, provision_token, rotate_token};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let help = "usage: niccolo-provision mint WORKSPACE_UUID PRINCIPAL_UUID read|write DAYS\n       niccolo-provision rotate|revoke < TOKEN_FILE\nNEVER pass bearer tokens in command arguments or shell history.";
    if args.len() < 2 {
        return Err(help.into());
    }
    let admin = std::env::var("NICCOLO_PG_ADMIN_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&pool)
    .await?;
    if !privileged {
        return Err("provisioning requires a separate privileged database login".into());
    }
    match args[1].as_str() {
        "mint" if args.len() == 6 => {
            let workspace = Uuid::parse_str(&args[2])?;
            let principal = Uuid::parse_str(&args[3])?;
            let (read, write) = match args[4].as_str() {
                "read" => (true, false),
                "write" => (true, true),
                _ => return Err(help.into()),
            };
            let days = args[5].parse::<i32>()?;
            let token = provision_token(&pool, principal, workspace, read, write, days).await?;
            println!("{token}");
        }
        "rotate" if args.len() == 2 => {
            let secret = read_stdin_token()?;
            println!("{}", rotate_token(&pool, &secret).await?);
        }
        "revoke" if args.len() == 2 => {
            let secret = read_stdin_token()?;
            let digest = credential_digest(&secret)?;
            let rows=sqlx::query("UPDATE niccolo_auth.tokens SET revoked_at=now() WHERE token_sha256=$1 AND revoked_at IS NULL")
                .bind(digest).execute(&pool).await?.rows_affected();
            println!("revoked {rows} token(s)");
        }
        _ => return Err(help.into()),
    }
    Ok(())
}

fn read_stdin_token() -> Result<String, Box<dyn std::error::Error>> {
    use std::io::BufRead;
    let mut value = String::new();
    std::io::stdin().lock().read_line(&mut value)?;
    let token = value.trim_end_matches(['\r', '\n']);
    if token.len() != 68 {
        return Err("invalid token input".into());
    }
    Ok(token.to_owned())
}
