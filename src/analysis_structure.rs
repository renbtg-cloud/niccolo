//! v0.39-v0.42: explicit claim structure and uncertainty history.
//! Records describe reasoning structure and known limits; they never establish factual truth.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisScope {
    pub workspace: Uuid,
    pub situation: Uuid,
    pub analysis: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum StructureError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid request")]
    Invalid,
    #[error("idempotency conflict")]
    Conflict,
    #[error("record is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimRelation {
    pub relation_id: Uuid,
    pub source_claim_id: Uuid,
    pub target_claim_id: Uuid,
    pub relation: String,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uncertainty {
    pub uncertainty_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub kind: String,
    pub statement: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UncertaintyEvent {
    pub uncertainty_id: Uuid,
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub target_analysis_id: Option<Uuid>,
    pub recorded_by: Uuid,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, StructureError> {
    let mut tx = pool.begin().await?;
    let mode = if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    };
    sqlx::query(mode).execute(&mut *tx).await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(StructureError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn relation_kind(v: &str) -> bool {
    matches!(
        v,
        "SUPPORTS" | "QUALIFIES" | "TENSIONS_WITH" | "ALTERNATIVE_TO" | "DEPENDS_ON"
    )
}

pub async fn record_relation(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &ClaimRelation,
) -> Result<(), StructureError> {
    if item.source_claim_id == item.target_claim_id
        || !relation_kind(&item.relation)
        || item.rationale.trim().is_empty()
        || item.rationale.len() > 4096
    {
        return Err(StructureError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let result = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_relations \
         (workspace_id,situation_id,analysis_id,relation_id,source_claim_id,target_claim_id,relation,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.relation_id)
    .bind(item.source_claim_id)
    .bind(item.target_claim_id)
    .bind(&item.relation)
    .bind(&item.rationale)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        let existing: Option<(Uuid, Uuid, String, String, Uuid)> = sqlx::query_as(
            "SELECT source_claim_id,target_claim_id,relation,rationale,recorded_by_principal_id \
             FROM niccolo.situation_analysis_claim_relations \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND relation_id=$4",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(item.relation_id)
        .fetch_optional(&mut *tx)
        .await?;
        let expected = Some((
            item.source_claim_id,
            item.target_claim_id,
            item.relation.clone(),
            item.rationale.clone(),
            item.recorded_by,
        ));
        if existing != expected {
            return Err(StructureError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_relations(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<ClaimRelation>, StructureError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(Uuid, Uuid, Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT relation_id,source_claim_id,target_claim_id,relation,rationale,recorded_by_principal_id \
         FROM niccolo.situation_analysis_claim_relations \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         ORDER BY source_claim_id,target_claim_id,relation,relation_id",
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
            |(relation_id, source_claim_id, target_claim_id, relation, rationale, recorded_by)| {
                ClaimRelation {
                    relation_id,
                    source_claim_id,
                    target_claim_id,
                    relation,
                    rationale,
                    recorded_by,
                }
            },
        )
        .collect())
}

fn uncertainty_kind(v: &str) -> bool {
    matches!(
        v,
        "MISSING_EVIDENCE" | "AMBIGUITY" | "CONFLICT" | "ASSUMPTION" | "UNKNOWN"
    )
}

pub async fn record_uncertainty(
    pool: &PgPool,
    scope: AnalysisScope,
    item: &Uncertainty,
) -> Result<(), StructureError> {
    if !uncertainty_kind(&item.kind)
        || item.statement.trim().is_empty()
        || item.statement.len() > 4096
    {
        return Err(StructureError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_uncertainties \
         (workspace_id,situation_id,analysis_id,uncertainty_id,claim_id,uncertainty_kind,statement,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.uncertainty_id)
    .bind(item.claim_id)
    .bind(&item.kind)
    .bind(&item.statement)
    .bind(item.recorded_by)
    .execute(&mut *tx)
    .await?;
    let existing: Option<(Option<Uuid>, String, String, Uuid)> = sqlx::query_as(
        "SELECT claim_id,uncertainty_kind,statement,recorded_by_principal_id \
         FROM niccolo.situation_analysis_uncertainties \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND uncertainty_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(item.uncertainty_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        != Some((
            item.claim_id,
            item.kind.clone(),
            item.statement.clone(),
            item.recorded_by,
        ))
    {
        return Err(StructureError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}

fn uncertainty_event_kind(v: &str) -> bool {
    matches!(
        v,
        "CLARIFIED" | "REMAINS" | "OUT_OF_SCOPE" | "REQUIRES_SOURCE" | "REPHRASED"
    )
}

pub async fn append_uncertainty_event(
    pool: &PgPool,
    scope: AnalysisScope,
    event: &UncertaintyEvent,
) -> Result<(), StructureError> {
    if !uncertainty_event_kind(&event.kind)
        || event.note.trim().is_empty()
        || event.note.len() > 4096
        || (event.kind == "CLARIFIED" && event.target_analysis_id.is_none())
    {
        return Err(StructureError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_uncertainty_events \
         (workspace_id,situation_id,analysis_id,uncertainty_id,event_id,event_kind,note,target_analysis_id,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(event.uncertainty_id)
    .bind(event.event_id)
    .bind(&event.kind)
    .bind(&event.note)
    .bind(event.target_analysis_id)
    .bind(event.recorded_by)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
