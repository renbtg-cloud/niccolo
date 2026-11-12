//! Read-only, analysis-frozen retrieval across *all* imported source fragments.
//!
//! This is a bounded result page, not a bounded search horizon: matching evidence
//! beyond the initial model packet remains discoverable. The query has no side
//! effects and makes no claim of factual truth or actor identity.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ObservationSearchError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged database connection forbidden")]
    Privileged,
    #[error("invalid observation search request")]
    Invalid,
    #[error("analysis snapshot not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationHit {
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
    /// Lexical matching score, never evidentiary strength or truth probability.
    pub lexical_relevance: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationSearchPage {
    pub analysis_id: Uuid,
    pub input_revision: i64,
    pub query: String,
    pub results: Vec<ObservationHit>,
    pub has_more: bool,
    pub searched_entire_frozen_revision: bool,
    pub results_establish_truth: bool,
    pub lexical_ranking_is_evidentiary_weight: bool,
}

type HitRow = (
    Uuid,
    Uuid,
    i32,
    i32,
    i32,
    String,
    String,
    String,
    String,
    String,
    f32,
);

pub fn valid_query(query: &str, limit: i64) -> bool {
    (1..=25).contains(&limit)
        && !query.trim().is_empty()
        && query.len() <= 160
        && query.chars().any(char::is_alphanumeric)
        && !query.chars().any(char::is_control)
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, ObservationSearchError> {
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
        return Err(ObservationSearchError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

/// Search the entire immutable input revision, not just the initial reasoning
/// packet's first 256 fragments. Every returned hit is identified by the
/// original evidence ID, source document and byte interval.
///
/// Plain-text search uses AND semantics for its lexemes, not semantic reasoning.
/// Pagination is a bounded *top-ranked page*; the caller can refine the query.
/// Search results are observations of source text, not certified facts.
pub async fn search(
    pool: &PgPool,
    scope: AnalysisScope,
    query: &str,
    limit: i64,
) -> Result<ObservationSearchPage, ObservationSearchError> {
    if !valid_query(query, limit) {
        return Err(ObservationSearchError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace).await?;
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT input_revision FROM niccolo.situation_analysis_snapshots          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let revision = revision.ok_or(ObservationSearchError::Invisible)?;

    // A punctuation-heavy query may contain valid letters but produce zero
    // PostgreSQL lexemes. It must not silently look like a negative finding.
    let nodes: i32 = sqlx::query_scalar("SELECT numnode(plainto_tsquery('simple', $1))")
        .bind(query)
        .fetch_one(&mut *tx)
        .await?;
    if nodes == 0 {
        return Err(ObservationSearchError::Invalid);
    }

    // All joins are scoped to the snapshot's original Situation revision.
    // PostgreSQL evaluates the full eligible set before applying the top-N
    // response limit; no fixed ingestion/packet-order prefix is searched.
    let rows: Vec<HitRow> = sqlx::query_as(
        "WITH search_terms AS MATERIALIZED (SELECT plainto_tsquery('simple', $4) AS q)          SELECT f.evidence_id,d.document_id,f.ordinal,f.byte_start,f.byte_end,                 f.speaker_label,e.content,e.source_class,d.format,d.locator,                 ts_rank_cd(to_tsvector('simple',e.content),q.q)::real AS lexical_relevance          FROM niccolo.situation_analysis_snapshots s          JOIN niccolo.situation_revision_sources rs            ON rs.workspace_id=s.workspace_id AND rs.situation_id=s.situation_id           AND rs.revision=s.input_revision          JOIN niccolo.source_documents d            ON d.workspace_id=rs.workspace_id AND d.document_id=rs.document_id          JOIN niccolo.source_fragments f            ON f.workspace_id=d.workspace_id AND f.document_id=d.document_id          JOIN niccolo.evidence e            ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id          CROSS JOIN search_terms q          WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3            AND e.admissible            AND to_tsvector('simple',e.content) @@ q.q          ORDER BY lexical_relevance DESC,d.document_id,f.ordinal,f.evidence_id          LIMIT $5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(query)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let has_more = rows.len() as i64 > limit;
    let results = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| ObservationHit {
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
            lexical_relevance: row.10,
        })
        .collect();
    Ok(ObservationSearchPage {
        analysis_id: scope.analysis,
        input_revision: revision,
        query: query.to_owned(),
        results,
        has_more,
        searched_entire_frozen_revision: true,
        results_establish_truth: false,
        lexical_ranking_is_evidentiary_weight: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_bounded_and_requires_searchable_text() {
        assert!(valid_query("Kessler", 10));
        assert!(valid_query("análise", 25));
        assert!(!valid_query("", 10));
        assert!(!valid_query("   ", 10));
        assert!(!valid_query("---", 10));
        assert!(!valid_query("word\nother", 10));
        assert!(!valid_query(&"x".repeat(161), 10));
        assert!(!valid_query("Kessler", 0));
        assert!(!valid_query("Kessler", 26));
    }
}
