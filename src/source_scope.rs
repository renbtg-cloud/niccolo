//! Explicit collection-scope and missingness assertions.
//!
//! "Not found", "not collected", "known unavailable", "out of scope" and
//! "unknown whether it exists" are deliberately distinct.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SourceScopeError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid source-scope assertion")]
    Invalid,
    #[error("analysis, source or superseded assertion unavailable")]
    Invisible,
    #[error("source-scope assertion replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceScopeInput {
    pub scope_id: Uuid,
    pub source_family: String,
    pub collection_status: String,
    pub scope_description: String,
    pub reason: String,
    pub collector_label: Option<String>,
    pub date_scope: Option<String>,
    pub assertion_kind: String,
    pub document_id: Option<Uuid>,
    pub supersedes_scope_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceScopeAssertion {
    pub scope_id: Uuid,
    pub source_family: String,
    pub collection_status: String,
    pub scope_description: String,
    pub reason: String,
    pub collector_label: Option<String>,
    pub date_scope: Option<String>,
    pub assertion_kind: String,
    pub document_id: Option<Uuid>,
    pub supersedes_scope_id: Option<Uuid>,
    pub recorded_by: Uuid,
    pub superseded: bool,
    pub missingness_establishes_misconduct: bool,
    pub replayed: bool,
}

type ScopeRow = (
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<Uuid>,
    Option<Uuid>,
    Uuid,
    bool,
);

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

fn valid(input: &SourceScopeInput) -> bool {
    valid_text(&input.source_family, 512)
        && valid_text(&input.scope_description, 4096)
        && valid_text(&input.reason, 4096)
        && matches!(
            input.collection_status.as_str(),
            "EXPECTED_NOT_REQUESTED"
                | "REQUESTED_PENDING"
                | "COLLECTED"
                | "KNOWN_UNAVAILABLE"
                | "DECLARED_OUT_OF_SCOPE"
                | "UNKNOWN_EXISTENCE"
        )
        && matches!(
            input.assertion_kind.as_str(),
            "DOCUMENTARY_OBSERVATION" | "HUMAN_ASSERTION" | "MACHINE_HYPOTHESIS"
        )
        && input
            .collector_label
            .as_ref()
            .is_none_or(|v| valid_text(v, 512))
        && input
            .date_scope
            .as_ref()
            .is_none_or(|v| valid_text(v, 1024))
        && input.supersedes_scope_id != Some(input.scope_id)
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, SourceScopeError> {
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
        return Err(SourceScopeError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn from_row(row: ScopeRow, replayed: bool) -> SourceScopeAssertion {
    SourceScopeAssertion {
        scope_id: row.0,
        source_family: row.1,
        collection_status: row.2,
        scope_description: row.3,
        reason: row.4,
        collector_label: row.5,
        date_scope: row.6,
        assertion_kind: row.7,
        document_id: row.8,
        supersedes_scope_id: row.9,
        recorded_by: row.10,
        superseded: row.11,
        missingness_establishes_misconduct: false,
        replayed,
    }
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    input: &SourceScopeInput,
) -> Result<SourceScopeAssertion, SourceScopeError> {
    if !valid(input) {
        return Err(SourceScopeError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(revision) = revision else {
        return Err(SourceScopeError::Invisible);
    };

    if let Some(document_id) = input.document_id {
        let frozen: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM niccolo.situation_revision_sources
                WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 AND document_id=$4
             )",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(revision)
        .bind(document_id)
        .fetch_one(&mut *tx)
        .await?;
        if !frozen {
            return Err(SourceScopeError::Invisible);
        }
    }
    if let Some(previous) = input.supersedes_scope_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM niccolo.situation_analysis_source_scope
                WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND scope_id=$4
             )",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(previous)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(SourceScopeError::Invisible);
        }
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_source_scope
         (workspace_id,situation_id,analysis_id,scope_id,source_family,collection_status,
          scope_description,reason,collector_label,date_scope,assertion_kind,document_id,
          supersedes_scope_id,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.scope_id)
    .bind(input.source_family.trim())
    .bind(&input.collection_status)
    .bind(input.scope_description.trim())
    .bind(input.reason.trim())
    .bind(input.collector_label.as_deref().map(str::trim))
    .bind(input.date_scope.as_deref().map(str::trim))
    .bind(&input.assertion_kind)
    .bind(input.document_id)
    .bind(input.supersedes_scope_id)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: Option<ScopeRow> = sqlx::query_as(
        "SELECT s.scope_id,s.source_family,s.collection_status,s.scope_description,s.reason,
                s.collector_label,s.date_scope,s.assertion_kind,s.document_id,s.supersedes_scope_id,
                s.recorded_by_principal_id,
                EXISTS(SELECT 1 FROM niccolo.situation_analysis_source_scope n
                        WHERE n.workspace_id=s.workspace_id AND n.situation_id=s.situation_id
                          AND n.analysis_id=s.analysis_id AND n.supersedes_scope_id=s.scope_id)
           FROM niccolo.situation_analysis_source_scope s
          WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 AND s.scope_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.scope_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(SourceScopeError::Invisible);
    };
    if row.1 != input.source_family.trim()
        || row.2 != input.collection_status
        || row.3 != input.scope_description.trim()
        || row.4 != input.reason.trim()
        || row.5
            != input
                .collector_label
                .as_deref()
                .map(str::trim)
                .map(str::to_owned)
        || row.6
            != input
                .date_scope
                .as_deref()
                .map(str::trim)
                .map(str::to_owned)
        || row.7 != input.assertion_kind
        || row.8 != input.document_id
        || row.9 != input.supersedes_scope_id
        || row.10 != actor
    {
        return Err(SourceScopeError::Conflict);
    }
    tx.commit().await?;
    Ok(from_row(row, !inserted))
}

pub async fn list(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<SourceScopeAssertion>, SourceScopeError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(SourceScopeError::Invisible);
    }
    let rows: Vec<ScopeRow> = sqlx::query_as(
        "SELECT s.scope_id,s.source_family,s.collection_status,s.scope_description,s.reason,
                s.collector_label,s.date_scope,s.assertion_kind,s.document_id,s.supersedes_scope_id,
                s.recorded_by_principal_id,
                EXISTS(SELECT 1 FROM niccolo.situation_analysis_source_scope n
                        WHERE n.workspace_id=s.workspace_id AND n.situation_id=s.situation_id
                          AND n.analysis_id=s.analysis_id AND n.supersedes_scope_id=s.scope_id)
           FROM niccolo.situation_analysis_source_scope s
          WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3
          ORDER BY s.source_family,s.created_at,s.scope_id
          LIMIT 256",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(|row| from_row(row, false)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(status: &str) -> SourceScopeInput {
        SourceScopeInput {
            scope_id: Uuid::new_v4(),
            source_family: "Finance shared-drive drafts".into(),
            collection_status: status.into(),
            scope_description: "Drafts before April 2026".into(),
            reason: "Collection inventory supplied by the external operator".into(),
            collector_label: Some("Elias".into()),
            date_scope: Some("2026-01-01..2026-03-31".into()),
            assertion_kind: "HUMAN_ASSERTION".into(),
            document_id: None,
            supersedes_scope_id: None,
        }
    }

    #[test]
    fn missingness_states_are_not_collapsed() {
        for status in [
            "EXPECTED_NOT_REQUESTED",
            "REQUESTED_PENDING",
            "COLLECTED",
            "KNOWN_UNAVAILABLE",
            "DECLARED_OUT_OF_SCOPE",
            "UNKNOWN_EXISTENCE",
        ] {
            assert!(valid(&sample(status)));
        }
        assert_ne!("KNOWN_UNAVAILABLE", "UNKNOWN_EXISTENCE");
        assert_ne!("EXPECTED_NOT_REQUESTED", "COLLECTED");
    }

    #[test]
    fn rejects_self_supersession() {
        let mut x = sample("UNKNOWN_EXISTENCE");
        x.supersedes_scope_id = Some(x.scope_id);
        assert!(!valid(&x));
    }
}
