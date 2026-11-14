//! Qualified collection, custody and transformation provenance.
//!
//! This models how an artifact reached Niccolò. A collector, summarizer, exporter,
//! importer or operator is not thereby the original author or an epistemically
//! privileged actor.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CollectionProvenanceError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid collection provenance event")]
    Invalid,
    #[error("analysis or referenced frozen source unavailable")]
    Invisible,
    #[error("collection provenance replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionEventInput {
    pub event_id: Uuid,
    pub actor_label: String,
    pub actor_role: String,
    pub operation: String,
    pub input_document_id: Option<Uuid>,
    pub input_evidence_id: Option<Uuid>,
    pub output_document_id: Option<Uuid>,
    pub output_evidence_id: Option<Uuid>,
    pub occurred_at: Option<String>,
    pub method: String,
    pub assertion_kind: String,
    pub qualification: String,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionEvent {
    pub event_id: Uuid,
    pub actor_label: String,
    pub actor_role: String,
    pub operation: String,
    pub input_document_id: Option<Uuid>,
    pub input_evidence_id: Option<Uuid>,
    pub output_document_id: Option<Uuid>,
    pub output_evidence_id: Option<Uuid>,
    pub occurred_at: Option<String>,
    pub method: String,
    pub assertion_kind: String,
    pub qualification: String,
    pub note: String,
    pub recorded_by: Uuid,
    pub actor_role_establishes_original_authorship: bool,
    pub actor_role_establishes_neutrality: bool,
    pub replayed: bool,
}

type EventRow = (
    Uuid,
    String,
    String,
    String,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    String,
    String,
    String,
    String,
    Uuid,
);

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid(input: &CollectionEventInput) -> bool {
    !input.event_id.is_nil()
        && input.input_document_id.is_none_or(|id| !id.is_nil())
        && input.input_evidence_id.is_none_or(|id| !id.is_nil())
        && input.output_document_id.is_none_or(|id| !id.is_nil())
        && input.output_evidence_id.is_none_or(|id| !id.is_nil())
        && valid_text(&input.actor_label, 512)
        && valid_text(&input.operation, 512)
        && valid_text(&input.method, 4096)
        && valid_text(&input.note, 4096)
        && matches!(
            input.actor_role.as_str(),
            "ORIGINAL_AUTHOR"
                | "CUSTODIAN"
                | "COLLECTOR"
                | "INTERVIEWER"
                | "TRANSCRIBER"
                | "SUMMARIZER"
                | "TRANSFORMER"
                | "EXPORTER"
                | "IMPORTER"
                | "ATTESTER"
                | "OPERATOR"
        )
        && matches!(
            input.assertion_kind.as_str(),
            "DOCUMENTARY_OBSERVATION" | "HUMAN_ASSERTION" | "MACHINE_HYPOTHESIS"
        )
        && matches!(
            input.qualification.as_str(),
            "OBSERVED"
                | "REPORTED"
                | "ATTRIBUTED"
                | "INFERRED"
                | "UNCERTAIN"
                | "CONTRADICTED"
                | "UNKNOWN"
        )
        && input
            .occurred_at
            .as_ref()
            .is_none_or(|v| valid_text(v, 128))
        && (input.input_document_id.is_some()
            || input.input_evidence_id.is_some()
            || input.output_document_id.is_some()
            || input.output_evidence_id.is_some())
}

fn valid_scope(scope: AnalysisScope) -> bool {
    !scope.workspace.is_nil() && !scope.situation.is_nil() && !scope.analysis.is_nil()
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, CollectionProvenanceError> {
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
        return Err(CollectionProvenanceError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

async fn visible_document(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
    revision: i64,
    document: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM niccolo.situation_revision_sources
            WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 AND document_id=$4
         )",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(revision)
    .bind(document)
    .fetch_one(&mut **tx)
    .await
}

async fn visible_evidence(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
    revision: i64,
    evidence: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
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
    .bind(evidence)
    .fetch_one(&mut **tx)
    .await
}

fn from_row(row: EventRow, replayed: bool) -> CollectionEvent {
    CollectionEvent {
        event_id: row.0,
        actor_label: row.1,
        actor_role: row.2,
        operation: row.3,
        input_document_id: row.4,
        input_evidence_id: row.5,
        output_document_id: row.6,
        output_evidence_id: row.7,
        occurred_at: row.8,
        method: row.9,
        assertion_kind: row.10,
        qualification: row.11,
        note: row.12,
        recorded_by: row.13,
        actor_role_establishes_original_authorship: false,
        actor_role_establishes_neutrality: false,
        replayed,
    }
}

pub async fn record(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    input: &CollectionEventInput,
) -> Result<CollectionEvent, CollectionProvenanceError> {
    if !valid_scope(scope) || actor.is_nil() || !valid(input) {
        return Err(CollectionProvenanceError::Invalid);
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
        return Err(CollectionProvenanceError::Invisible);
    };

    for document in [input.input_document_id, input.output_document_id]
        .into_iter()
        .flatten()
    {
        if !visible_document(&mut tx, scope, revision, document).await? {
            return Err(CollectionProvenanceError::Invisible);
        }
    }
    for evidence in [input.input_evidence_id, input.output_evidence_id]
        .into_iter()
        .flatten()
    {
        if !visible_evidence(&mut tx, scope, revision, evidence).await? {
            return Err(CollectionProvenanceError::Invisible);
        }
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_collection_events
         (workspace_id,situation_id,analysis_id,event_id,actor_label,actor_role,operation,
          input_document_id,input_evidence_id,output_document_id,output_evidence_id,
          occurred_at,method,assertion_kind,qualification,note,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,
                CASE WHEN $12::text IS NULL THEN NULL ELSE $12::text::timestamptz END,
                $13,$14,$15,$16,$17)
         ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.event_id)
    .bind(input.actor_label.trim())
    .bind(&input.actor_role)
    .bind(input.operation.trim())
    .bind(input.input_document_id)
    .bind(input.input_evidence_id)
    .bind(input.output_document_id)
    .bind(input.output_evidence_id)
    .bind(input.occurred_at.as_deref())
    .bind(input.method.trim())
    .bind(&input.assertion_kind)
    .bind(&input.qualification)
    .bind(input.note.trim())
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: Option<EventRow> = sqlx::query_as(
        "SELECT event_id,actor_label,actor_role,operation,input_document_id,input_evidence_id,
                output_document_id,output_evidence_id,occurred_at::text,method,assertion_kind,
                qualification,note,recorded_by_principal_id
           FROM niccolo.situation_analysis_collection_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND event_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(CollectionProvenanceError::Invisible);
    };
    let time_matches: bool = sqlx::query_scalar(
        "SELECT occurred_at IS NOT DISTINCT FROM
                CASE WHEN $5::text IS NULL THEN NULL ELSE $5::text::timestamptz END
           FROM niccolo.situation_analysis_collection_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND event_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.event_id)
    .bind(input.occurred_at.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    if row.1 != input.actor_label.trim()
        || row.2 != input.actor_role
        || row.3 != input.operation.trim()
        || row.4 != input.input_document_id
        || row.5 != input.input_evidence_id
        || row.6 != input.output_document_id
        || row.7 != input.output_evidence_id
        || !time_matches
        || row.9 != input.method.trim()
        || row.10 != input.assertion_kind
        || row.11 != input.qualification
        || row.12 != input.note.trim()
        || row.13 != actor
    {
        return Err(CollectionProvenanceError::Conflict);
    }
    tx.commit().await?;
    Ok(from_row(row, !inserted))
}

pub async fn list(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<CollectionEvent>, CollectionProvenanceError> {
    if !valid_scope(scope) {
        return Err(CollectionProvenanceError::Invalid);
    }
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
        return Err(CollectionProvenanceError::Invisible);
    }
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT event_id,actor_label,actor_role,operation,input_document_id,input_evidence_id,
                output_document_id,output_evidence_id,occurred_at::text,method,assertion_kind,
                qualification,note,recorded_by_principal_id
           FROM niccolo.situation_analysis_collection_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
          ORDER BY COALESCE(occurred_at,'-infinity'::timestamptz),created_at,event_id
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

    fn sample(role: &str) -> CollectionEventInput {
        CollectionEventInput {
            event_id: Uuid::new_v4(),
            actor_label: "Elias".into(),
            actor_role: role.into(),
            operation: "Prepared interview summary".into(),
            input_document_id: Some(Uuid::new_v4()),
            input_evidence_id: None,
            output_document_id: None,
            output_evidence_id: None,
            occurred_at: None,
            method: "Human-authored summary from interview notes".into(),
            assertion_kind: "HUMAN_ASSERTION".into(),
            qualification: "ATTRIBUTED".into(),
            note: "Role describes collection behavior, not source authorship.".into(),
        }
    }

    #[test]
    fn nil_collection_artifact_identities_are_invalid() {
        let mut value = sample("COLLECTOR");
        value.event_id = Uuid::nil();
        assert!(!valid(&value));
        value = sample("COLLECTOR");
        value.input_document_id = Some(Uuid::nil());
        assert!(!valid(&value));
        value = sample("COLLECTOR");
        value.input_document_id = None;
        value.output_evidence_id = Some(Uuid::nil());
        assert!(!valid(&value));
    }

    #[test]
    fn collection_roles_are_explicit_and_non_interchangeable() {
        assert!(valid(&sample("COLLECTOR")));
        assert!(valid(&sample("SUMMARIZER")));
        assert!(valid(&sample("IMPORTER")));
        assert_ne!("COLLECTOR", "ORIGINAL_AUTHOR");
    }

    #[test]
    fn provenance_text_rejects_control_characters() {
        for value in ["label\nforged", "method\rforged", "note\tforged", "x\0y"] {
            assert!(!valid_text(value, 4096));
        }
        assert!(valid_text("Human-authored collection note.", 4096));
    }

    #[test]
    fn event_requires_an_artifact_reference() {
        let mut x = sample("COLLECTOR");
        x.input_document_id = None;
        assert!(!valid(&x));
    }
}
