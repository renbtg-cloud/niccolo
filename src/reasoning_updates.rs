//! v0.801-v0.850: structured intermediate reasoning updates.
//! These records feed later reasoning steps but remain explicit assumptions/gaps/questions, not facts.
use crate::{analysis_structure::AnalysisScope, observation_query_ledger::{self, SourceQueryIntent}};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid reasoning update")]
    Invalid,
    #[error("reasoning update replay conflict")]
    Conflict,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateInterpretation {
    pub interpretation_id: Uuid,
    pub text: String,
    pub rationale: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAssumption {
    pub assumption_id: Uuid,
    pub statement: String,
    pub basis_note: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateGap {
    pub gap_id: Uuid,
    pub description: String,
    pub requested_source_kind: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateClarification {
    pub question_id: Uuid,
    pub text: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningUpdate {
    #[serde(default)]
    pub used_contribution_ids: Vec<Uuid>,
    #[serde(default)]
    pub candidate_interpretations: Vec<UpdateInterpretation>,
    #[serde(default)]
    pub assumptions: Vec<UpdateAssumption>,
    #[serde(default)]
    pub evidence_gaps: Vec<UpdateGap>,
    #[serde(default)]
    pub clarifications: Vec<UpdateClarification>,
    #[serde(default)]
    pub source_queries: Vec<SourceQueryIntent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedUpdate {
    pub used_contribution_count: usize,
    pub candidate_interpretation_count: usize,
    pub assumption_count: usize,
    pub evidence_gap_count: usize,
    pub clarification_count: usize,
    pub source_query_count: usize,
    pub update_asserts_truth: bool,
}
fn valid(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.contains('\0')
}
fn validate(x: &ReasoningUpdate) -> Result<(), UpdateError> {
    if x.used_contribution_ids.len() > 256
        || x.candidate_interpretations.len() > 64
        || x.assumptions.len() > 64
        || x.evidence_gaps.len() > 64
        || x.clarifications.len() > 32
        || x.source_queries.len() > 3
        || x.used_contribution_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != x.used_contribution_ids.len()
    {
        return Err(UpdateError::Invalid);
    };
    let mut ids = std::collections::BTreeSet::new();
    for interpretation in &x.candidate_interpretations {
        if !ids.insert(interpretation.interpretation_id)
            || !valid(&interpretation.text, 4096)
            || !valid(&interpretation.rationale, 4096)
        {
            return Err(UpdateError::Invalid);
        }
    }
    ids.clear();
    for query in &x.source_queries {
        sqlx::query(
            "INSERT INTO niccolo.situation_analysis_observation_queries              (workspace_id,situation_id,analysis_id,query_id,query_text,rationale,expected_observation,direction,interpretation_id,recorded_by_principal_id)              VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(query.query_id)
        .bind(query.query.trim())
        .bind(query.rationale.trim())
        .bind(query.expected_observation.trim())
        .bind(&query.direction)
        .bind(query.interpretation_id)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
        let stored: Option<(String, String, String, String, Option<Uuid>, Uuid)> =
            sqlx::query_as(
                "SELECT query_text,rationale,expected_observation,direction,interpretation_id,recorded_by_principal_id                  FROM niccolo.situation_analysis_observation_queries                  WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND query_id=$4",
            )
            .bind(scope.workspace)
            .bind(scope.situation)
            .bind(scope.analysis)
            .bind(query.query_id)
            .fetch_optional(&mut *tx)
            .await?;
        if stored != Some((
            query.query.trim().to_owned(),
            query.rationale.trim().to_owned(),
            query.expected_observation.trim().to_owned(),
            query.direction.clone(),
            query.interpretation_id,
            actor,
        )) {
            return Err(UpdateError::Conflict);
        }
    }
    for a in &x.assumptions {
        if !ids.insert(a.assumption_id) || !valid(&a.statement, 4096) || !valid(&a.basis_note, 4096)
        {
            return Err(UpdateError::Invalid);
        }
    }
    ids.clear();
    for g in &x.evidence_gaps {
        if !ids.insert(g.gap_id)
            || !valid(&g.description, 4096)
            || g.requested_source_kind
                .as_ref()
                .is_some_and(|v| v.trim().is_empty() || v.len() > 256)
        {
            return Err(UpdateError::Invalid);
        }
    }
    ids.clear();
    for q in &x.clarifications {
        if !ids.insert(q.question_id) || !valid(&q.text, 4096) || !valid(&q.reason, 2048) {
            return Err(UpdateError::Invalid);
        }
    }
    ids.clear();
    for q in &x.source_queries {
        if !ids.insert(q.query_id) || !observation_query_ledger::valid_intent(q) {
            return Err(UpdateError::Invalid);
        }
    }
    Ok(())
}
async fn scoped(pool: &PgPool, w: Uuid) -> Result<Transaction<'_, Postgres>, UpdateError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    let p: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if p {
        return Err(UpdateError::Invalid);
    };
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
pub fn parse(v: &Value) -> Result<Option<ReasoningUpdate>, UpdateError> {
    let Some(x) = v.get("niccolo_update") else {
        return Ok(None);
    };
    let x: ReasoningUpdate = serde_json::from_value(x.clone())?;
    validate(&x)?;
    Ok(Some(x))
}
pub async fn apply(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    x: &ReasoningUpdate,
) -> Result<AppliedUpdate, UpdateError> {
    validate(x)?;
    let mut tx = scoped(pool, scope.workspace).await?;
    for contribution_id in &x.used_contribution_ids {
        sqlx::query(
            "INSERT INTO niccolo.situation_analysis_used_contributions \
             (workspace_id,situation_id,analysis_id,input_revision,contribution_id,recorded_by_principal_id) \
             SELECT s.workspace_id,s.situation_id,s.analysis_id,s.input_revision,$4,$5 \
             FROM niccolo.situation_analysis_snapshots s \
             JOIN niccolo.situation_revision_contributions rc \
               ON rc.workspace_id=s.workspace_id AND rc.situation_id=s.situation_id \
              AND rc.revision=s.input_revision AND rc.contribution_id=$4 \
             WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 \
             ON CONFLICT DO NOTHING",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(*contribution_id)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
        let recorded_by: Option<Uuid> = sqlx::query_scalar(
            "SELECT recorded_by_principal_id FROM niccolo.situation_analysis_used_contributions \
             WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND contribution_id=$4",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(*contribution_id)
        .fetch_optional(&mut *tx)
        .await?;
        match recorded_by {
            Some(id) if id == actor => {}
            Some(_) => return Err(UpdateError::Conflict),
            None => return Err(UpdateError::Invalid),
        }
    }
    for interpretation in &x.candidate_interpretations {
        sqlx::query("INSERT INTO niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id,interpretation_text,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(scope.workspace)
            .bind(scope.situation)
            .bind(scope.analysis)
            .bind(interpretation.interpretation_id)
            .bind(interpretation.text.trim())
            .bind(interpretation.rationale.trim())
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        let row: Option<(String, String, Uuid)> = sqlx::query_as(
            "SELECT interpretation_text,rationale,recorded_by_principal_id FROM niccolo.situation_analysis_candidate_interpretations WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND interpretation_id=$4",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(interpretation.interpretation_id)
        .fetch_optional(&mut *tx)
        .await?;
        if row
            != Some((
                interpretation.text.trim().into(),
                interpretation.rationale.trim().into(),
                actor,
            ))
        {
            return Err(UpdateError::Conflict);
        }
    }
    for a in &x.assumptions {
        sqlx::query("INSERT INTO niccolo.situation_analysis_assumptions(workspace_id,situation_id,analysis_id,assumption_id,claim_id,statement,basis_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,NULL,$5,$6,$7) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.assumption_id).bind(a.statement.trim()).bind(a.basis_note.trim()).bind(actor).execute(&mut *tx).await?;
        let row:Option<(String,String,Uuid)>=sqlx::query_as("SELECT statement,basis_note,recorded_by_principal_id FROM niccolo.situation_analysis_assumptions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND assumption_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.assumption_id).fetch_optional(&mut *tx).await?;
        if row != Some((a.statement.trim().into(), a.basis_note.trim().into(), actor)) {
            return Err(UpdateError::Conflict);
        }
    }
    for g in &x.evidence_gaps {
        sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,claim_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,NULL,$5,$6,$7) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(g.gap_id).bind(g.description.trim()).bind(g.requested_source_kind.as_deref().map(str::trim)).bind(actor).execute(&mut *tx).await?;
        let row:Option<(String,Option<String>,Uuid)>=sqlx::query_as("SELECT description,requested_source_kind,recorded_by_principal_id FROM niccolo.situation_analysis_evidence_gaps WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND gap_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(g.gap_id).fetch_optional(&mut *tx).await?;
        if row
            != Some((
                g.description.trim().into(),
                g.requested_source_kind
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_owned),
                actor,
            ))
        {
            return Err(UpdateError::Conflict);
        }
    }
    for q in &x.clarifications {
        sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(q.question_id).bind(q.text.trim()).execute(&mut *tx).await?;
        let t:Option<String>=sqlx::query_scalar("SELECT question FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2").bind(scope.workspace).bind(q.question_id).fetch_optional(&mut *tx).await?;
        if t.as_deref() != Some(q.text.trim()) {
            return Err(UpdateError::Conflict);
        };
        sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(q.question_id).bind(q.reason.trim()).bind(actor).execute(&mut *tx).await?;
        let row:Option<(String,Uuid)>=sqlx::query_as("SELECT reason,proposed_by_principal_id FROM niccolo.situation_analysis_clarification_questions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND question_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(q.question_id).fetch_optional(&mut *tx).await?;
        if row != Some((q.reason.trim().into(), actor)) {
            return Err(UpdateError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(AppliedUpdate {
        used_contribution_count: x.used_contribution_ids.len(),
        candidate_interpretation_count: x.candidate_interpretations.len(),
        assumption_count: x.assumptions.len(),
        evidence_gap_count: x.evidence_gaps.len(),
        clarification_count: x.clarifications.len(),
        source_query_count: x.source_queries.len(),
        update_asserts_truth: false,
    })
}
