//! Immutable completed answers bound to one frozen analysis-input snapshot.
//!
//! Publishing an answer never edits the Situation or a previous answer. A later
//! reanalysis gets a new analysis id and can coexist with the historical output.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisOutputError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid analysis output")]
    Invalid,
    #[error("analysis output replay conflicts with stored answer")]
    Conflict,
    #[error("analysis snapshot or output is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisOutput {
    pub analysis_id: Uuid,
    pub situation_id: Uuid,
    pub input_revision: i64,
    pub input_fingerprint: String,
    pub answer_text: String,
    pub coverage_note: String,
    pub published_by: Uuid,
    pub current_revision: i64,
    pub input_is_current: bool,
    pub replayed: bool,
}

fn clean(input: &str, max_bytes: usize) -> Result<String, AnalysisOutputError> {
    let value = input.trim();
    if value.is_empty() || value.len() > max_bytes || value.contains('\0') {
        return Err(AnalysisOutputError::Invalid);
    }
    Ok(value.to_owned())
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AnalysisOutputError> {
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
        return Err(AnalysisOutputError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

type OutputRow = (i64, String, String, String, Uuid, i64);

async fn load_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<Option<OutputRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT s.input_revision,s.input_fingerprint,o.answer_text,o.coverage_note,\
                o.published_by_principal_id,z.current_revision \
         FROM niccolo.situation_analysis_outputs o \
         JOIN niccolo.situation_analysis_snapshots s \
           USING(workspace_id,situation_id,analysis_id) \
         JOIN niccolo.situations z USING(workspace_id,situation_id) \
         WHERE o.workspace_id=$1 AND o.situation_id=$2 AND o.analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut **tx)
    .await
}

fn from_row(analysis: Uuid, situation: Uuid, row: OutputRow, replayed: bool) -> AnalysisOutput {
    let (input_revision, input_fingerprint, answer_text, coverage_note, published_by, current) =
        row;
    AnalysisOutput {
        analysis_id: analysis,
        situation_id: situation,
        input_revision,
        input_fingerprint,
        answer_text,
        coverage_note,
        published_by,
        current_revision: current,
        input_is_current: current == input_revision,
        replayed,
    }
}

pub async fn publish(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    analysis: Uuid,
    answer_text: &str,
    coverage_note: &str,
) -> Result<AnalysisOutput, AnalysisOutputError> {
    // These are POC transport/storage safety bounds, not product/model token ceilings.
    let answer_text = clean(answer_text, 1024 * 1024)?;
    let coverage_note = clean(coverage_note, 64 * 1024)?;
    let mut tx = scoped(pool, workspace, false).await?;

    if let Some(row) = load_in_tx(&mut tx, workspace, situation, analysis).await? {
        if row.2 != answer_text || row.3 != coverage_note || row.4 != actor {
            return Err(AnalysisOutputError::Conflict);
        }
        tx.commit().await?;
        return Ok(from_row(analysis, situation, row, true));
    }

    let snapshot_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !snapshot_exists {
        return Err(AnalysisOutputError::Invisible);
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_outputs \
         (workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6) \
         ON CONFLICT(workspace_id,situation_id,analysis_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(&answer_text)
    .bind(&coverage_note)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let Some(row) = load_in_tx(&mut tx, workspace, situation, analysis).await? else {
        return Err(AnalysisOutputError::Invisible);
    };
    if !inserted && (row.2 != answer_text || row.3 != coverage_note || row.4 != actor) {
        return Err(AnalysisOutputError::Conflict);
    }
    tx.commit().await?;
    Ok(from_row(analysis, situation, row, !inserted))
}

pub async fn get(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<AnalysisOutput, AnalysisOutputError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let Some(row) = load_in_tx(&mut tx, workspace, situation, analysis).await? else {
        return Err(AnalysisOutputError::Invisible);
    };
    tx.commit().await?;
    Ok(from_row(analysis, situation, row, false))
}
