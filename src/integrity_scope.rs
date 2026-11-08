//! Source-integrity scope separated from authenticity and completeness.
//!
//! Byte continuity can be checked for retained stored originals. That check does not
//! establish who created the bytes before collection, whether they are authentic,
//! or whether the collected set is complete.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IntegrityScopeError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid integrity assessment")]
    Invalid,
    #[error("analysis or frozen source unavailable")]
    Invisible,
    #[error("integrity assessment replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityAssessmentInput {
    pub assessment_id: Uuid,
    pub document_id: Uuid,
    pub integrity_scope: String,
    pub authenticity: String,
    pub completeness: String,
    pub rationale: String,
    pub assertion_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityAssessment {
    pub assessment_id: Uuid,
    pub document_id: Uuid,
    pub integrity_scope: String,
    pub byte_continuity: String,
    pub document_sha256: String,
    pub computed_sha256: Option<String>,
    pub authenticity: String,
    pub completeness: String,
    pub rationale: String,
    pub assertion_kind: String,
    pub recorded_by: Uuid,
    pub byte_continuity_establishes_authenticity: bool,
    pub byte_continuity_establishes_completeness: bool,
    pub replayed: bool,
}

type AssessmentRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Uuid,
);

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

fn valid(input: &IntegrityAssessmentInput) -> bool {
    matches!(
        input.integrity_scope.as_str(),
        "STORED_ORIGINAL_BYTES" | "FROM_DECLARED_COLLECTION_POINT" | "EXTERNAL_ARTIFACT"
    ) && matches!(
        input.authenticity.as_str(),
        "NOT_ASSESSED" | "UNKNOWN" | "ATTESTED" | "DISPUTED"
    ) && matches!(
        input.completeness.as_str(),
        "UNKNOWN" | "DECLARED_COMPLETE" | "KNOWN_INCOMPLETE" | "VERIFIED_FOR_SCOPE"
    ) && matches!(
        input.assertion_kind.as_str(),
        "DOCUMENTARY_OBSERVATION" | "HUMAN_ASSERTION" | "MACHINE_HYPOTHESIS"
    ) && valid_text(&input.rationale, 4096)
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, IntegrityScopeError> {
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
        return Err(IntegrityScopeError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn from_row(row: AssessmentRow, replayed: bool) -> IntegrityAssessment {
    IntegrityAssessment {
        assessment_id: row.0,
        document_id: row.1,
        integrity_scope: row.2,
        byte_continuity: row.3,
        document_sha256: row.4,
        computed_sha256: row.5,
        authenticity: row.6,
        completeness: row.7,
        rationale: row.8,
        assertion_kind: row.9,
        recorded_by: row.10,
        byte_continuity_establishes_authenticity: false,
        byte_continuity_establishes_completeness: false,
        replayed,
    }
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    input: &IntegrityAssessmentInput,
) -> Result<IntegrityAssessment, IntegrityScopeError> {
    if !valid(input) {
        return Err(IntegrityScopeError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let document_sha256: Option<String> = sqlx::query_scalar(
        "SELECT d.sha256
           FROM niccolo.situation_analysis_snapshots a
           JOIN niccolo.situation_revision_sources rs
             ON rs.workspace_id=a.workspace_id AND rs.situation_id=a.situation_id
            AND rs.revision=a.input_revision
           JOIN niccolo.source_documents d
             ON d.workspace_id=rs.workspace_id AND d.document_id=rs.document_id
          WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3
            AND d.document_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.document_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(document_sha256) = document_sha256 else {
        return Err(IntegrityScopeError::Invisible);
    };

    let (byte_continuity, computed_sha256) = if input.integrity_scope == "STORED_ORIGINAL_BYTES" {
        let payload: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT original_bytes FROM niccolo.source_payloads
              WHERE workspace_id=$1 AND document_id=$2",
        )
        .bind(scope.workspace)
        .bind(input.document_id)
        .fetch_optional(&mut *tx)
        .await?;
        match payload {
            Some(bytes) => {
                let computed = format!("{:x}", Sha256::digest(&bytes));
                (
                    if computed == document_sha256 {
                        "VERIFIED".to_owned()
                    } else {
                        "FAILED".to_owned()
                    },
                    Some(computed),
                )
            }
            None => ("UNKNOWN".to_owned(), None),
        }
    } else {
        ("UNKNOWN".to_owned(), None)
    };

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_source_integrity
         (workspace_id,situation_id,analysis_id,document_id,assessment_id,integrity_scope,
          byte_continuity,document_sha256,computed_sha256,authenticity,completeness,
          rationale,assertion_kind,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.document_id)
    .bind(input.assessment_id)
    .bind(&input.integrity_scope)
    .bind(&byte_continuity)
    .bind(&document_sha256)
    .bind(&computed_sha256)
    .bind(&input.authenticity)
    .bind(&input.completeness)
    .bind(input.rationale.trim())
    .bind(&input.assertion_kind)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: Option<AssessmentRow> = sqlx::query_as(
        "SELECT assessment_id,document_id,integrity_scope,byte_continuity,document_sha256,
                computed_sha256,authenticity,completeness,rationale,assertion_kind,
                recorded_by_principal_id
           FROM niccolo.situation_analysis_source_integrity
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND document_id=$4 AND assessment_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.document_id)
    .bind(input.assessment_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(IntegrityScopeError::Invisible);
    };
    if row.2 != input.integrity_scope
        || row.3 != byte_continuity
        || row.4 != document_sha256
        || row.5 != computed_sha256
        || row.6 != input.authenticity
        || row.7 != input.completeness
        || row.8 != input.rationale.trim()
        || row.9 != input.assertion_kind
        || row.10 != actor
    {
        return Err(IntegrityScopeError::Conflict);
    }
    tx.commit().await?;
    Ok(from_row(row, !inserted))
}

pub async fn list(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<IntegrityAssessment>, IntegrityScopeError> {
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
        return Err(IntegrityScopeError::Invisible);
    }
    let rows: Vec<AssessmentRow> = sqlx::query_as(
        "SELECT assessment_id,document_id,integrity_scope,byte_continuity,document_sha256,
                computed_sha256,authenticity,completeness,rationale,assertion_kind,
                recorded_by_principal_id
           FROM niccolo.situation_analysis_source_integrity
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
          ORDER BY document_id,created_at,assessment_id LIMIT 256",
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

    #[test]
    fn valid_hash_scope_does_not_imply_authenticity() {
        let input = IntegrityAssessmentInput {
            assessment_id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            integrity_scope: "FROM_DECLARED_COLLECTION_POINT".into(),
            authenticity: "UNKNOWN".into(),
            completeness: "UNKNOWN".into(),
            rationale: "Hash continuity starts only at the declared collection point.".into(),
            assertion_kind: "HUMAN_ASSERTION".into(),
        };
        assert!(valid(&input));
        assert_ne!(input.integrity_scope, "AUTHENTICITY");
    }
}
