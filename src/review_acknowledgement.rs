//! v0.77-v0.78: acknowledge an exact review-package hash without implying agreement.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AcknowledgementError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid acknowledgement")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewAcknowledgement {
    pub acknowledgement_id: Uuid,
    pub package_sha256: String,
    pub kind: String,
    pub note: Option<String>,
    pub acknowledged_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcknowledgementHistoryItem {
    pub acknowledgement_id: Uuid,
    pub package_sha256: String,
    pub kind: String,
    pub note: Option<String>,
    pub acknowledged_by: Uuid,
    pub acknowledgement_means_agreement: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AcknowledgementError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(AcknowledgementError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_sha256(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn valid_note(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn valid_scope(scope: AnalysisScope) -> bool {
    !scope.workspace.is_nil() && !scope.situation.is_nil() && !scope.analysis.is_nil()
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &ReviewAcknowledgement,
) -> Result<(), AcknowledgementError> {
    if !valid_scope(scope)
        || item.acknowledgement_id.is_nil()
        || item.acknowledged_by.is_nil()
        || !valid_sha256(&item.package_sha256)
        || !matches!(item.kind.as_str(), "VIEWED" | "EXPORTED" | "DISCUSSED")
        || item.note.as_ref().is_some_and(|value| !valid_note(value))
    {
        return Err(AcknowledgementError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_review_package_acknowledgements \
         (workspace_id,situation_id,analysis_id,acknowledgement_id,package_sha256,acknowledgement_kind,note,acknowledged_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.acknowledgement_id)
    .bind(&item.package_sha256)
    .bind(&item.kind)
    .bind(&item.note)
    .bind(item.acknowledged_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(String, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT package_sha256,acknowledgement_kind,note,acknowledged_by_principal_id \
         FROM niccolo.situation_review_package_acknowledgements \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND acknowledgement_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.acknowledgement_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            item.package_sha256.clone(),
            item.kind.clone(),
            item.note.clone(),
            item.acknowledged_by,
        ))
    {
        return Err(AcknowledgementError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn history(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<AcknowledgementHistoryItem>, AcknowledgementError> {
    if !valid_scope(scope) {
        return Err(AcknowledgementError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    type Row = (Uuid, String, String, Option<String>, Uuid);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT acknowledgement_id,package_sha256,acknowledgement_kind,note,acknowledged_by_principal_id \
         FROM niccolo.situation_review_package_acknowledgements \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY created_at,acknowledgement_id",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(acknowledgement_id, package_sha256, kind, note, acknowledged_by)| {
                AcknowledgementHistoryItem {
                    acknowledgement_id,
                    package_sha256,
                    kind,
                    note,
                    acknowledged_by,
                    acknowledgement_means_agreement: false,
                }
            },
        )
        .collect())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nil_acknowledgement_scope_is_rejected_before_sql() {
        let pool = PgPool::connect_lazy("postgres://nobody@127.0.0.1/unused").unwrap();
        let scope = AnalysisScope {
            workspace: Uuid::nil(),
            situation: Uuid::new_v4(),
            analysis: Uuid::new_v4(),
        };
        assert!(matches!(
            history(&pool, scope).await,
            Err(AcknowledgementError::Invalid)
        ));
    }

    #[test]
    fn acknowledgement_note_rejects_control_characters() {
        for value in ["viewed\nforged", "discussed\rforged", "exported\tforged", "x\0y"] {
            assert!(!valid_note(value));
        }
        assert!(valid_note("Reviewed, with disagreement preserved."));
    }
}
