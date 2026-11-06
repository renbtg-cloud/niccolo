//! Development-only loopback HTTP server. Put behind a trusted TLS gateway
//! and complete a security review before any multi-user deployment.
use niccolo_evidence::{
    api,
    model_adapter::{ReasoningModel, UnavailableModel},
    openai_compatible::OpenAiCompatibleConfig,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("NICCOLO_PG_SECURE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&pool)
    .await?;
    if privileged {
        return Err("refusing privileged service connection".into());
    }
    let secure: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user,'niccolo_api','member')")
            .fetch_one(&pool)
            .await?;
    if !secure {
        return Err(
            "refusing legacy application role without restrictive bearer RLS policies".into(),
        );
    }
    let port = std::env::var("NICCOLO_API_PORT")
        .unwrap_or_else(|_| "38080".into())
        .parse::<u16>()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let model: Arc<dyn ReasoningModel> = match OpenAiCompatibleConfig::from_env()? {
        Some(config) => Arc::new(config.native_model()?),
        None => Arc::new(UnavailableModel),
    };
    eprintln!("niccolo HTTP API listening on loopback port {port}");
    axum::serve(listener, api::router_with_model(pool, model)).await?;
    Ok(())
}
