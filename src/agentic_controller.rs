//! v0.211-v0.225: durable agentic plan DAG with explicit human gates and step budget.
use crate::bff_operations::{self, OperationScope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid plan")]
    Invalid,
    #[error("plan not visible")]
    Invisible,
    #[error("step is not ready")]
    NotReady,
    #[error("step budget exhausted")]
    BudgetExhausted,
    #[error("state conflict")]
    StateConflict,
    #[error("operation unavailable: {0}")]
    Operation(#[from] bff_operations::BffOperationError),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepInput {
    pub step_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub label: String,
    pub requires_human: bool,
    pub depends_on: Vec<Uuid>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepView {
    pub step_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub label: String,
    pub requires_human: bool,
    pub depends_on: Vec<Uuid>,
    pub state: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControllerView {
    pub plan_revision: i64,
    pub plan_sha256: String,
    pub steps: Vec<StepView>,
    pub ready_step_ids: Vec<Uuid>,
    pub budget_used: i64,
    pub budget_total: i32,
    pub waiting_for_human: bool,
    pub complete: bool,
    pub blocked: bool,
}

fn valid_kind(k: &str) -> bool {
    matches!(
        k,
        "INSPECT_SOURCES"
            | "ASSESS_CONTEXT"
            | "PROPOSE_CLARIFICATION"
            | "SYNTHESIZE"
            | "CHALLENGE_SYNTHESIS"
            | "CHECK_ASSUMPTIONS"
            | "CHECK_ALTERNATIVES"
            | "CHECK_STOP_CONDITIONS"
            | "REQUEST_HUMAN_DECISION"
            | "PREPARE_HANDOFF"
    )
}
async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ControllerError> {
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
        return Err(ControllerError::Invalid);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
fn canonical_steps(steps: &[StepInput]) -> Result<String, ControllerError> {
    let mut x = steps.to_vec();
    x.sort_by_key(|s| s.ordinal);
    Ok(serde_json::to_string(&x)?)
}
fn validate(steps: &[StepInput]) -> Result<(), ControllerError> {
    if steps.is_empty() || steps.len() > 512 {
        return Err(ControllerError::Invalid);
    }
    let ids: BTreeSet<_> = steps.iter().map(|s| s.step_id).collect();
    let ords: BTreeSet<_> = steps.iter().map(|s| s.ordinal).collect();
    if ids.len() != steps.len()
        || ords.len() != steps.len()
        || steps.iter().any(|s| {
            s.ordinal < 1
                || !valid_kind(&s.kind)
                || s.label.trim().is_empty()
                || s.label.len() > 1024
                || s.depends_on
                    .iter()
                    .any(|d| *d == s.step_id || !ids.contains(d))
        })
    {
        return Err(ControllerError::Invalid);
    }
    let mut indeg: BTreeMap<Uuid, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut edges: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
    for s in steps {
        for d in &s.depends_on {
            *indeg.get_mut(&s.step_id).unwrap() += 1;
            edges.entry(*d).or_default().push(s.step_id);
        }
    }
    let mut q: VecDeque<_> = indeg
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut seen = 0;
    while let Some(id) = q.pop_front() {
        seen += 1;
        if let Some(next) = edges.get(&id) {
            for n in next {
                let x = indeg.get_mut(n).unwrap();
                *x -= 1;
                if *x == 0 {
                    q.push_back(*n);
                }
            }
        }
    }
    if seen != steps.len() {
        return Err(ControllerError::Invalid);
    }
    Ok(())
}
pub async fn create_plan(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    steps: &[StepInput],
) -> Result<(i64, String), ControllerError> {
    validate(steps)?;
    let json = canonical_steps(steps)?;
    let sha = format!("{:x}", Sha256::digest(json.as_bytes()));
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let rev:i64=sqlx::query_scalar("SELECT coalesce(max(plan_revision),0)+1 FROM niccolo.agentic_plans WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.operation).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO niccolo.agentic_plans(workspace_id,situation_id,operation_id,plan_revision,plan_sha256,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(rev).bind(&sha).bind(actor).execute(&mut *tx).await?;
    let mut ordered = steps.to_vec();
    ordered.sort_by_key(|s| s.ordinal);
    for s in &ordered {
        sqlx::query("INSERT INTO niccolo.agentic_steps(workspace_id,situation_id,operation_id,plan_revision,step_id,ordinal,step_kind,label,requires_human) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(rev).bind(s.step_id).bind(s.ordinal).bind(&s.kind).bind(&s.label).bind(s.requires_human).execute(&mut *tx).await?;
    }
    for s in &ordered {
        for d in &s.depends_on {
            sqlx::query("INSERT INTO niccolo.agentic_step_dependencies(workspace_id,situation_id,operation_id,plan_revision,step_id,depends_on_step_id) VALUES($1,$2,$3,$4,$5,$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(rev).bind(s.step_id).bind(d).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok((rev, sha))
}
pub async fn view(pool: &PgPool, scope: OperationScope) -> Result<ControllerView, ControllerError> {
    let op = bff_operations::get(pool, scope).await?;
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let plan:Option<(i64,String)>=sqlx::query_as("SELECT plan_revision,plan_sha256 FROM niccolo.agentic_plans WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 ORDER BY plan_revision DESC LIMIT 1").bind(scope.workspace).bind(scope.situation).bind(scope.operation).fetch_optional(&mut *tx).await?;
    let Some((rev, sha)) = plan else {
        return Err(ControllerError::Invisible);
    };
    type Row = (Uuid, i32, String, String, bool, Option<String>);
    let rows:Vec<Row>=sqlx::query_as("SELECT s.step_id,s.ordinal,s.step_kind,s.label,s.requires_human,NULLIF((SELECT e.event_kind FROM niccolo.agentic_step_events e WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id AND e.operation_id=s.operation_id AND e.plan_revision=s.plan_revision AND e.step_id=s.step_id ORDER BY e.event_seq DESC LIMIT 1),'RETRY_READY') FROM niccolo.agentic_steps s WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.operation_id=$3 AND s.plan_revision=$4 ORDER BY s.ordinal").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(rev).fetch_all(&mut *tx).await?;
    let deps:Vec<(Uuid,Uuid)>=sqlx::query_as("SELECT step_id,depends_on_step_id FROM niccolo.agentic_step_dependencies WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(rev).fetch_all(&mut *tx).await?;
    let budget_used:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.agentic_step_events WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND event_kind='STARTED'").bind(scope.workspace).bind(scope.situation).bind(scope.operation).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    let mut depmap: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
    for (d, p) in deps {
        depmap.entry(d).or_default().push(p)
    }
    let state: BTreeMap<Uuid, String> = rows
        .iter()
        .map(|(id, _, _, _, _, st)| (*id, st.clone().unwrap_or_else(|| "PENDING".into())))
        .collect();
    let steps: Vec<StepView> = rows
        .into_iter()
        .map(|(id, ordinal, kind, label, requires_human, st)| StepView {
            step_id: id,
            ordinal,
            kind,
            label,
            requires_human,
            depends_on: depmap.remove(&id).unwrap_or_default(),
            state: st.unwrap_or_else(|| "PENDING".into()),
        })
        .collect();
    let ready_step_ids = steps
        .iter()
        .filter(|s| {
            s.state == "PENDING"
                && s.depends_on.iter().all(|d| {
                    matches!(
                        state.get(d).map(String::as_str),
                        Some("SUCCEEDED" | "SKIPPED")
                    )
                })
        })
        .map(|s| s.step_id)
        .collect::<Vec<_>>();
    let waiting_for_human =
        steps.iter().any(|s| s.state == "WAITING_FOR_HUMAN") || op.state == "WAITING_FOR_HUMAN";
    let complete = steps
        .iter()
        .all(|s| matches!(s.state.as_str(), "SUCCEEDED" | "SKIPPED"));
    let blocked = !complete && !waiting_for_human && ready_step_ids.is_empty();
    Ok(ControllerView {
        plan_revision: rev,
        plan_sha256: sha,
        steps,
        ready_step_ids,
        budget_used,
        budget_total: op.step_budget,
        waiting_for_human,
        complete,
        blocked,
    })
}
pub async fn start_step(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
) -> Result<ControllerView, ControllerError> {
    let before = view(pool, scope).await?;
    if before.budget_used >= i64::from(before.budget_total) {
        return Err(ControllerError::BudgetExhausted);
    }
    if !before.ready_step_ids.contains(&step) {
        return Err(ControllerError::NotReady);
    }
    let op = bff_operations::get(pool, scope).await?;
    if op.state == "QUEUED" {
        let _ = bff_operations::start(pool, scope, op.state_version, actor).await?;
    } else if op.state != "RUNNING" {
        return Err(ControllerError::StateConflict);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'STARTED','controller started step',$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(actor).execute(&mut *tx).await?;
    tx.commit().await?;
    view(pool, scope).await
}
pub async fn finish_step(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
    outcome: &str,
    note: &str,
) -> Result<ControllerView, ControllerError> {
    if !matches!(
        outcome,
        "SUCCEEDED" | "FAILED" | "WAITING_FOR_HUMAN" | "SKIPPED" | "CANCELLED"
    ) || note.trim().is_empty()
    {
        return Err(ControllerError::Invalid);
    }
    let before = view(pool, scope).await?;
    let s = before
        .steps
        .iter()
        .find(|x| x.step_id == step)
        .ok_or(ControllerError::Invisible)?;
    if s.state != "STARTED" {
        return Err(ControllerError::StateConflict);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(outcome).bind(note).bind(actor).execute(&mut *tx).await?;
    tx.commit().await?;
    if outcome == "WAITING_FOR_HUMAN" {
        let op = bff_operations::get(pool, scope).await?;
        let _ = bff_operations::wait_for_human(pool, scope, op.state_version, actor, note).await?;
    }
    view(pool, scope).await
}
pub async fn resume_step(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
    note: &str,
) -> Result<ControllerView, ControllerError> {
    let before = view(pool, scope).await?;
    let s = before
        .steps
        .iter()
        .find(|x| x.step_id == step)
        .ok_or(ControllerError::Invisible)?;
    if s.state != "WAITING_FOR_HUMAN" || note.trim().is_empty() {
        return Err(ControllerError::StateConflict);
    }
    let op = bff_operations::get(pool, scope).await?;
    let _ = bff_operations::resume(pool, scope, op.state_version, actor).await?;
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'RESUMED',$6,$7)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(note).bind(actor).execute(&mut *tx).await?;
    tx.commit().await?;
    // RESUMED means active again, so add a STARTED event as the latest operational state.
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'STARTED','resumed after human input',$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(actor).execute(&mut *tx).await?;
    tx.commit().await?;
    view(pool, scope).await
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanSteeringEntry {
    pub plan_revision: i64,
    pub step_id: Uuid,
    pub decision: String,
    pub response_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanSteeringView {
    pub scope: String,
    pub responses: Vec<HumanSteeringEntry>,
    pub truncated: bool,
    pub responses_are_evidence: bool,
    pub responses_are_facts: bool,
    pub frozen_input_unchanged: bool,
}

pub async fn human_review_occurred(
    pool: &PgPool,
    scope: OperationScope,
) -> Result<bool, ControllerError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let occurred: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM niccolo.agentic_human_responses
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
         )",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(occurred)
}

pub async fn human_steering(
    pool: &PgPool,
    scope: OperationScope,
    limit: i64,
) -> Result<HumanSteeringView, ControllerError> {
    if !(1..=64).contains(&limit) {
        return Err(ControllerError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<(i64, Uuid, String, String)> = sqlx::query_as(
        "SELECT plan_revision,step_id,decision,response_text \
         FROM niccolo.agentic_human_responses \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 \
         ORDER BY plan_revision DESC,created_at DESC,response_id DESC LIMIT $4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let truncated = rows.len() as i64 > limit;
    // Keep the newest bounded steering while presenting it chronologically.
    // Otherwise a long-running operation could silently drop the human's latest instruction.
    let mut responses = rows
        .into_iter()
        .take(limit as usize)
        .map(|(plan_revision, step_id, decision, response_text)| HumanSteeringEntry {
            plan_revision,
            step_id,
            decision,
            response_text,
        })
        .collect::<Vec<_>>();
    responses.reverse();
    Ok(HumanSteeringView {
        scope: "THIS_OPERATION_ONLY".into(),
        responses,
        truncated,
        responses_are_evidence: false,
        responses_are_facts: false,
        frozen_input_unchanged: true,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanBoundaryResolution {
    pub decision: String,
    pub operation_state: String,
    pub plan_revision: i64,
    pub step_id: Uuid,
    pub human_response_persisted: bool,
    pub system_inferred_consent: bool,
    pub executor_resume_required: bool,
}
fn human_boundary_input_valid(decision: &str, response_text: &str) -> bool {
    matches!(decision, "CONTINUE" | "STOP" | "REPLAN")
        && (decision == "STOP" || !response_text.trim().is_empty())
        && response_text.len() <= 4096
}

async fn human_boundary_serialization_guard(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
) -> Result<Transaction<'_, Postgres>, ControllerError> {
    let mut tx = pool.begin().await?;
    let key = format!(
        "niccolo:agentic-human-boundary:{}:{}:{}",
        scope.workspace, scope.operation, step
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn replan_steps(
    pool: &PgPool,
    scope: OperationScope,
) -> Result<Vec<StepInput>, ControllerError> {
    let op = bff_operations::get(pool, scope).await?;
    let analysis = op.target_analysis_id.ok_or(ControllerError::Invalid)?;
    let input = bff_operations::input(pool, scope).await?;
    let intent = input
        .get("human_intent")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok());
    crate::agentic_policy::plan_for_intent(
        pool,
        scope.workspace,
        scope.situation,
        analysis,
        intent,
    )
    .await
    .map_err(|_| ControllerError::Invalid)
}

// This flag means "ensure autonomous execution is running", not "this HTTP
// response was fresh". Exact retries may return true while work remains;
// the BFF execution lease prevents duplicate actual execution.
fn executor_should_resume(
    operation: &bff_operations::OperationView,
    controller: &ControllerView,
) -> bool {
    operation.state == "RUNNING" && !operation.terminal && !controller.waiting_for_human
}

pub async fn resolve_human_boundary(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    response_id: Uuid,
    actor: Uuid,
    decision: &str,
    response_text: &str,
) -> Result<HumanBoundaryResolution, ControllerError> {
    if !human_boundary_input_valid(decision, response_text) {
        return Err(ControllerError::Invalid);
    }
    let _serialization_guard = human_boundary_serialization_guard(pool, scope, step).await?;
    let response_text = response_text.trim();
    let before = view(pool, scope).await?;
    let step_state = before
        .steps
        .iter()
        .find(|candidate| candidate.step_id == step)
        .map(|candidate| candidate.state.as_str());

    let mut preplanned_replan: Option<Vec<StepInput>> = None;

    // Human responses are client-generated idempotent commands. If an HTTP
    // response is lost after persistence, an exact retry must not become a
    // conflict just because the step has already advanced. Conversely, an
    // already-persisted response with different content remains a conflict.
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let existing: Option<(i64, Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT plan_revision,response_id,decision,response_text,actor_principal_id \
         FROM niccolo.agentic_human_responses \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND step_id=$4 \
         ORDER BY plan_revision DESC LIMIT 1",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(step)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    if let Some((stored_revision, stored_id, stored_decision, stored_text, stored_actor)) = existing
    {
        if stored_id != response_id
            || stored_decision != decision
            || stored_text != response_text
            || stored_actor != actor
        {
            return Err(ControllerError::StateConflict);
        }

        // Exact retry is also crash recovery. If REPLAN was recorded and the old
        // boundary step reached SUCCEEDED but the next plan was never persisted,
        // reconstruct that missing plan from the original operation intent.
        let operation = bff_operations::get(pool, scope).await?;
        if decision == "REPLAN"
            && stored_revision == before.plan_revision
            && step_state == Some("SUCCEEDED")
            && !operation.terminal
        {
            let steps = replan_steps(pool, scope).await?;
            let _ = create_plan(pool, scope, actor, &steps).await?;
            let refreshed = view(pool, scope).await?;
            return Ok(HumanBoundaryResolution {
                decision: decision.into(),
                operation_state: operation.state.clone(),
                plan_revision: refreshed.plan_revision,
                step_id: step,
                human_response_persisted: true,
                system_inferred_consent: false,
                executor_resume_required: executor_should_resume(&operation, &refreshed),
            });
        }
        if stored_revision != before.plan_revision
            || step_state != Some("WAITING_FOR_HUMAN")
            || operation.terminal
        {
            return Ok(HumanBoundaryResolution {
                decision: decision.into(),
                operation_state: operation.state.clone(),
                plan_revision: before.plan_revision,
                step_id: step,
                human_response_persisted: true,
                system_inferred_consent: false,
                executor_resume_required: executor_should_resume(&operation, &before),
            });
        }
    } else {
        if step_state.is_none() {
            return Err(ControllerError::Invisible);
        }
        if step_state != Some("WAITING_FOR_HUMAN") {
            return Err(ControllerError::StateConflict);
        }
        if decision == "REPLAN" && before.budget_used >= i64::from(before.budget_total) {
            return Err(ControllerError::BudgetExhausted);
        }
        if decision == "REPLAN" {
            // Validate policy construction before consuming the human command.
            preplanned_replan = Some(replan_steps(pool, scope).await?);
        }

        let mut tx = scoped(pool, scope.workspace, false).await?;
        let inserted = sqlx::query("INSERT INTO niccolo.agentic_human_responses(workspace_id,situation_id,operation_id,plan_revision,step_id,response_id,decision,response_text,actor_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
            .bind(scope.workspace)
            .bind(scope.situation)
            .bind(scope.operation)
            .bind(before.plan_revision)
            .bind(step)
            .bind(response_id)
            .bind(decision)
            .bind(response_text)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        let row: Option<(Uuid, String, String, Uuid)> = sqlx::query_as("SELECT response_id,decision,response_text,actor_principal_id FROM niccolo.agentic_human_responses WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5")
            .bind(scope.workspace)
            .bind(scope.situation)
            .bind(scope.operation)
            .bind(before.plan_revision)
            .bind(step)
            .fetch_optional(&mut *tx)
            .await?;
        if row != Some((response_id, decision.into(), response_text.into(), actor)) {
            return Err(ControllerError::StateConflict);
        }
        tx.commit().await?;

        // If an identical concurrent request won the insert race, refresh the
        // plan. If it already advanced, this request is a side-effect-free
        // replay. If it has not advanced yet, operation state-version checks
        // still gate the actual resume.
        if inserted.rows_affected() == 0 {
            let refreshed = view(pool, scope).await?;
            let refreshed_state = refreshed
                .steps
                .iter()
                .find(|candidate| candidate.step_id == step)
                .map(|candidate| candidate.state.as_str());
            if refreshed.plan_revision != before.plan_revision
                || refreshed_state != Some("WAITING_FOR_HUMAN")
            {
                let operation = bff_operations::get(pool, scope).await?;
                return Ok(HumanBoundaryResolution {
                    decision: decision.into(),
                    operation_state: operation.state.clone(),
                    plan_revision: refreshed.plan_revision,
                    step_id: step,
                    human_response_persisted: true,
                    system_inferred_consent: false,
                    executor_resume_required: executor_should_resume(&operation, &refreshed),
                });
            }
        }
    }

    let op = bff_operations::get(pool, scope).await?;
    if decision == "STOP" {
        let v = bff_operations::cancel(
            pool,
            scope,
            op.state_version,
            actor,
            "human stopped agentic continuation",
        )
        .await?;
        return Ok(HumanBoundaryResolution {
            decision: decision.into(),
            operation_state: v.state,
            plan_revision: before.plan_revision,
            step_id: step,
            human_response_persisted: true,
            system_inferred_consent: false,
            executor_resume_required: false,
        });
    }
    if op.state == "WAITING_FOR_HUMAN" {
        let _ = bff_operations::resume(pool, scope, op.state_version, actor).await?;
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'RESUMED',$6,$7),($1,$2,$3,$4,$5,'SUCCEEDED',$8,$7)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(response_text.trim()).bind(actor).bind(if decision=="REPLAN"{"human requested replanning"}else{"human explicitly continued"}).execute(&mut *tx).await?;
    tx.commit().await?;
    if decision == "REPLAN" {
        let steps = match preplanned_replan {
            Some(steps) => steps,
            None => replan_steps(pool, scope).await?,
        };
        let _ = create_plan(pool, scope, actor, &steps).await?;
    }
    let op = bff_operations::get(pool, scope).await?;
    let controller = view(pool, scope).await?;
    Ok(HumanBoundaryResolution {
        decision: decision.into(),
        operation_state: op.state.clone(),
        plan_revision: controller.plan_revision,
        step_id: step,
        human_response_persisted: true,
        system_inferred_consent: false,
        executor_resume_required: executor_should_resume(&op, &controller),
    })
}

/// v1.401-v1.500: recover only genuinely stale STARTED steps. Recovery is an
/// append-only state transition; completed/waiting steps are never reset.
pub async fn recover_stale_started(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    stale_after_seconds: i32,
) -> Result<ControllerView, ControllerError> {
    if !(30..=86400).contains(&stale_after_seconds) {
        return Err(ControllerError::Invalid);
    }
    let before = view(pool, scope).await?;
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let stale:Vec<Uuid>=sqlx::query_scalar("SELECT s.step_id FROM niccolo.agentic_steps s WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.operation_id=$3 AND s.plan_revision=$4 AND (SELECT e.event_kind FROM niccolo.agentic_step_events e WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id AND e.operation_id=s.operation_id AND e.plan_revision=s.plan_revision AND e.step_id=s.step_id ORDER BY e.event_seq DESC LIMIT 1)='STARTED' AND (SELECT e.created_at FROM niccolo.agentic_step_events e WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id AND e.operation_id=s.operation_id AND e.plan_revision=s.plan_revision AND e.step_id=s.step_id ORDER BY e.event_seq DESC LIMIT 1)<clock_timestamp()-make_interval(secs=>$5)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(stale_after_seconds).fetch_all(&mut *tx).await?;
    for step in stale {
        sqlx::query("UPDATE niccolo.model_invocations SET status='FAILED',error_code='RECOVERY_STALE',error_message='orphaned invocation recovered after stale threshold',completed_at=clock_timestamp() WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND plan_revision=$4 AND step_id=$5 AND status='STARTED'").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'RETRY_READY','stale STARTED step recovered for bounded retry',$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(actor).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    view(pool, scope).await
}

#[cfg(test)]
mod human_boundary_input_tests {
    use super::human_boundary_input_valid;

    #[test]
    fn stopping_needs_no_explanation_but_continuing_does() {
        assert!(human_boundary_input_valid("STOP", ""));
        assert!(human_boundary_input_valid("STOP", "  "));
        assert!(!human_boundary_input_valid("CONTINUE", ""));
        assert!(!human_boundary_input_valid("REPLAN", "  "));
        assert!(human_boundary_input_valid("CONTINUE", "Please include another source"));
        assert!(!human_boundary_input_valid("UNKNOWN", "yes"));
        assert!(!human_boundary_input_valid("STOP", &"x".repeat(4097)));
    }
}
