//! Qualified temporal assertions about actors, roles, awareness, authority and influence.
//!
//! These records are observations, attributed reports or hypotheses. They never establish
//! consciousness, intent, motive, authority or influence merely by being stored.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum TemporalStateError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid temporal assertion")]
    Invalid,
    #[error("analysis or referenced frozen input unavailable")]
    Invisible,
    #[error("temporal assertion replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalAssertionInput {
    pub assertion_id: Uuid,
    pub subject_label: String,
    pub assertion_type: String,
    pub object_text: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub epistemic_kind: String,
    pub qualification: String,
    pub evidence_id: Option<Uuid>,
    pub contribution_id: Option<Uuid>,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalAssertion {
    pub assertion_id: Uuid,
    pub subject_label: String,
    pub assertion_type: String,
    pub object_text: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub epistemic_kind: String,
    pub qualification: String,
    pub evidence_id: Option<Uuid>,
    pub contribution_id: Option<Uuid>,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub assertion_establishes_truth: bool,
    pub replayed: bool,
}

type TemporalRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    Option<Uuid>,
    Option<Uuid>,
    String,
    Uuid,
);

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

fn valid_input(input: &TemporalAssertionInput) -> bool {
    let assertion_type_valid = matches!(
        input.assertion_type.as_str(),
        "ROLE"
            | "FORMAL_AUTHORITY"
            | "ACCESS_TO_INFORMATION"
            | "COMMUNICATION"
            | "DECLARED_BELIEF"
            | "ATTRIBUTED_BELIEF"
            | "INFERRED_AWARENESS"
            | "RELATIONSHIP"
            | "DECISION_PARTICIPATION"
            | "PRACTICAL_INFLUENCE"
    );
    let epistemic_kind_valid = matches!(
        input.epistemic_kind.as_str(),
        "DOCUMENTARY_OBSERVATION" | "HUMAN_ASSERTION" | "MACHINE_HYPOTHESIS"
    );
    let qualification_valid = matches!(
        input.qualification.as_str(),
        "OBSERVED"
            | "REPORTED"
            | "ATTRIBUTED"
            | "INFERRED"
            | "UNCERTAIN"
            | "CONTRADICTED"
            | "UNKNOWN"
    );
    // Awareness is never directly observable in this bounded actor model.
    // A source may observe communication or access, but the step from those
    // observations to awareness must remain visibly inferential/uncertain.
    let bounded_theory_of_mind = input.assertion_type != "INFERRED_AWARENESS"
        || matches!(
            input.qualification.as_str(),
            "INFERRED" | "UNCERTAIN" | "CONTRADICTED" | "UNKNOWN"
        );

    valid_text(&input.subject_label, 512)
        && valid_text(&input.object_text, 4096)
        && valid_text(&input.rationale, 4096)
        && assertion_type_valid
        && epistemic_kind_valid
        && qualification_valid
        && bounded_theory_of_mind
        && !(input.evidence_id.is_some() && input.contribution_id.is_some())
        && input
            .valid_from
            .as_ref()
            .is_none_or(|v| valid_text(v, 128))
        && input
            .valid_to
            .as_ref()
            .is_none_or(|v| valid_text(v, 128))
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, TemporalStateError> {
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
        return Err(TemporalStateError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn from_row(row: TemporalRow, replayed: bool) -> TemporalAssertion {
    TemporalAssertion {
        assertion_id: row.0,
        subject_label: row.1,
        assertion_type: row.2,
        object_text: row.3,
        valid_from: row.4,
        valid_to: row.5,
        epistemic_kind: row.6,
        qualification: row.7,
        evidence_id: row.8,
        contribution_id: row.9,
        rationale: row.10,
        recorded_by: row.11,
        assertion_establishes_truth: false,
        replayed,
    }
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    input: &TemporalAssertionInput,
) -> Result<TemporalAssertion, TemporalStateError> {
    if !valid_input(input) {
        return Err(TemporalStateError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let analysis: Option<i64> = sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(revision) = analysis else {
        return Err(TemporalStateError::Invisible);
    };

    // Keep malformed/reversed time ranges out of the durable reasoning state.
    // PostgreSQL is the timestamp parser used by storage, so validate with the
    // same semantics before the INSERT instead of accepting a backwards range
    // or surfacing malformed user input as a generic database failure.
    let temporal_bounds_valid: bool = sqlx::query_scalar(
        "SELECT CASE
           WHEN $1::text IS NOT NULL AND NOT pg_input_is_valid($1::text,'timestamptz'::regtype) THEN FALSE
           WHEN $2::text IS NOT NULL AND NOT pg_input_is_valid($2::text,'timestamptz'::regtype) THEN FALSE
           WHEN $1::text IS NOT NULL AND $2::text IS NOT NULL
             THEN $1::text::timestamptz <= $2::text::timestamptz
           ELSE TRUE
         END",
    )
    .bind(input.valid_from.as_deref())
    .bind(input.valid_to.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    if !temporal_bounds_valid {
        return Err(TemporalStateError::Invalid);
    }

    if let Some(evidence_id) = input.evidence_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1
                 FROM niccolo.situation_revision_sources rs
                 JOIN niccolo.source_fragments f
                   ON f.workspace_id=rs.workspace_id AND f.document_id=rs.document_id
                WHERE rs.workspace_id=$1 AND rs.situation_id=$2 AND rs.revision=$3
                  AND f.evidence_id=$4
             )",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(revision)
        .bind(evidence_id)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(TemporalStateError::Invisible);
        }
    }
    if let Some(contribution_id) = input.contribution_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(
               SELECT 1 FROM niccolo.situation_revision_contributions
                WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 AND contribution_id=$4
             )",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(revision)
        .bind(contribution_id)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(TemporalStateError::Invisible);
        }
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_temporal_assertions
         (workspace_id,situation_id,analysis_id,assertion_id,subject_label,assertion_type,object_text,
          valid_from,valid_to,epistemic_kind,qualification,evidence_id,contribution_id,rationale,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,
                CASE WHEN $8::text IS NULL THEN NULL ELSE $8::text::timestamptz END,
                CASE WHEN $9::text IS NULL THEN NULL ELSE $9::text::timestamptz END,
                $10,$11,$12,$13,$14,$15)
         ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.assertion_id)
    .bind(input.subject_label.trim())
    .bind(&input.assertion_type)
    .bind(input.object_text.trim())
    .bind(input.valid_from.as_deref())
    .bind(input.valid_to.as_deref())
    .bind(&input.epistemic_kind)
    .bind(&input.qualification)
    .bind(input.evidence_id)
    .bind(input.contribution_id)
    .bind(input.rationale.trim())
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: Option<TemporalRow> = sqlx::query_as(
        "SELECT assertion_id,subject_label,assertion_type,object_text,
                valid_from::text,valid_to::text,epistemic_kind,qualification,
                evidence_id,contribution_id,rationale,recorded_by_principal_id
           FROM niccolo.situation_analysis_temporal_assertions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND assertion_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.assertion_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(TemporalStateError::Invisible);
    };
    let times_match: bool = sqlx::query_scalar(
        "SELECT valid_from IS NOT DISTINCT FROM CASE WHEN $5::text IS NULL THEN NULL ELSE $5::text::timestamptz END
                AND valid_to IS NOT DISTINCT FROM CASE WHEN $6::text IS NULL THEN NULL ELSE $6::text::timestamptz END
           FROM niccolo.situation_analysis_temporal_assertions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND assertion_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.assertion_id)
    .bind(input.valid_from.as_deref())
    .bind(input.valid_to.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    if row.1 != input.subject_label.trim()
        || row.2 != input.assertion_type
        || row.3 != input.object_text.trim()
        || !times_match
        || row.6 != input.epistemic_kind
        || row.7 != input.qualification
        || row.8 != input.evidence_id
        || row.9 != input.contribution_id
        || row.10 != input.rationale.trim()
        || row.11 != actor
    {
        return Err(TemporalStateError::Conflict);
    }
    tx.commit().await?;
    Ok(from_row(row, !inserted))
}

pub async fn list(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<TemporalAssertion>, TemporalStateError> {
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
        return Err(TemporalStateError::Invisible);
    }
    let rows: Vec<TemporalRow> = sqlx::query_as(
        "SELECT assertion_id,subject_label,assertion_type,object_text,
                valid_from::text,valid_to::text,epistemic_kind,qualification,
                evidence_id,contribution_id,rationale,recorded_by_principal_id
           FROM niccolo.situation_analysis_temporal_assertions
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
          ORDER BY COALESCE(valid_from,'-infinity'::timestamptz),created_at,assertion_id
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

    fn sample(kind: &str, qualification: &str) -> TemporalAssertionInput {
        TemporalAssertionInput {
            assertion_id: Uuid::new_v4(),
            subject_label: "Actor X".into(),
            assertion_type: kind.into(),
            object_text: "Could approve settlement band X".into(),
            valid_from: Some("2026-03-01T00:00:00Z".into()),
            valid_to: Some("2026-04-01T00:00:00Z".into()),
            epistemic_kind: "MACHINE_HYPOTHESIS".into(),
            qualification: qualification.into(),
            evidence_id: None,
            contribution_id: None,
            rationale: "Current evidence supports only a qualified inference.".into(),
        }
    }

    #[test]
    fn authority_and_influence_are_distinct_types() {
        assert!(valid_input(&sample("FORMAL_AUTHORITY", "ATTRIBUTED")));
        assert!(valid_input(&sample("PRACTICAL_INFLUENCE", "INFERRED")));
        assert_ne!("FORMAL_AUTHORITY", "PRACTICAL_INFLUENCE");
    }

    #[test]
    fn inferred_awareness_cannot_be_spelled_as_observed_fact_by_type() {
        let mut x = sample("INFERRED_AWARENESS", "INFERRED");
        x.epistemic_kind = "MACHINE_HYPOTHESIS".into();
        assert!(valid_input(&x));

        for forbidden in ["OBSERVED", "REPORTED", "ATTRIBUTED"] {
            x.qualification = forbidden.into();
            assert!(
                !valid_input(&x),
                "awareness must remain inferential even when source material is strong"
            );
        }
        for bounded in ["INFERRED", "UNCERTAIN", "CONTRADICTED", "UNKNOWN"] {
            x.qualification = bounded.into();
            assert!(valid_input(&x));
        }
    }

    #[test]
    fn rejects_unknown_semantics_and_dual_source_links() {
        let mut x = sample("MIND_READING", "CERTAIN");
        x.evidence_id = Some(Uuid::new_v4());
        x.contribution_id = Some(Uuid::new_v4());
        assert!(!valid_input(&x));
    }
}
