//! v0.121-v0.140: human-owned rationale, evidence gaps, goals, and next steps.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AgencyError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid agency input")]
    Invalid,
    #[error("agency replay conflicts with stored content")]
    Conflict,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, AgencyError> {
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
        return Err(AgencyError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
fn valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096
}

type DecisionRationaleRow = (Uuid, Option<Uuid>, String, Option<String>, Uuid);
type GoalRow = (Uuid, String, String, Uuid, Option<String>);
type NextStepRow = (Uuid, String, String, String, Option<String>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionRationaleInput {
    pub rationale_id: Uuid,
    pub option_id: Option<Uuid>,
    pub rationale: String,
    pub tradeoff_ack: Option<String>,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRationaleView {
    pub rationale_id: Uuid,
    pub option_id: Option<Uuid>,
    pub rationale: String,
    pub tradeoff_ack: Option<String>,
    pub recorded_by: Uuid,
    pub rationale_decides_for_human: bool,
}

pub async fn record_decision_rationale(
    pool: &PgPool,
    scope: AnalysisScope,
    decision: Uuid,
    input: &DecisionRationaleInput,
) -> Result<(), AgencyError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || decision.is_nil()
        || input.rationale_id.is_nil()
        || input.option_id.is_some_and(|id| id.is_nil())
        || input.recorded_by.is_nil()
        || !valid(&input.rationale)
        || input.tradeoff_ack.as_ref().is_some_and(|x| !valid(x))
    {
        return Err(AgencyError::Invalid);
    }
    let rationale = input.rationale.trim();
    let tradeoff_ack = input.tradeoff_ack.as_deref().map(str::trim);
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_decision_rationales(workspace_id,situation_id,analysis_id,decision_id,rationale_id,option_id,rationale_text,tradeoff_ack,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(decision)
        .bind(input.rationale_id)
        .bind(input.option_id)
        .bind(rationale)
        .bind(tradeoff_ack)
        .bind(input.recorded_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(Uuid, Option<Uuid>, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT decision_id,option_id,rationale_text,tradeoff_ack,recorded_by_principal_id
           FROM niccolo.situation_analysis_decision_rationales
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND rationale_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.rationale_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            decision,
            input.option_id,
            rationale.into(),
            tradeoff_ack.map(str::to_owned),
            input.recorded_by,
        ))
    {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn decision_rationales(
    pool: &PgPool,
    scope: AnalysisScope,
    decision: Uuid,
) -> Result<Vec<DecisionRationaleView>, AgencyError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || decision.is_nil()
    {
        return Err(AgencyError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<DecisionRationaleRow> = sqlx::query_as("SELECT rationale_id,option_id,rationale_text,tradeoff_ack,recorded_by_principal_id FROM niccolo.situation_analysis_decision_rationales WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND decision_id=$4 ORDER BY created_at,rationale_id")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(rationale_id, option_id, rationale, tradeoff_ack, recorded_by)| {
                DecisionRationaleView {
                    rationale_id,
                    option_id,
                    rationale,
                    tradeoff_ack,
                    recorded_by,
                    rationale_decides_for_human: false,
                }
            },
        )
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceGapInput {
    pub gap_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub description: String,
    pub requested_source_kind: Option<String>,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceGapEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub evidence_id: Option<Uuid>,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceGapView {
    pub gap_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub description: String,
    pub requested_source_kind: Option<String>,
    pub state: String,
    pub evidence_id: Option<Uuid>,
    pub satisfied_means_claim_true: bool,
    pub automatically_ranked: bool,
}

pub async fn record_evidence_gap(
    pool: &PgPool,
    scope: AnalysisScope,
    input: &EvidenceGapInput,
) -> Result<(), AgencyError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || input.gap_id.is_nil()
        || input.claim_id.is_some_and(|id| id.is_nil())
        || input.recorded_by.is_nil()
        || !valid(&input.description)
        || input
            .requested_source_kind
            .as_ref()
            .is_some_and(|x| x.trim().is_empty() || x.len() > 256)
    {
        return Err(AgencyError::Invalid);
    }
    let description = input.description.trim();
    let requested_source_kind = input.requested_source_kind.as_deref().map(str::trim);
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,claim_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(input.gap_id)
        .bind(input.claim_id)
        .bind(description)
        .bind(requested_source_kind)
        .bind(input.recorded_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(Option<Uuid>, String, Option<String>, Uuid)> = sqlx::query_as(
        "SELECT claim_id,description,requested_source_kind,recorded_by_principal_id
           FROM niccolo.situation_analysis_evidence_gaps
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND gap_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.gap_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            input.claim_id,
            description.into(),
            requested_source_kind.map(str::to_owned),
            input.recorded_by,
        ))
    {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn append_gap_event(
    pool: &PgPool,
    scope: AnalysisScope,
    gap: Uuid,
    event: &EvidenceGapEvent,
) -> Result<(), AgencyError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || gap.is_nil()
        || event.event_id.is_nil()
        || event.evidence_id.is_some_and(|id| id.is_nil())
        || event.recorded_by.is_nil()
        || !matches!(event.kind.as_str(), "SATISFIED" | "WAIVED" | "REOPENED")
        || !valid(&event.note)
        || (event.kind == "SATISFIED") != event.evidence_id.is_some()
    {
        return Err(AgencyError::Invalid);
    }
    let note = event.note.trim();
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gap_events(workspace_id,situation_id,analysis_id,gap_id,event_id,event_kind,evidence_id,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(gap)
        .bind(event.event_id)
        .bind(&event.kind)
        .bind(event.evidence_id)
        .bind(note)
        .bind(event.recorded_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(Uuid, String, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT gap_id,event_kind,evidence_id,note,recorded_by_principal_id
           FROM niccolo.situation_analysis_evidence_gap_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND event_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(event.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            gap,
            event.kind.clone(),
            event.evidence_id,
            note.into(),
            event.recorded_by,
        ))
    {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn evidence_gaps(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<EvidenceGapView>, AgencyError> {
    if scope.workspace.is_nil() || scope.situation.is_nil() || scope.analysis.is_nil() {
        return Err(AgencyError::Invalid);
    }
    type GapRow = (
        Uuid,
        Option<Uuid>,
        String,
        Option<String>,
        Option<String>,
        Option<Uuid>,
    );
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows:Vec<GapRow>=sqlx::query_as("SELECT g.gap_id,g.claim_id,g.description,g.requested_source_kind,(SELECT e.event_kind FROM niccolo.situation_analysis_evidence_gap_events e WHERE e.workspace_id=g.workspace_id AND e.situation_id=g.situation_id AND e.analysis_id=g.analysis_id AND e.gap_id=g.gap_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),(SELECT e.evidence_id FROM niccolo.situation_analysis_evidence_gap_events e WHERE e.workspace_id=g.workspace_id AND e.situation_id=g.situation_id AND e.analysis_id=g.analysis_id AND e.gap_id=g.gap_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_analysis_evidence_gaps g WHERE g.workspace_id=$1 AND g.situation_id=$2 AND g.analysis_id=$3 ORDER BY g.gap_id")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(gap_id, claim_id, description, requested_source_kind, state, evidence_id)| {
                EvidenceGapView {
                    gap_id,
                    claim_id,
                    description,
                    requested_source_kind,
                    state: state.unwrap_or_else(|| "OPEN".into()),
                    evidence_id,
                    satisfied_means_claim_true: false,
                    automatically_ranked: false,
                }
            },
        )
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalInput {
    pub goal_id: Uuid,
    pub goal: String,
    pub success_criterion: String,
    pub owner: Uuid,
    pub created_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalView {
    pub goal_id: Uuid,
    pub goal: String,
    pub success_criterion: String,
    pub owner: Uuid,
    pub state: String,
    pub system_owns_goal: bool,
}

pub async fn create_goal(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    input: &GoalInput,
) -> Result<(), AgencyError> {
    if workspace.is_nil()
        || situation.is_nil()
        || input.goal_id.is_nil()
        || input.owner.is_nil()
        || input.created_by.is_nil()
        || !valid(&input.goal)
        || !valid(&input.success_criterion)
    {
        return Err(AgencyError::Invalid);
    }
    let goal = input.goal.trim();
    let success_criterion = input.success_criterion.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_goals(workspace_id,situation_id,goal_id,goal_text,success_criterion,owner_principal_id,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(workspace)
        .bind(situation)
        .bind(input.goal_id)
        .bind(goal)
        .bind(success_criterion)
        .bind(input.owner)
        .bind(input.created_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(String, String, Uuid, Uuid)> = sqlx::query_as(
        "SELECT goal_text,success_criterion,owner_principal_id,created_by_principal_id
           FROM niccolo.situation_goals
          WHERE workspace_id=$1 AND situation_id=$2 AND goal_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(input.goal_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            goal.into(),
            success_criterion.into(),
            input.owner,
            input.created_by,
        ))
    {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn append_goal_event(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    goal: Uuid,
    event: &GoalEvent,
) -> Result<(), AgencyError> {
    if workspace.is_nil()
        || situation.is_nil()
        || goal.is_nil()
        || event.event_id.is_nil()
        || event.recorded_by.is_nil()
        || !matches!(event.kind.as_str(), "ACHIEVED" | "REOPENED" | "ABANDONED")
        || !valid(&event.note)
    {
        return Err(AgencyError::Invalid);
    }
    let note = event.note.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_goal_events(workspace_id,situation_id,goal_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(workspace)
        .bind(situation)
        .bind(goal)
        .bind(event.event_id)
        .bind(&event.kind)
        .bind(note)
        .bind(event.recorded_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT goal_id,event_kind,note,recorded_by_principal_id
           FROM niccolo.situation_goal_events
          WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((goal, event.kind.clone(), note.into(), event.recorded_by)) {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn goals(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<GoalView>, AgencyError> {
    if workspace.is_nil() || situation.is_nil() {
        return Err(AgencyError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<GoalRow> = sqlx::query_as("SELECT g.goal_id,g.goal_text,g.success_criterion,g.owner_principal_id,(SELECT e.event_kind FROM niccolo.situation_goal_events e WHERE e.workspace_id=g.workspace_id AND e.situation_id=g.situation_id AND e.goal_id=g.goal_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_goals g WHERE g.workspace_id=$1 AND g.situation_id=$2 ORDER BY g.goal_id")
        .bind(workspace).bind(situation).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(goal_id, goal, success_criterion, owner, state)| GoalView {
                goal_id,
                goal,
                success_criterion,
                owner,
                state: state.unwrap_or_else(|| "ACTIVE".into()),
                system_owns_goal: false,
            },
        )
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextStepInput {
    pub step_id: Uuid,
    pub kind: String,
    pub text: String,
    pub rationale: String,
    pub proposed_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NextStepEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextStepView {
    pub step_id: Uuid,
    pub kind: String,
    pub text: String,
    pub rationale: String,
    pub state: String,
    pub automatically_ranked: bool,
    pub accepted_means_executed: bool,
    pub system_may_execute_without_human: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextStepPackage {
    pub steps: Vec<NextStepView>,
    pub human_owns_next_action: bool,
    pub package_executes_actions: bool,
}

pub async fn propose_next_step(
    pool: &PgPool,
    scope: AnalysisScope,
    input: &NextStepInput,
) -> Result<(), AgencyError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || input.step_id.is_nil()
        || input.proposed_by.is_nil()
        || !matches!(
            input.kind.as_str(),
            "ASK_HUMAN" | "GATHER_EVIDENCE" | "REANALYZE" | "USER_ACTION"
        )
        || !valid(&input.text)
        || !valid(&input.rationale)
    {
        return Err(AgencyError::Invalid);
    }
    let text = input.text.trim();
    let rationale = input.rationale.trim();
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_next_steps(workspace_id,situation_id,analysis_id,step_id,step_kind,step_text,rationale,proposed_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(input.step_id)
        .bind(&input.kind)
        .bind(text)
        .bind(rationale)
        .bind(input.proposed_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(String, String, String, Uuid)> = sqlx::query_as(
        "SELECT step_kind,step_text,rationale,proposed_by_principal_id
           FROM niccolo.situation_analysis_next_steps
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND step_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(input.step_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            input.kind.clone(),
            text.into(),
            rationale.into(),
            input.proposed_by,
        ))
    {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn append_next_step_event(
    pool: &PgPool,
    scope: AnalysisScope,
    step: Uuid,
    event: &NextStepEvent,
) -> Result<(), AgencyError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || step.is_nil()
        || event.event_id.is_nil()
        || event.recorded_by.is_nil()
        || !matches!(
            event.kind.as_str(),
            "ACCEPTED" | "DEFERRED" | "DISMISSED" | "REOPENED"
        )
        || !valid(&event.note)
    {
        return Err(AgencyError::Invalid);
    }
    let note = event.note.trim();
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_next_step_events(workspace_id,situation_id,analysis_id,step_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(step)
        .bind(event.event_id)
        .bind(&event.kind)
        .bind(note)
        .bind(event.recorded_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT step_id,event_kind,note,recorded_by_principal_id
           FROM niccolo.situation_analysis_next_step_events
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND event_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .bind(event.event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((step, event.kind.clone(), note.into(), event.recorded_by)) {
        return Err(AgencyError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn next_steps(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<NextStepPackage, AgencyError> {
    if scope.workspace.is_nil() || scope.situation.is_nil() || scope.analysis.is_nil() {
        return Err(AgencyError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<NextStepRow> = sqlx::query_as("SELECT s.step_id,s.step_kind,s.step_text,s.rationale,(SELECT e.event_kind FROM niccolo.situation_analysis_next_step_events e WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id AND e.analysis_id=s.analysis_id AND e.step_id=s.step_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_analysis_next_steps s WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3 ORDER BY s.step_id")
        .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(NextStepPackage {
        steps: rows
            .into_iter()
            .map(|(step_id, kind, text, rationale, state)| NextStepView {
                step_id,
                kind,
                text,
                rationale,
                state: state.unwrap_or_else(|| "PROPOSED".into()),
                automatically_ranked: false,
                accepted_means_executed: false,
                system_may_execute_without_human: false,
            })
            .collect(),
        human_owns_next_action: true,
        package_executes_actions: false,
    })
}

pub fn next_steps_sha256(value: &NextStepPackage) -> Result<String, serde_json::Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_string(value)?.as_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> AnalysisScope {
        AnalysisScope {
            workspace: Uuid::new_v4(),
            situation: Uuid::new_v4(),
            analysis: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn invalid_agency_identifiers_fail_before_database_access() {
        let pool = PgPool::connect_lazy("postgres://127.0.0.1:1/never").unwrap();
        let mut invalid = scope();
        invalid.workspace = Uuid::nil();
        assert!(matches!(
            evidence_gaps(&pool, invalid).await,
            Err(AgencyError::Invalid)
        ));
        assert!(matches!(
            goals(&pool, Uuid::new_v4(), Uuid::nil()).await,
            Err(AgencyError::Invalid)
        ));
        assert!(matches!(
            decision_rationales(&pool, scope(), Uuid::nil()).await,
            Err(AgencyError::Invalid)
        ));
        assert!(matches!(
            next_steps(
                &pool,
                AnalysisScope {
                    workspace: Uuid::new_v4(),
                    situation: Uuid::nil(),
                    analysis: Uuid::new_v4(),
                },
            )
            .await,
            Err(AgencyError::Invalid)
        ));
    }
}
