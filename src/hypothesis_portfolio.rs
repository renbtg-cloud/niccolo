//! Durable investigation portfolio layered over candidate interpretations.
//!
//! Candidate interpretations remain hypotheses, not facts. Evidence links record how
//! observations are used; relations allow competing and compatible hypotheses to coexist.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum HypothesisPortfolioError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("analysis unavailable")]
    Invisible,
    #[error("invalid hypothesis portfolio input")]
    Invalid,
    #[error("hypothesis portfolio replay conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypothesisEvidenceLink {
    pub link_id: Uuid,
    pub evidence_id: Uuid,
    pub relation: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypothesisPrediction {
    pub prediction_id: Uuid,
    pub expected_observation: String,
    pub direction: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypothesisRelation {
    pub relation_id: Uuid,
    pub other_interpretation_id: Uuid,
    pub relation: String,
    pub role: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypothesisView {
    pub interpretation_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub latest_status: String,
    pub evidence_links: Vec<HypothesisEvidenceLink>,
    pub predictions: Vec<HypothesisPrediction>,
    pub relations: Vec<HypothesisRelation>,
    pub interpretation_is_fact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypothesisPortfolio {
    pub hypotheses: Vec<HypothesisView>,
    pub portfolio_establishes_truth: bool,
    pub forced_single_winner: bool,
    pub truncated: bool,
}

type HypothesisRow = (Uuid, String, String, Option<String>);
type EvidenceRow = (Uuid, Uuid, String, String);
type PredictionRow = (Uuid, String, String, String);
type RelationRow = (Uuid, Uuid, Uuid, String, String);

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, HypothesisPortfolioError> {
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
        return Err(HypothesisPortfolioError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HypothesisEvent {
    pub event_id: Uuid,
    pub interpretation_id: Uuid,
    pub event_kind: String,
    pub note: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

fn valid_note(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= 4096 && !value.contains('\0')
}

pub async fn append_event(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    interpretation_id: Uuid,
    event_id: Uuid,
    event_kind: &str,
    note: &str,
) -> Result<HypothesisEvent, HypothesisPortfolioError> {
    if !matches!(
        event_kind,
        "CHALLENGED" | "WEAKENED" | "RETAINED" | "RETIRED" | "RESTORED"
    ) || !valid_note(note)
    {
        return Err(HypothesisPortfolioError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_candidate_interpretations
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND interpretation_id=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(interpretation_id)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(HypothesisPortfolioError::Invisible);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_analysis_hypothesis_events
         (workspace_id,situation_id,analysis_id,interpretation_id,event_id,event_kind,note,recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(interpretation_id)
    .bind(event_id)
    .bind(event_kind)
    .bind(note.trim())
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row: Option<(String, String, Uuid)> = sqlx::query_as(
        "SELECT event_kind,note,recorded_by_principal_id
           FROM niccolo.situation_analysis_hypothesis_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND interpretation_id=$4 AND event_id=$5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(interpretation_id)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if row != Some((event_kind.to_owned(), note.trim().to_owned(), actor)) {
        return Err(HypothesisPortfolioError::Conflict);
    }
    tx.commit().await?;
    Ok(HypothesisEvent {
        event_id,
        interpretation_id,
        event_kind: event_kind.into(),
        note: note.trim().into(),
        recorded_by: actor,
        replayed: !inserted,
    })
}

pub async fn portfolio(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<HypothesisPortfolio, HypothesisPortfolioError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(HypothesisPortfolioError::Invisible);
    }

    let rows: Vec<HypothesisRow> = sqlx::query_as(
        "SELECT c.interpretation_id,c.interpretation_text,c.rationale,
                (SELECT e.event_kind
                   FROM niccolo.situation_analysis_hypothesis_events e
                  WHERE e.workspace_id=c.workspace_id
                    AND e.situation_id=c.situation_id
                    AND e.analysis_id=c.analysis_id
                    AND e.interpretation_id=c.interpretation_id
                  ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1)
           FROM niccolo.situation_analysis_candidate_interpretations c
          WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3
          ORDER BY c.created_at,c.interpretation_id
          LIMIT 65",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await?;
    let truncated = rows.len() > 64;
    let mut hypotheses = Vec::new();

    for (interpretation_id, text, rationale, status) in rows.into_iter().take(64) {
        let evidence_rows: Vec<EvidenceRow> = sqlx::query_as(
            "SELECT link_id,evidence_id,relation,rationale
               FROM niccolo.situation_analysis_hypothesis_evidence
              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND interpretation_id=$4
              ORDER BY created_at,link_id LIMIT 64",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(interpretation_id)
        .fetch_all(&mut *tx)
        .await?;
        let prediction_rows: Vec<PredictionRow> = sqlx::query_as(
            "SELECT prediction_id,expected_observation,direction,rationale
               FROM niccolo.situation_analysis_hypothesis_predictions
              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND interpretation_id=$4
              ORDER BY created_at,prediction_id LIMIT 32",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(interpretation_id)
        .fetch_all(&mut *tx)
        .await?;
        let relation_rows: Vec<RelationRow> = sqlx::query_as(
            "SELECT relation_id,left_interpretation_id,right_interpretation_id,relation,rationale
               FROM niccolo.situation_analysis_hypothesis_relations
              WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
                AND (left_interpretation_id=$4 OR right_interpretation_id=$4)
              ORDER BY created_at,relation_id LIMIT 64",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(interpretation_id)
        .fetch_all(&mut *tx)
        .await?;

        hypotheses.push(HypothesisView {
            interpretation_id,
            text,
            rationale,
            latest_status: status.unwrap_or_else(|| "LIVE".into()),
            evidence_links: evidence_rows
                .into_iter()
                .map(
                    |(link_id, evidence_id, relation, rationale)| HypothesisEvidenceLink {
                        link_id,
                        evidence_id,
                        relation,
                        rationale,
                    },
                )
                .collect(),
            predictions: prediction_rows
                .into_iter()
                .map(
                    |(prediction_id, expected_observation, direction, rationale)| {
                        HypothesisPrediction {
                            prediction_id,
                            expected_observation,
                            direction,
                            rationale,
                        }
                    },
                )
                .collect(),
            relations: relation_rows
                .into_iter()
                .map(
                    |(relation_id, left, right, relation, rationale)| HypothesisRelation {
                        relation_id,
                        other_interpretation_id: if left == interpretation_id {
                            right
                        } else {
                            left
                        },
                        relation,
                        role: if left == interpretation_id {
                            "LEFT".into()
                        } else {
                            "RIGHT".into()
                        },
                        rationale,
                    },
                )
                .collect(),
            interpretation_is_fact: false,
        });
    }
    tx.commit().await?;
    Ok(HypothesisPortfolio {
        hypotheses,
        portfolio_establishes_truth: false,
        forced_single_winner: false,
        truncated,
    })
}
