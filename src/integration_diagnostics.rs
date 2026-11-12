//! Read-only aggregate diagnostics for system-integrator delivery behavior.
//!
//! These counters describe transport/integration patterns only. They are not
//! evidence quality, source credibility, truth probability, or operator trust.
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IntegrationDiagnosticsError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrationDiagnostics {
    pub source_document_count: i64,
    pub operational_event_count: i64,
    pub total_delivery_count: i64,
    pub duplicate_delivery_count: i64,
    pub duplicate_operational_event_count: i64,
    pub max_delivery_count: i64,
    pub documents_with_operational_identity: i64,
    pub documents_without_operational_identity: i64,
    pub repeated_unidentified_payload_group_count: i64,
    pub repeated_unidentified_payload_document_count: i64,
    pub max_repeated_unidentified_payload_documents: i64,
    pub documents_with_multiple_operational_ids: i64,
    pub max_operational_ids_per_document: i64,
    pub warning_codes: Vec<String>,
    pub interpretation: &'static str,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, IntegrationDiagnosticsError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(IntegrationDiagnosticsError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn summarize(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<IntegrationDiagnostics, IntegrationDiagnosticsError> {
    let mut tx = scoped(pool, workspace).await?;

    let (
        operational_event_count,
        total_delivery_count,
        duplicate_operational_event_count,
        duplicate_delivery_count,
        max_delivery_count,
    ): (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                COALESCE(sum(delivery_count),0)::bigint,
                count(*) FILTER (WHERE delivery_count > 1)::bigint,
                COALESCE(sum(GREATEST(delivery_count - 1,0)),0)::bigint,
                COALESCE(max(delivery_count),0)::bigint
           FROM niccolo.source_operational_events
          WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;

    let (source_document_count, documents_with_operational_identity): (i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint,
                    count(*) FILTER (WHERE EXISTS(
                      SELECT 1 FROM niccolo.source_operational_events e
                       WHERE e.workspace_id=d.workspace_id AND e.document_id=d.document_id
                    ))::bigint
               FROM niccolo.source_documents d
              WHERE d.workspace_id=$1",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;

    let (
        repeated_unidentified_payload_group_count,
        repeated_unidentified_payload_document_count,
        max_repeated_unidentified_payload_documents,
    ): (i64, i64, i64) = sqlx::query_as(
        "WITH repeated AS (
           SELECT count(*)::bigint AS document_count
             FROM niccolo.source_documents d
            WHERE d.workspace_id=$1
              AND NOT EXISTS(
                SELECT 1 FROM niccolo.source_operational_events e
                 WHERE e.workspace_id=d.workspace_id AND e.document_id=d.document_id
              )
            GROUP BY d.sha256
           HAVING count(*) > 1
         )
         SELECT count(*)::bigint,
                COALESCE(sum(document_count),0)::bigint,
                COALESCE(max(document_count),0)::bigint
           FROM repeated",
    )
    .bind(workspace)
    .fetch_one(&mut *tx)
    .await?;

    let (documents_with_multiple_operational_ids, max_operational_ids_per_document): (i64, i64) =
        sqlx::query_as(
            "WITH fanout AS (
               SELECT document_id,count(*)::bigint AS event_count
                 FROM niccolo.source_operational_events
                WHERE workspace_id=$1
                GROUP BY document_id
             )
             SELECT count(*) FILTER (WHERE event_count > 1)::bigint,
                    COALESCE(max(event_count),0)::bigint
               FROM fanout",
        )
        .bind(workspace)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    let documents_without_operational_identity =
        source_document_count.saturating_sub(documents_with_operational_identity);
    let mut warning_codes = Vec::new();
    if duplicate_delivery_count > 0 {
        warning_codes.push("DUPLICATE_OPERATIONAL_DELIVERIES".to_owned());
    }
    if repeated_unidentified_payload_group_count > 0 {
        warning_codes.push("REPEATED_PAYLOAD_WITHOUT_OPERATIONAL_IDENTITY".to_owned());
    }
    if documents_with_multiple_operational_ids > 0 {
        warning_codes.push("MULTIPLE_OPERATIONAL_IDS_FOR_DOCUMENT".to_owned());
    }

    Ok(IntegrationDiagnostics {
        source_document_count,
        operational_event_count,
        total_delivery_count,
        duplicate_delivery_count,
        duplicate_operational_event_count,
        max_delivery_count,
        documents_with_operational_identity,
        documents_without_operational_identity,
        repeated_unidentified_payload_group_count,
        repeated_unidentified_payload_document_count,
        max_repeated_unidentified_payload_documents,
        documents_with_multiple_operational_ids,
        max_operational_ids_per_document,
        warning_codes,
        interpretation: "Integration diagnostics describe delivery and identity patterns only; they do not measure evidence quality, credibility, independence, or truth.",
    })
}
