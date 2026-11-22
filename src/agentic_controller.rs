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
    #[error("selected plan revision is no longer current")]
    PlanChanged,
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
    for step in &mut x {
        step.depends_on.sort_unstable();
    }
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
                || s.label.contains('\0')
                || s.depends_on.iter().collect::<BTreeSet<_>>().len() != s.depends_on.len()
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
async fn read_operation_state(
    tx: &mut Transaction<'_, Postgres>,
    scope: OperationScope,
    lock_for_update: bool,
) -> Result<(String, i32, i64), ControllerError> {
    sqlx::query_as(if lock_for_update {
        "SELECT state,step_budget,state_version FROM niccolo.bff_operations
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 FOR UPDATE"
    } else {
        "SELECT state,step_budget,state_version FROM niccolo.bff_operations
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3"
    })
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(ControllerError::Invisible)
}

/// Publish a result only for the current, completed plan.
pub async fn complete_plan(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    expected_revision: i64,
    result: &serde_json::Value,
) -> Result<bool, ControllerError> {
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (state, budget, version) = read_operation_state(&mut tx, scope, true).await?;
    let plan = view_plan(&mut tx, scope, &state, budget).await?;
    if state != "RUNNING" || plan.plan_revision != expected_revision || !plan.complete {
        return Ok(false);
    }
    bff_operations::succeed_in(&mut tx, scope, version, actor, result).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn create_plan(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    steps: &[StepInput],
) -> Result<(i64, String), ControllerError> {
    create_plan_checked(pool, scope, actor, steps, None)
        .await?
        .ok_or(ControllerError::StateConflict)
}

/// Append a follow-up only to the current completed plan.
pub async fn append_followup_plan(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    steps: &[StepInput],
    completed_revision: i64,
) -> Result<bool, ControllerError> {
    Ok(create_plan_checked(pool, scope, actor, steps, Some(completed_revision))
        .await?
        .is_some())
}

async fn create_plan_checked(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    steps: &[StepInput],
    completed_revision: Option<i64>,
) -> Result<Option<(i64, String)>, ControllerError> {
    validate(steps)?;
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (state, budget, _) = read_operation_state(&mut tx, scope, true).await?;
    if completed_revision.is_some() && state != "RUNNING" {
        return Ok(None);
    }
    if !matches!(state.as_str(), "QUEUED" | "RUNNING") {
        return Err(ControllerError::StateConflict);
    }
    let rev = match view_plan(&mut tx, scope, &state, budget).await {
        Ok(view) => {
            if completed_revision.is_some_and(|revision| {
                view.plan_revision != revision || !view.complete
            }) {
                return Ok(None);
            }
            if view.steps.iter().any(|step| step.state == "STARTED") {
                return Err(ControllerError::StateConflict);
            }
            view.plan_revision.checked_add(1).ok_or(ControllerError::Invalid)?
        }
        Err(ControllerError::Invisible) if completed_revision.is_none() => 1,
        Err(error) => return Err(error),
    };
    let sha = insert_plan(&mut tx, scope, actor, steps, rev).await?;
    tx.commit().await?;
    Ok(Some((rev, sha)))
}

pub(crate) async fn ensure_plan(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    steps: &[StepInput],
) -> Result<(ControllerView, bool), ControllerError> {
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (state, budget, _) = read_operation_state(&mut tx, scope, true).await?;
    let (view, created) = match view_plan(&mut tx, scope, &state, budget).await {
        Ok(view) => (view, false),
        Err(ControllerError::Invisible) => {
            if !matches!(state.as_str(), "QUEUED" | "RUNNING") {
                return Err(ControllerError::StateConflict);
            }
            validate(steps)?;
            insert_plan(&mut tx, scope, actor, steps, 1).await?;
            (view_plan(&mut tx, scope, &state, budget).await?, true)
        }
        Err(error) => return Err(error),
    };
    tx.commit().await?;
    Ok((view, created))
}

async fn insert_plan(
    tx: &mut Transaction<'_, Postgres>,
    scope: OperationScope,
    actor: Uuid,
    steps: &[StepInput],
    rev: i64,
) -> Result<String, ControllerError> {
    let json = canonical_steps(steps)?;
    let sha = format!("{:x}", Sha256::digest(json.as_bytes()));
    sqlx::query("INSERT INTO niccolo.agentic_plans(workspace_id,situation_id,operation_id,plan_revision,plan_sha256,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(rev).bind(&sha).bind(actor).execute(&mut **tx).await?;
    sqlx::query(
        "INSERT INTO niccolo.agentic_steps
             (workspace_id,situation_id,operation_id,plan_revision,step_id,
              ordinal,step_kind,label,requires_human)
         SELECT $1,$2,$3,$4,step_id,ordinal,kind,label,requires_human
         FROM jsonb_to_recordset($5::text::jsonb) AS step(
             step_id uuid, ordinal integer, kind text, label text, requires_human boolean
         )
         ORDER BY ordinal",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(rev)
    .bind(&json)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO niccolo.agentic_step_dependencies
             (workspace_id,situation_id,operation_id,plan_revision,step_id,depends_on_step_id)
         SELECT $1,$2,$3,$4,step.step_id,dependency.value::uuid
         FROM jsonb_to_recordset($5::text::jsonb) AS step(step_id uuid, depends_on jsonb)
         CROSS JOIN LATERAL jsonb_array_elements_text(step.depends_on) AS dependency(value)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(rev)
    .bind(&json)
    .execute(&mut **tx)
    .await?;
    Ok(sha)
}

pub async fn view(pool: &PgPool, scope: OperationScope) -> Result<ControllerView, ControllerError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let (operation_state, step_budget, _) = read_operation_state(&mut tx, scope, false).await?;
    let result = view_plan(&mut tx, scope, &operation_state, step_budget).await?;
    tx.commit().await?;
    Ok(result)
}

async fn view_plan(
    tx: &mut Transaction<'_, Postgres>,
    scope: OperationScope,
    operation_state: &str,
    step_budget: i32,
) -> Result<ControllerView, ControllerError> {
    let plan: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT latest.plan_revision, latest.plan_sha256, (
             SELECT count(*) FROM niccolo.agentic_step_events e
             WHERE e.workspace_id=$1 AND e.situation_id=$2 AND e.operation_id=$3
               AND e.plan_revision=latest.plan_revision AND e.event_kind='STARTED'
         )
         FROM (
             SELECT plan_revision,plan_sha256 FROM niccolo.agentic_plans
             WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
             ORDER BY plan_revision DESC LIMIT 1
         ) latest",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((rev, sha, budget_used)) = plan else {
        return Err(ControllerError::Invisible);
    };
    type Row = (Uuid, i32, String, String, bool, Option<String>, Vec<Uuid>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.step_id,s.ordinal,s.step_kind,s.label,s.requires_human,
                NULLIF(latest.event_kind,'RETRY_READY'),
                ARRAY(
                    SELECT d.depends_on_step_id
                    FROM niccolo.agentic_step_dependencies d
                    WHERE d.workspace_id=s.workspace_id AND d.situation_id=s.situation_id
                      AND d.operation_id=s.operation_id AND d.plan_revision=s.plan_revision
                      AND d.step_id=s.step_id
                    ORDER BY d.depends_on_step_id
                )
         FROM niccolo.agentic_steps s
         LEFT JOIN LATERAL (
             SELECT e.event_kind
             FROM niccolo.agentic_step_events e
             WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id
               AND e.operation_id=s.operation_id AND e.plan_revision=s.plan_revision
               AND e.step_id=s.step_id
             ORDER BY e.event_seq DESC LIMIT 1
         ) latest ON true
         WHERE s.workspace_id=$1 AND s.situation_id=$2
           AND s.operation_id=$3 AND s.plan_revision=$4
         ORDER BY s.ordinal",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(rev)
    .fetch_all(&mut **tx)
    .await?;
    let steps: Vec<StepView> = rows
        .into_iter()
        .map(|(step_id, ordinal, kind, label, requires_human, state, depends_on)| StepView {
            step_id,
            ordinal,
            kind,
            label,
            requires_human,
            depends_on,
            state: state.unwrap_or_else(|| "PENDING".into()),
        })
        .collect();
    let state: BTreeMap<Uuid, &str> = steps
        .iter()
        .map(|step| (step.step_id, step.state.as_str()))
        .collect();
    let ready_step_ids = steps
        .iter()
        .filter(|s| {
            s.state == "PENDING"
                && s.depends_on.iter().all(|d| {
                    matches!(
                        state.get(d).copied(),
                        Some("SUCCEEDED" | "SKIPPED")
                    )
                })
        })
        .map(|s| s.step_id)
        .collect::<Vec<_>>();
    let waiting_for_human =
        steps.iter().any(|s| s.state == "WAITING_FOR_HUMAN") || operation_state == "WAITING_FOR_HUMAN";
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
        budget_total: step_budget,
        waiting_for_human,
        complete,
        blocked,
    })
}
fn check_step_start(
    view: &ControllerView,
    step: Uuid,
    expected_revision: Option<i64>,
) -> Result<(), ControllerError> {
    if expected_revision.is_some_and(|revision| revision != view.plan_revision) {
        return Err(ControllerError::PlanChanged);
    }
    if view.budget_used >= i64::from(view.budget_total) {
        return Err(ControllerError::BudgetExhausted);
    }
    if !view.ready_step_ids.contains(&step) {
        return Err(ControllerError::NotReady);
    }
    Ok(())
}

pub async fn start_step(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
) -> Result<ControllerView, ControllerError> {
    start_step_checked(pool, scope, step, actor, None).await
}

/// Admit a step only if the plan revision from which it was selected is current.
pub async fn start_step_at_revision(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
    expected_revision: i64,
) -> Result<ControllerView, ControllerError> {
    start_step_checked(pool, scope, step, actor, Some(expected_revision)).await
}

async fn start_step_checked(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
    expected_revision: Option<i64>,
) -> Result<ControllerView, ControllerError> {
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (operation_state, step_budget, version) = read_operation_state(&mut tx, scope, true).await?;
    let before = view_plan(&mut tx, scope, &operation_state, step_budget).await?;
    check_step_start(&before, step, expected_revision)?;
    if operation_state == "QUEUED" {
        bff_operations::start_in(&mut tx, scope, version, actor).await?;
    } else if operation_state != "RUNNING" {
        return Err(ControllerError::StateConflict);
    }
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'STARTED','controller started step',$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(actor).execute(&mut *tx).await?;
    let started = view_plan(&mut tx, scope, "RUNNING", step_budget).await?;
    tx.commit().await?;
    Ok(started)
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
        || note.contains('\0')
        || note.chars().count() > 4096
    {
        return Err(ControllerError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (operation_state, step_budget, version) = read_operation_state(&mut tx, scope, true).await?;
    let before = view_plan(&mut tx, scope, &operation_state, step_budget).await?;
    let s = before
        .steps
        .iter()
        .find(|x| x.step_id == step)
        .ok_or(ControllerError::Invisible)?;
    if s.state != "STARTED" {
        return Err(ControllerError::StateConflict);
    }
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(outcome).bind(note).bind(actor).execute(&mut *tx).await?;
    if outcome == "WAITING_FOR_HUMAN" {
        bff_operations::wait_for_human_in(&mut tx, scope, version, actor, note).await?;
    }
    let state = if outcome == "WAITING_FOR_HUMAN" {
        "WAITING_FOR_HUMAN"
    } else {
        &operation_state
    };
    let finished = view_plan(&mut tx, scope, state, step_budget).await?;
    tx.commit().await?;
    Ok(finished)
}
pub async fn resume_step(
    pool: &PgPool,
    scope: OperationScope,
    step: Uuid,
    actor: Uuid,
    note: &str,
) -> Result<ControllerView, ControllerError> {
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (state, budget, version) = read_operation_state(&mut tx, scope, true).await?;
    let before = view_plan(&mut tx, scope, &state, budget).await?;
    let s = before
        .steps
        .iter()
        .find(|x| x.step_id == step)
        .ok_or(ControllerError::Invisible)?;
    if s.state != "WAITING_FOR_HUMAN" || note.trim().is_empty() {
        return Err(ControllerError::StateConflict);
    }
    bff_operations::resume_in(&mut tx, scope, version, actor).await?;
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'RESUMED',$6,$7)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(note).bind(actor).execute(&mut *tx).await?;
    // RESUMED means active again, so add a STARTED event as the latest operational state.
    sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'STARTED','resumed after human input',$6)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(actor).execute(&mut *tx).await?;
    let resumed = view_plan(&mut tx, scope, "RUNNING", budget).await?;
    tx.commit().await?;
    Ok(resumed)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanBoundaryResolution {
    pub decision: String,
    pub operation_state: String,
    pub plan_revision: i64,
    pub step_id: Uuid,
    pub human_response_persisted: bool,
    pub system_inferred_consent: bool,
}
fn human_boundary_input_valid(decision: &str, response_text: &str) -> bool {
    matches!(decision, "CONTINUE" | "STOP" | "REPLAN")
        && (decision == "STOP" || !response_text.trim().is_empty())
        && response_text.len() <= 4096
        && !response_text.contains('\0')
}

async fn recorded_human_resolution(
    tx: &mut Transaction<'_, Postgres>,
    scope: OperationScope,
    step: Uuid,
    response_id: Uuid,
    actor: Uuid,
    decision: &str,
    response_text: &str,
) -> Result<Option<HumanBoundaryResolution>, ControllerError> {
    type Receipt = (Uuid, Uuid, String, String, String, i64);
    let row: Option<Receipt> = sqlx::query_as(
        "SELECT step_id,actor_principal_id,decision,response_text,
                result_operation_state,result_plan_revision
         FROM niccolo.agentic_human_responses
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
           AND response_id=$4 AND result_plan_revision IS NOT NULL",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(response_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((saved_step, saved_actor, saved_decision, saved_text, state, revision)) = row else {
        return Ok(None);
    };
    if saved_step != step || saved_actor != actor || saved_decision != decision
        || saved_text != response_text.trim()
    {
        return Err(ControllerError::StateConflict);
    }
    Ok(Some(HumanBoundaryResolution {
        decision: saved_decision,
        operation_state: state,
        plan_revision: revision,
        step_id: step,
        human_response_persisted: true,
        system_inferred_consent: false,
    }))
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
    let replacement = if decision == "REPLAN" {
        // A replay does not need to regenerate the proposed plan.
        let mut tx = scoped(pool, scope.workspace, true).await?;
        let recorded = recorded_human_resolution(
            &mut tx, scope, step, response_id, actor, decision, response_text,
        )
        .await?;
        tx.commit().await?;
        if let Some(recorded) = recorded {
            return Ok(recorded);
        }
        let op = bff_operations::get(pool, scope).await?;
        let analysis = op.target_analysis_id.ok_or(ControllerError::Invalid)?;
        let input = bff_operations::input(pool, scope).await?;
        let intent = crate::agentic_policy::intent_from_operation_input(&input);
        let steps = crate::agentic_policy::plan_for_intent(
            pool, scope.workspace, scope.situation, analysis, intent,
        )
        .await
        .map_err(|_| ControllerError::Invalid)?;
        validate(&steps)?;
        Some(steps)
    } else {
        None
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (state, budget, version) = read_operation_state(&mut tx, scope, true).await?;
    if let Some(recorded) = recorded_human_resolution(
        &mut tx, scope, step, response_id, actor, decision, response_text,
    )
    .await?
    {
        tx.commit().await?;
        return Ok(recorded);
    }
    let before = view_plan(&mut tx, scope, &state, budget).await?;
    let step_view = before
        .steps
        .iter()
        .find(|candidate| candidate.step_id == step)
        .ok_or(ControllerError::Invisible)?;
    if step_view.state != "WAITING_FOR_HUMAN"
        || !matches!(state.as_str(), "WAITING_FOR_HUMAN" | "RUNNING")
    {
        return Err(ControllerError::StateConflict);
    }
    if replacement.is_some() && before.steps.iter().any(|step| step.state == "STARTED") {
        return Err(ControllerError::StateConflict);
    }
    let plan_revision = if replacement.is_some() {
        before.plan_revision.checked_add(1).ok_or(ControllerError::Invalid)?
    } else {
        before.plan_revision
    };
    let operation_state = if decision == "STOP" { "CANCELLED" } else { "RUNNING" };
    let inserted = sqlx::query(
        "INSERT INTO niccolo.agentic_human_responses
             (workspace_id,situation_id,operation_id,plan_revision,step_id,response_id,
              decision,response_text,actor_principal_id,result_operation_state,result_plan_revision)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(before.plan_revision)
    .bind(step)
    .bind(response_id)
    .bind(decision)
    .bind(response_text.trim())
    .bind(actor)
    .bind(operation_state)
    .bind(plan_revision)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        let matches_legacy: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM niccolo.agentic_human_responses
                WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
                  AND plan_revision=$4 AND step_id=$5 AND result_plan_revision IS NULL
                  AND decision=$6 AND response_text=$7 AND actor_principal_id=$8
            )",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.operation)
        .bind(before.plan_revision)
        .bind(step)
        .bind(decision)
        .bind(response_text.trim())
        .bind(actor)
        .fetch_one(&mut *tx)
        .await?;
        if !matches_legacy {
            return Err(ControllerError::StateConflict);
        }
    }
    if decision == "STOP" {
        bff_operations::cancel_in(
            &mut tx, scope, version, actor, "human stopped agentic continuation",
        )
        .await?;
    } else {
        if state == "WAITING_FOR_HUMAN" {
            bff_operations::resume_in(&mut tx, scope, version, actor).await?;
        }
        sqlx::query("INSERT INTO niccolo.agentic_step_events(workspace_id,situation_id,operation_id,plan_revision,step_id,event_kind,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'RESUMED',$6,$7),($1,$2,$3,$4,$5,'SUCCEEDED',$8,$7)").bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(before.plan_revision).bind(step).bind(response_text.trim()).bind(actor).bind(if decision=="REPLAN"{"human requested replanning"}else{"human explicitly continued"}).execute(&mut *tx).await?;
        if let Some(steps) = replacement {
            insert_plan(&mut tx, scope, actor, &steps, plan_revision).await?;
        }
    }
    tx.commit().await?;
    Ok(HumanBoundaryResolution {
        decision: decision.into(),
        operation_state: operation_state.into(),
        plan_revision,
        step_id: step,
        human_response_persisted: true,
        system_inferred_consent: false,
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
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let (state, budget, _) = read_operation_state(&mut tx, scope, true).await?;
    let before = view_plan(&mut tx, scope, &state, budget).await?;
    if state != "RUNNING" || before.steps.iter().all(|step| step.state != "STARTED") {
        return Ok(before);
    }
    let stale: Vec<Uuid> = sqlx::query_scalar(
        "SELECT s.step_id
         FROM niccolo.agentic_steps s
         JOIN LATERAL (
             SELECT e.event_kind, e.created_at
             FROM niccolo.agentic_step_events e
             WHERE e.workspace_id=s.workspace_id AND e.situation_id=s.situation_id
               AND e.operation_id=s.operation_id AND e.plan_revision=s.plan_revision
               AND e.step_id=s.step_id
             ORDER BY e.event_seq DESC LIMIT 1
         ) latest ON true
         WHERE s.workspace_id=$1 AND s.situation_id=$2
           AND s.operation_id=$3 AND s.plan_revision=$4
           AND latest.event_kind='STARTED'
           AND latest.created_at < statement_timestamp()-make_interval(secs=>$5)
         ORDER BY s.ordinal",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(before.plan_revision)
    .bind(stale_after_seconds)
    .fetch_all(&mut *tx)
    .await?;
    if stale.is_empty() {
        return Ok(before);
    }
    sqlx::query(
        "UPDATE niccolo.model_invocations
         SET status='FAILED', error_code='RECOVERY_STALE',
             error_message='orphaned invocation recovered after stale threshold',
             completed_at=clock_timestamp()
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
           AND plan_revision=$4 AND step_id=ANY($5) AND status='STARTED'",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(before.plan_revision)
    .bind(&stale)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO niccolo.agentic_step_events
             (workspace_id,situation_id,operation_id,plan_revision,step_id,
              event_kind,note,recorded_by_principal_id)
         SELECT $1,$2,$3,$4,step_id,'RETRY_READY',
                'stale STARTED step recovered for bounded retry',$6
         FROM unnest($5::uuid[]) WITH ORDINALITY AS recovered(step_id,position)
         ORDER BY position",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(before.plan_revision)
    .bind(&stale)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    let recovered = view_plan(&mut tx, scope, &state, budget).await?;
    tx.commit().await?;
    Ok(recovered)
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
        for decision in ["CONTINUE", "STOP", "REPLAN"] {
            assert!(!human_boundary_input_valid(decision, "invalid\0response"));
        }
    }
}
