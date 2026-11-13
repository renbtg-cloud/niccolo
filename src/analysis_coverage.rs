//! Explicit source-fragment coverage for a frozen analysis input.
//!
//! A source being present in a Situation never implies it was examined. This
//! module records only exact imported fragments that a caller explicitly says
//! were examined by one analysis, and only if the fragment belonged to that
//! analysis's frozen Situation revision.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisCoverageError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid coverage request")]
    Invalid,
    #[error("coverage identifier replay conflicts with original recorder")]
    Conflict,
    #[error("analysis or fragment is not visible in the frozen input")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentCoverage {
    pub document_id: Uuid,
    pub format: String,
    pub catalogued_fragment_count: i64,
    pub examined_fragment_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageTotals {
    pub catalogued_fragment_count: i64,
    pub examined_fragment_count: i64,
    pub unexamined_fragment_count: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExaminedFragment {
    pub evidence_id: Uuid,
    pub document_id: Uuid,
    pub ordinal: i32,
    pub byte_start: i32,
    pub byte_end: i32,
    pub speaker_label: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

type FragmentRow = (Uuid, i32, i32, i32, String, Uuid);

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AnalysisCoverageError> {
    let mut tx = pool.begin().await?;
    if read_only {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(AnalysisCoverageError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

async fn load(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    evidence: Uuid,
) -> Result<Option<FragmentRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT f.document_id,f.ordinal,f.byte_start,f.byte_end,f.speaker_label,c.recorded_by_principal_id \
         FROM niccolo.situation_analysis_examined_fragments c \
         JOIN niccolo.source_fragments f USING(workspace_id,evidence_id) \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND c.evidence_id=$4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(evidence)
    .fetch_optional(&mut **tx)
    .await
}

fn from_row(evidence: Uuid, row: FragmentRow, replayed: bool) -> ExaminedFragment {
    ExaminedFragment {
        evidence_id: evidence,
        document_id: row.0,
        ordinal: row.1,
        byte_start: row.2,
        byte_end: row.3,
        speaker_label: row.4,
        recorded_by: row.5,
        replayed,
    }
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    evidence: Uuid,
) -> Result<ExaminedFragment, AnalysisCoverageError> {
    let mut recorded =
        record_many(pool, workspace, actor, situation, analysis, &[evidence]).await?;
    Ok(recorded
        .pop()
        .expect("single evidence input returns one row"))
}

pub(crate) async fn record_many(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    evidence_ids: &[Uuid],
) -> Result<Vec<ExaminedFragment>, AnalysisCoverageError> {
    if evidence_ids.len() > 256
        || evidence_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != evidence_ids.len()
    {
        return Err(AnalysisCoverageError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let mut recorded = Vec::with_capacity(evidence_ids.len());
    for evidence in evidence_ids {
        if let Some(row) = load(&mut tx, workspace, situation, analysis, *evidence).await? {
            if row.5 != actor {
                return Err(AnalysisCoverageError::Conflict);
            }
            recorded.push(from_row(*evidence, row, true));
            continue;
        }

        let eligible: Option<(Uuid, i32, i32, i32, String)> = sqlx::query_as(
            "SELECT f.document_id,f.ordinal,f.byte_start,f.byte_end,f.speaker_label \
             FROM niccolo.situation_analysis_snapshots s \
             JOIN niccolo.situation_revision_sources rs \
               ON rs.workspace_id=s.workspace_id AND rs.situation_id=s.situation_id AND rs.revision=s.input_revision \
             JOIN niccolo.source_fragments f \
               ON f.workspace_id=rs.workspace_id AND f.document_id=rs.document_id \
             WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 AND f.evidence_id=$4",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .bind(*evidence)
        .fetch_optional(&mut *tx)
        .await?;
        if eligible.is_none() {
            return Err(AnalysisCoverageError::Invisible);
        }

        let inserted = sqlx::query(
            "INSERT INTO niccolo.situation_analysis_examined_fragments \
             (workspace_id,situation_id,analysis_id,evidence_id,recorded_by_principal_id) \
             VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(situation)
        .bind(analysis)
        .bind(*evidence)
        .bind(actor)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;

        let Some(row) = load(&mut tx, workspace, situation, analysis, *evidence).await? else {
            return Err(AnalysisCoverageError::Invisible);
        };
        if !inserted && row.5 != actor {
            return Err(AnalysisCoverageError::Conflict);
        }
        recorded.push(from_row(*evidence, row, !inserted));
    }
    tx.commit().await?;
    Ok(recorded)
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<ExaminedFragment>, Option<Uuid>), AnalysisCoverageError> {
    if !(1..=100).contains(&limit) {
        return Err(AnalysisCoverageError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(AnalysisCoverageError::Invisible);
    }
    let rows: Vec<(Uuid, Uuid, i32, i32, i32, String, Uuid)> = sqlx::query_as(
        "SELECT c.evidence_id,f.document_id,f.ordinal,f.byte_start,f.byte_end,f.speaker_label,c.recorded_by_principal_id \
         FROM niccolo.situation_analysis_examined_fragments c \
         JOIN niccolo.source_fragments f USING(workspace_id,evidence_id) \
         WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 \
           AND ($4::uuid IS NULL OR c.evidence_id > $4) \
         ORDER BY c.evidence_id LIMIT $5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() as i64 > limit;
    let fragments = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(
                evidence_id,
                document_id,
                ordinal,
                byte_start,
                byte_end,
                speaker_label,
                recorded_by,
            )| {
                ExaminedFragment {
                    evidence_id,
                    document_id,
                    ordinal,
                    byte_start,
                    byte_end,
                    speaker_label,
                    recorded_by,
                    replayed: false,
                }
            },
        )
        .collect::<Vec<_>>();
    let next_after = has_more.then(|| fragments.last().expect("non-empty page").evidence_id);
    tx.commit().await?;
    Ok((fragments, next_after))
}

pub async fn totals(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<CoverageTotals, AnalysisCoverageError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT COUNT(f.evidence_id)::bigint,COUNT(c.evidence_id)::bigint
           FROM niccolo.situation_analysis_snapshots s
           LEFT JOIN niccolo.situation_revision_sources rs
             ON rs.workspace_id=s.workspace_id AND rs.situation_id=s.situation_id
            AND rs.revision=s.input_revision
           LEFT JOIN niccolo.source_fragments f
             ON f.workspace_id=rs.workspace_id AND f.document_id=rs.document_id
           LEFT JOIN niccolo.situation_analysis_examined_fragments c
             ON c.workspace_id=s.workspace_id AND c.situation_id=s.situation_id
            AND c.analysis_id=s.analysis_id AND c.evidence_id=f.evidence_id
          WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3
          GROUP BY s.analysis_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((catalogued_fragment_count, examined_fragment_count)) = row else {
        return Err(AnalysisCoverageError::Invisible);
    };
    let Some(unexamined_fragment_count) =
        catalogued_fragment_count.checked_sub(examined_fragment_count)
    else {
        return Err(AnalysisCoverageError::Invalid);
    };
    if catalogued_fragment_count < 0
        || examined_fragment_count < 0
        || unexamined_fragment_count < 0
    {
        return Err(AnalysisCoverageError::Invalid);
    }
    tx.commit().await?;
    Ok(CoverageTotals {
        catalogued_fragment_count,
        examined_fragment_count,
        unexamined_fragment_count,
    })
}

pub async fn summarize(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<(Vec<DocumentCoverage>, Option<Uuid>), AnalysisCoverageError> {
    if !(1..=100).contains(&limit) {
        return Err(AnalysisCoverageError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(AnalysisCoverageError::Invisible);
    }
    let rows: Vec<(Uuid, String, i64, i64)> = sqlx::query_as(
        "SELECT rs.document_id,d.format,COUNT(f.evidence_id)::bigint,COUNT(c.evidence_id)::bigint \
         FROM niccolo.situation_analysis_snapshots s \
         JOIN niccolo.situation_revision_sources rs \
           ON rs.workspace_id=s.workspace_id AND rs.situation_id=s.situation_id AND rs.revision=s.input_revision \
         JOIN niccolo.source_documents d \
           ON d.workspace_id=rs.workspace_id AND d.document_id=rs.document_id \
         LEFT JOIN niccolo.source_fragments f \
           ON f.workspace_id=rs.workspace_id AND f.document_id=rs.document_id \
         LEFT JOIN niccolo.situation_analysis_examined_fragments c \
           ON c.workspace_id=s.workspace_id AND c.situation_id=s.situation_id \
          AND c.analysis_id=s.analysis_id AND c.evidence_id=f.evidence_id \
         WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 \
           AND ($4::uuid IS NULL OR rs.document_id > $4) \
         GROUP BY rs.document_id,d.format ORDER BY rs.document_id LIMIT $5",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() as i64 > limit;
    let documents = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(document_id, format, catalogued_fragment_count, examined_fragment_count)| {
                DocumentCoverage {
                    document_id,
                    format,
                    catalogued_fragment_count,
                    examined_fragment_count,
                }
            },
        )
        .collect::<Vec<_>>();
    let next_after = has_more.then(|| documents.last().expect("non-empty page").document_id);
    tx.commit().await?;
    Ok((documents, next_after))
}
