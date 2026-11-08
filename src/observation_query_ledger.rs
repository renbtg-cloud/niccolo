//! Durable audit of model-requested observation searches and their original
//! source-fragment results. Query scores are lexical, never evidence weights.
use crate::{analysis_structure::AnalysisScope, observation_retrieval::ObservationHit};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum QueryLedgerError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged connection forbidden")]
    Privileged,
    #[error("invalid search result batch")]
    Invalid,
    #[error("query result or snapshot unavailable")]
    Invisible,
    #[error("query result replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceQueryIntent {
    pub query_id: Uuid,
    pub query: String,
    pub rationale: String,
    pub expected_observation: String,
    pub direction: String,
    pub interpretation_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrievedObservation {
    pub evidence_id: Uuid,
    pub document_id: Uuid,
    pub ordinal: i32,
    pub byte_start: i32,
    pub byte_end: i32,
    pub speaker_label: String,
    pub content: String,
    pub source_class: String,
    pub format: String,
    pub locator: String,
    pub search_rank: i16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryTrace {
    pub query_id: Uuid,
    pub query: String,
    pub rationale: String,
    pub expected_observation: String,
    pub direction: String,
    pub interpretation_id: Option<Uuid>,
    pub returned_hit_count: usize,
    pub withheld_hit_count: usize,
    pub retrieved_observations: Vec<RetrievedObservation>,
    pub is_evidence_verification: bool,
}

pub fn valid_intent(q: &SourceQueryIntent) -> bool {
    crate::observation_retrieval::valid_query(&q.query, 5)
        && !q.rationale.trim().is_empty()
        && q.rationale.len() <= 2048
        && !q.rationale.chars().any(char::is_control)
        && !q.expected_observation.trim().is_empty()
        && q.expected_observation.len() <= 2048
        && !q.expected_observation.chars().any(char::is_control)
        && matches!(
            q.direction.as_str(),
            "SUPPORT" | "COUNTEREVIDENCE" | "DISCRIMINATE"
        )
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, QueryLedgerError> {
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
        return Err(QueryLedgerError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

/// Store only evidence IDs selected by the authorized, frozen-snapshot query.
/// The SQL selection revalidates analysis membership even if a caller submits
/// an arbitrary evidence ID.
pub async fn record_hits(
    pool: &PgPool,
    scope: AnalysisScope,
    query_id: Uuid,
    actor: Uuid,
    hits: &[ObservationHit],
) -> Result<(), QueryLedgerError> {
    if hits.len() > 5
        || hits.iter().any(|h| !h.lexical_relevance.is_finite() || h.lexical_relevance < 0.0)
        || hits.iter().map(|h| h.evidence_id).collect::<std::collections::BTreeSet<_>>().len() != hits.len()
    {
        return Err(QueryLedgerError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let query_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_observation_queries          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND query_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(query_id)
    .fetch_one(&mut *tx)
    .await?;
    if !query_exists {
        return Err(QueryLedgerError::Invisible);
    }
    for (position, hit) in hits.iter().enumerate() {
        let rank = (position + 1) as i16;
        let inserted = sqlx::query(
            "INSERT INTO niccolo.situation_analysis_observation_query_hits              (workspace_id,situation_id,analysis_id,query_id,evidence_id,hit_rank,lexical_relevance,recorded_by_principal_id)              SELECT s.workspace_id,s.situation_id,s.analysis_id,$4,$5,$6,$7,$8              FROM niccolo.situation_analysis_snapshots s              JOIN niccolo.situation_analysis_observation_queries q                ON q.workspace_id=s.workspace_id AND q.situation_id=s.situation_id AND q.analysis_id=s.analysis_id AND q.query_id=$4              JOIN niccolo.situation_revision_sources rs                ON rs.workspace_id=s.workspace_id AND rs.situation_id=s.situation_id AND rs.revision=s.input_revision              JOIN niccolo.source_fragments f                ON f.workspace_id=rs.workspace_id AND f.document_id=rs.document_id AND f.evidence_id=$5              JOIN niccolo.evidence e                ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id AND e.admissible              WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3                AND to_tsvector('simple',e.content) @@ plainto_tsquery('simple',q.query_text)              ON CONFLICT DO NOTHING",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(query_id)
        .bind(hit.evidence_id)
        .bind(rank)
        .bind(hit.lexical_relevance)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
        let existing: Option<(i16, f32, Uuid)> = sqlx::query_as(
            "SELECT hit_rank,lexical_relevance,recorded_by_principal_id              FROM niccolo.situation_analysis_observation_query_hits              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND query_id=$4 AND evidence_id=$5",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(query_id)
        .bind(hit.evidence_id)
        .fetch_optional(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 && existing.is_none() {
            return Err(QueryLedgerError::Invisible);
        }
        if existing != Some((rank, hit.lexical_relevance, actor)) {
            return Err(QueryLedgerError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Model-facing audit of retrieval intents and their outcomes, including
/// negative searches. The packet is bounded by a separate byte budget rather
/// than pretending that unreturned hits were examined.
pub async fn for_packet(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<(Vec<QueryTrace>, bool), QueryLedgerError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let queries: Vec<(Uuid, String, String, String, String, Option<Uuid>)> =
        sqlx::query_as(
            "SELECT query_id,query_text,rationale,expected_observation,direction,interpretation_id              FROM niccolo.situation_analysis_observation_queries              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3              ORDER BY created_at,query_id LIMIT 4",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .fetch_all(&mut *tx)
        .await?;

    const MAX_RETRIEVED_BYTES: usize = 128 * 1024;
    let mut bytes = 0usize;
    let mut truncated = false;
    let mut traces = Vec::new();
    for (query_id, query, rationale, expected_observation, direction, interpretation_id) in queries {
        type ResultRow = (Uuid, Uuid, i32, i32, i32, String, String, String, String, String, i16);
        let rows: Vec<ResultRow> = sqlx::query_as(
            "SELECT h.evidence_id,d.document_id,f.ordinal,f.byte_start,f.byte_end,                     f.speaker_label,e.content,e.source_class,d.format,d.locator,h.hit_rank              FROM niccolo.situation_analysis_observation_query_hits h              JOIN niccolo.source_fragments f                ON f.workspace_id=h.workspace_id AND f.evidence_id=h.evidence_id              JOIN niccolo.evidence e                ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id AND e.admissible              JOIN niccolo.source_documents d                ON d.workspace_id=f.workspace_id AND d.document_id=f.document_id              WHERE h.workspace_id=$1 AND h.situation_id=$2 AND h.analysis_id=$3 AND h.query_id=$4              ORDER BY h.hit_rank,h.evidence_id",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(query_id)
        .fetch_all(&mut *tx)
        .await?;
        let stored_hit_count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM niccolo.situation_analysis_observation_query_hits
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND query_id=$4",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(query_id)
        .fetch_one(&mut *tx)
        .await?;
        let returned_hit_count = rows.len();
        let withheld_hit_count = (stored_hit_count as usize).saturating_sub(returned_hit_count);
        let mut retrieved_observations = Vec::new();
        for row in rows {
            if bytes.saturating_add(row.6.len()) > MAX_RETRIEVED_BYTES {
                truncated = true;
                continue;
            }
            bytes += row.6.len();
            retrieved_observations.push(RetrievedObservation {
                evidence_id: row.0,
                document_id: row.1,
                ordinal: row.2,
                byte_start: row.3,
                byte_end: row.4,
                speaker_label: row.5,
                content: row.6,
                source_class: row.7,
                format: row.8,
                locator: row.9,
                search_rank: row.10,
            });
        }
        traces.push(QueryTrace {
            query_id,
            query,
            rationale,
            expected_observation,
            direction,
            interpretation_id,
            returned_hit_count,
            withheld_hit_count,
            retrieved_observations,
            is_evidence_verification: false,
        });
    }
    tx.commit().await?;
    Ok((traces, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn intent() -> SourceQueryIntent {
        SourceQueryIntent {
            query_id: Uuid::nil(),
            query: "Kessler".into(),
            rationale: "Distinguish an alternative explanation".into(),
            expected_observation: "A statement about the disputed settlement".into(),
            direction: "COUNTEREVIDENCE".into(),
            interpretation_id: None,
        }
    }
    #[test]
    fn validates_model_queries_as_intents_not_facts() {
        assert!(valid_intent(&intent()));
        let mut x = intent();
        x.direction = "TRUTH".into();
        assert!(!valid_intent(&x));
        x = intent();
        x.query = "---".into();
        assert!(!valid_intent(&x));
        x = intent();
        x.expected_observation.clear();
        assert!(!valid_intent(&x));
    }
}
