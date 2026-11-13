//! v0.141-v0.160: explicit human priorities, constraints, projected consequences, and stop gates.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid control input")]
    Invalid,
    #[error("control replay conflicts with stored content")]
    Conflict,
}
async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    ro: bool,
) -> Result<Transaction<'_, Postgres>, ControlError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if ro {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let p: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if p {
        return Err(ControlError::Privileged);
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorityInput {
    pub priority_id: Uuid,
    pub ordinal: i32,
    pub text: String,
    pub rationale: String,
    pub declared_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityView {
    pub priority_id: Uuid,
    pub ordinal: i32,
    pub text: String,
    pub rationale: String,
    pub declared_by: Uuid,
    pub automatically_reordered: bool,
}
pub async fn declare_priority(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    p: &PriorityInput,
) -> Result<(), ControlError> {
    if workspace.is_nil()
        || situation.is_nil()
        || p.priority_id.is_nil()
        || p.declared_by.is_nil()
        || p.ordinal < 1
        || !valid(&p.text)
        || !valid(&p.rationale)
    {
        return Err(ControlError::Invalid);
    }
    let text = p.text.trim();
    let rationale = p.rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_human_priorities(workspace_id,situation_id,priority_id,ordinal,priority_text,rationale,declared_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(workspace)
        .bind(situation)
        .bind(p.priority_id)
        .bind(p.ordinal)
        .bind(text)
        .bind(rationale)
        .bind(p.declared_by)
        .execute(&mut *tx)
        .await?;
    let stored: Option<(i32, String, String, Uuid)> = sqlx::query_as(
        "SELECT ordinal,priority_text,rationale,declared_by_principal_id
           FROM niccolo.situation_human_priorities
          WHERE workspace_id=$1 AND situation_id=$2 AND priority_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(p.priority_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((p.ordinal, text.into(), rationale.into(), p.declared_by)) {
        return Err(ControlError::Conflict);
    }
    tx.commit().await?;
    Ok(())
}
pub async fn priorities(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<PriorityView>, ControlError> {
    if workspace.is_nil() || situation.is_nil() {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let rows:Vec<(Uuid,i32,String,String,Uuid)>=sqlx::query_as("SELECT priority_id,ordinal,priority_text,rationale,declared_by_principal_id FROM niccolo.situation_human_priorities WHERE workspace_id=$1 AND situation_id=$2 ORDER BY ordinal,priority_id").bind(workspace).bind(situation).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(priority_id, ordinal, text, rationale, declared_by)| PriorityView {
                priority_id,
                ordinal,
                text,
                rationale,
                declared_by,
                automatically_reordered: false,
            },
        )
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintInput {
    pub constraint_id: Uuid,
    pub kind: String,
    pub text: String,
    pub declared_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintView {
    pub constraint_id: Uuid,
    pub kind: String,
    pub text: String,
    pub state: String,
    pub system_may_waive: bool,
}
pub async fn declare_constraint(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    c: &ConstraintInput,
) -> Result<(), ControlError> {
    if workspace.is_nil()
        || situation.is_nil()
        || c.constraint_id.is_nil()
        || c.declared_by.is_nil()
        || !matches!(c.kind.as_str(), "MUST" | "MUST_NOT" | "PREFERENCE")
        || !valid(&c.text)
    {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_human_constraints(workspace_id,situation_id,constraint_id,constraint_kind,constraint_text,declared_by_principal_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(c.constraint_id).bind(&c.kind).bind(&c.text).bind(c.declared_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn append_constraint_event(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    constraint: Uuid,
    e: &ConstraintEvent,
) -> Result<(), ControlError> {
    if workspace.is_nil()
        || situation.is_nil()
        || constraint.is_nil()
        || e.event_id.is_nil()
        || e.recorded_by.is_nil()
        || !matches!(e.kind.as_str(), "WAIVED" | "REINSTATED")
        || !valid(&e.note)
    {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_human_constraint_events(workspace_id,situation_id,constraint_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(workspace).bind(situation).bind(constraint).bind(e.event_id).bind(&e.kind).bind(&e.note).bind(e.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn constraints(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<ConstraintView>, ControlError> {
    if workspace.is_nil() || situation.is_nil() {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let rows:Vec<(Uuid,String,String,Option<String>)>=sqlx::query_as("SELECT c.constraint_id,c.constraint_kind,c.constraint_text,(SELECT e.event_kind FROM niccolo.situation_human_constraint_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.constraint_id=c.constraint_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_human_constraints c WHERE c.workspace_id=$1 AND c.situation_id=$2 ORDER BY c.constraint_id").bind(workspace).bind(situation).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(constraint_id, kind, text, state)| ConstraintView {
            constraint_id,
            kind,
            text,
            state: state.unwrap_or_else(|| "ACTIVE".into()),
            system_may_waive: false,
        })
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsequenceInput {
    pub consequence_id: Uuid,
    pub text: String,
    pub uncertainty_note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsequenceView {
    pub consequence_id: Uuid,
    pub text: String,
    pub uncertainty_note: String,
    pub projection_is_fact: bool,
    pub automatically_recommends_option: bool,
}
pub async fn add_consequence(
    pool: &PgPool,
    scope: AnalysisScope,
    decision: Uuid,
    option: Uuid,
    c: &ConsequenceInput,
) -> Result<(), ControlError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || decision.is_nil()
        || option.is_nil()
        || c.consequence_id.is_nil()
        || c.recorded_by.is_nil()
        || !valid(&c.text)
        || !valid(&c.uncertainty_note)
    {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_option_consequences(workspace_id,situation_id,analysis_id,decision_id,option_id,consequence_id,consequence_text,uncertainty_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision).bind(option).bind(c.consequence_id).bind(&c.text).bind(&c.uncertainty_note).bind(c.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn consequences(
    pool: &PgPool,
    scope: AnalysisScope,
    decision: Uuid,
    option: Uuid,
) -> Result<Vec<ConsequenceView>, ControlError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || decision.is_nil()
        || option.is_nil()
    {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows:Vec<(Uuid,String,String)>=sqlx::query_as("SELECT consequence_id,consequence_text,uncertainty_note FROM niccolo.situation_analysis_option_consequences WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND decision_id=$4 AND option_id=$5 ORDER BY consequence_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision).bind(option).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(consequence_id, text, uncertainty_note)| ConsequenceView {
            consequence_id,
            text,
            uncertainty_note,
            projection_is_fact: false,
            automatically_recommends_option: false,
        })
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopConditionInput {
    pub condition_id: Uuid,
    pub text: String,
    pub escalation_note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopConditionEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopConditionView {
    pub condition_id: Uuid,
    pub text: String,
    pub escalation_note: String,
    pub state: String,
    pub automation_allowed: bool,
    pub requires_human_handling: bool,
}
pub async fn add_stop_condition(
    pool: &PgPool,
    scope: AnalysisScope,
    c: &StopConditionInput,
) -> Result<(), ControlError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || c.condition_id.is_nil()
        || c.recorded_by.is_nil()
        || !valid(&c.text)
        || !valid(&c.escalation_note)
    {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_stop_conditions(workspace_id,situation_id,analysis_id,condition_id,condition_text,escalation_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(c.condition_id).bind(&c.text).bind(&c.escalation_note).bind(c.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn append_stop_event(
    pool: &PgPool,
    scope: AnalysisScope,
    condition: Uuid,
    e: &StopConditionEvent,
) -> Result<(), ControlError> {
    if scope.workspace.is_nil()
        || scope.situation.is_nil()
        || scope.analysis.is_nil()
        || condition.is_nil()
        || e.event_id.is_nil()
        || e.recorded_by.is_nil()
        || !matches!(e.kind.as_str(), "TRIGGERED" | "CLEARED")
        || !valid(&e.note)
    {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_stop_condition_events(workspace_id,situation_id,analysis_id,condition_id,event_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(condition).bind(e.event_id).bind(&e.kind).bind(&e.note).bind(e.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn stop_conditions(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<Vec<StopConditionView>, ControlError> {
    if scope.workspace.is_nil() || scope.situation.is_nil() || scope.analysis.is_nil() {
        return Err(ControlError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows:Vec<(Uuid,String,String,Option<String>)>=sqlx::query_as("SELECT c.condition_id,c.condition_text,c.escalation_note,(SELECT e.event_kind FROM niccolo.situation_analysis_stop_condition_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id AND e.condition_id=c.condition_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_analysis_stop_conditions c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 ORDER BY c.condition_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(condition_id, text, escalation_note, state)| {
            let state = state.unwrap_or_else(|| "CLEAR".into());
            let triggered = state == "TRIGGERED";
            StopConditionView {
                condition_id,
                text,
                escalation_note,
                state,
                automation_allowed: !triggered,
                requires_human_handling: triggered,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_control_identifiers_fail_before_database_access() {
        let pool = PgPool::connect_lazy("postgres://127.0.0.1:1/never").unwrap();
        assert!(matches!(
            priorities(&pool, Uuid::nil(), Uuid::new_v4()).await,
            Err(ControlError::Invalid)
        ));
        assert!(matches!(
            constraints(&pool, Uuid::new_v4(), Uuid::nil()).await,
            Err(ControlError::Invalid)
        ));
        assert!(matches!(
            consequences(
                &pool,
                AnalysisScope {
                    workspace: Uuid::new_v4(),
                    situation: Uuid::new_v4(),
                    analysis: Uuid::nil(),
                },
                Uuid::new_v4(),
                Uuid::new_v4()
            )
            .await,
            Err(ControlError::Invalid)
        ));
        assert!(matches!(
            stop_conditions(
                &pool,
                AnalysisScope {
                    workspace: Uuid::new_v4(),
                    situation: Uuid::nil(),
                    analysis: Uuid::new_v4(),
                }
            )
            .await,
            Err(ControlError::Invalid)
        ));
    }
}
