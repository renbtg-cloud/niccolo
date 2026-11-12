//! v0.182-v0.200: one durable BFF operation model with sync and async delivery surfaces.
//! Interaction mode never changes domain semantics or result/error representation.
use crate::analysis_structure::AnalysisScope;
use crate::{human_briefing, operation_budget, reasoning_handoff};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BffOperationError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid operation request")]
    Invalid,
    #[error("operation not visible")]
    Invisible,
    #[error("idempotency conflict")]
    IdempotencyConflict,
    #[error("state conflict")]
    StateConflict,
    #[error("unsupported operation kind")]
    UnsupportedKind,
    #[error("task failed: {0}")]
    Task(String),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationScope {
    pub workspace: Uuid,
    pub situation: Uuid,
    pub operation: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewOperation {
    pub kind: String,
    pub target_analysis_id: Option<Uuid>,
    pub parent_operation_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub input: Value,
    pub deadline_at: Option<String>,
    pub step_budget: i32,
    pub requested_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationView {
    pub operation_id: Uuid,
    pub kind: String,
    pub target_analysis_id: Option<Uuid>,
    pub parent_operation_id: Option<Uuid>,
    pub state: String,
    pub state_version: i64,
    pub result: Option<Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub step_budget: i32,
    pub terminal: bool,
    pub retry_after_ms: Option<u64>,
    pub etag: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEventView {
    pub seq: i64,
    pub kind: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPage {
    pub events: Vec<OperationEventView>,
    pub next_after: Option<i64>,
}

type OperationRow = (
    Uuid,
    String,
    Option<Uuid>,
    Option<Uuid>,
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    i32,
);

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, BffOperationError> {
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
        return Err(BffOperationError::Invalid);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn kind_valid(kind: &str) -> bool {
    matches!(
        kind,
        "BUILD_BRIEFING" | "BUILD_HANDOFF" | "AGENTIC_ANALYSIS" | "REANALYZE"
    )
}
fn terminal(state: &str) -> bool {
    matches!(state, "SUCCEEDED" | "FAILED" | "CANCELLED")
}
fn retry_after_ms(state: &str) -> Option<u64> {
    match state {
        "QUEUED" | "RUNNING" => Some(750),
        "WAITING_FOR_HUMAN" => Some(3000),
        _ => None,
    }
}
fn etag(id: Uuid, version: i64) -> String {
    format!("\"op-{id}-{version}\"")
}
fn canonical(value: &Value) -> Result<String, BffOperationError> {
    Ok(serde_json::to_string(value)?)
}
fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn row_to_view(row: OperationRow) -> Result<OperationView, BffOperationError> {
    let (
        operation_id,
        kind,
        target_analysis_id,
        parent_operation_id,
        state,
        state_version,
        result_json,
        error_code,
        error_message,
        step_budget,
    ) = row;
    Ok(OperationView {
        operation_id,
        kind,
        target_analysis_id,
        parent_operation_id,
        terminal: terminal(&state),
        retry_after_ms: retry_after_ms(&state),
        etag: etag(operation_id, state_version),
        state,
        state_version,
        result: result_json.map(|x| serde_json::from_str(&x)).transpose()?,
        error_code,
        error_message,
        step_budget,
    })
}

pub async fn input(pool: &PgPool, scope: OperationScope) -> Result<Value, BffOperationError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT input_json FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let raw = raw.ok_or(BffOperationError::Invisible)?;
    Ok(serde_json::from_str(&raw)?)
}

pub async fn submit(
    pool: &PgPool,
    scope: OperationScope,
    input: &NewOperation,
) -> Result<OperationView, BffOperationError> {
    if !kind_valid(&input.kind)
        || !(1..=10_000).contains(&input.step_budget)
        || input
            .idempotency_key
            .as_ref()
            .is_some_and(|x| x.trim().is_empty() || x.len() > 256)
        || matches!(
            input.kind.as_str(),
            "BUILD_BRIEFING" | "BUILD_HANDOFF" | "REANALYZE"
        ) && input.target_analysis_id.is_none()
        || input.deadline_at.as_deref().is_some_and(|value| {
            value.trim() != value || chrono::DateTime::parse_from_rfc3339(value).is_err()
        })
    {
        return Err(BffOperationError::Invalid);
    }
    operation_budget::validate_input(&input.input).map_err(|_| BffOperationError::Invalid)?;
    let input_json = canonical(&input.input)?;
    if input_json.len() > 262_144 {
        return Err(BffOperationError::Invalid);
    }
    let input_sha = digest(&input_json);
    let mut tx = scoped(pool, scope.workspace, false).await?;
    type ReplayRow = (
        Uuid,
        String,
        String,
        Option<Uuid>,
        Option<Uuid>,
        i32,
        Uuid,
        bool,
    );
    let row: Option<ReplayRow> = if let Some(key) = &input.idempotency_key {
        sqlx::query_as("SELECT operation_id,input_sha256,input_json,target_analysis_id,parent_operation_id,step_budget,requested_by_principal_id,(deadline_at IS NOT DISTINCT FROM $5::timestamptz) FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_kind=$3 AND idempotency_key=$4")
            .bind(scope.workspace).bind(scope.situation).bind(&input.kind).bind(key).bind(input.deadline_at.as_deref()).fetch_optional(&mut *tx).await?
    } else {
        None
    };
    if let Some((existing, sha, json, target, parent, steps, actor, same_deadline)) = row {
        if sha != input_sha
            || json != input_json
            || target != input.target_analysis_id
            || parent != input.parent_operation_id
            || steps != input.step_budget
            || actor != input.requested_by
            || !same_deadline
        {
            return Err(BffOperationError::IdempotencyConflict);
        }
        tx.commit().await?;
        return get(
            pool,
            OperationScope {
                operation: existing,
                ..scope
            },
        )
        .await;
    }
    let deadline = input.deadline_at.as_deref();
    let inserted = sqlx::query("INSERT INTO niccolo.bff_operations(workspace_id,situation_id,operation_id,operation_kind,target_analysis_id,parent_operation_id,idempotency_key,input_json,input_sha256,deadline_at,step_budget,requested_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::timestamptz,$11,$12) ON CONFLICT DO NOTHING")
        .bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(&input.kind).bind(input.target_analysis_id).bind(input.parent_operation_id).bind(&input.idempotency_key).bind(&input_json).bind(&input_sha).bind(deadline).bind(input.step_budget).bind(input.requested_by).execute(&mut *tx).await?.rows_affected();
    if inserted == 0 {
        // Another transaction may have won the same idempotency key after our
        // initial lookup. ON CONFLICT DO NOTHING waits for its commit; a fresh
        // READ COMMITTED statement can now resolve the winning operation.
        if let Some(key) = &input.idempotency_key {
            let replay: Option<ReplayRow> = sqlx::query_as("SELECT operation_id,input_sha256,input_json,target_analysis_id,parent_operation_id,step_budget,requested_by_principal_id,(deadline_at IS NOT DISTINCT FROM $5::timestamptz) FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_kind=$3 AND idempotency_key=$4")
                .bind(scope.workspace).bind(scope.situation).bind(&input.kind).bind(key).bind(deadline).fetch_optional(&mut *tx).await?;
            if let Some((existing, sha, json, target, parent, steps, actor, same_deadline)) = replay
            {
                if sha != input_sha
                    || json != input_json
                    || target != input.target_analysis_id
                    || parent != input.parent_operation_id
                    || steps != input.step_budget
                    || actor != input.requested_by
                    || !same_deadline
                {
                    return Err(BffOperationError::IdempotencyConflict);
                }
                tx.commit().await?;
                return get(
                    pool,
                    OperationScope {
                        operation: existing,
                        ..scope
                    },
                )
                .await;
            }
        }
        type ExactReplayRow = (
            String,
            String,
            Option<Uuid>,
            Option<Uuid>,
            i32,
            Uuid,
            Option<String>,
            bool,
        );
        let exact: Option<ExactReplayRow> = sqlx::query_as("SELECT operation_kind,input_sha256,target_analysis_id,parent_operation_id,step_budget,requested_by_principal_id,idempotency_key,(deadline_at IS NOT DISTINCT FROM $4::timestamptz) FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3")
            .bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(deadline).fetch_optional(&mut *tx).await?;
        if exact
            != Some((
                input.kind.clone(),
                input_sha.clone(),
                input.target_analysis_id,
                input.parent_operation_id,
                input.step_budget,
                input.requested_by,
                input.idempotency_key.clone(),
                true,
            ))
        {
            return Err(BffOperationError::IdempotencyConflict);
        }
    } else {
        sqlx::query("INSERT INTO niccolo.bff_operation_events(workspace_id,situation_id,operation_id,event_kind,event_json,recorded_by_principal_id) VALUES($1,$2,$3,'QUEUED','{}',$4)")
            .bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(input.requested_by).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    get(pool, scope).await
}

pub async fn get(pool: &PgPool, scope: OperationScope) -> Result<OperationView, BffOperationError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let row: Option<OperationRow> = sqlx::query_as("SELECT operation_id,operation_kind,target_analysis_id,parent_operation_id,state,state_version,result_json,error_code,error_message,step_budget FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3")
        .bind(scope.workspace).bind(scope.situation).bind(scope.operation).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    row.map(row_to_view)
        .transpose()?
        .ok_or(BffOperationError::Invisible)
}

struct Transition<'a> {
    expected_version: i64,
    from: &'a [&'a str],
    to: &'a str,
    result: Option<&'a Value>,
    error: Option<(&'a str, &'a str)>,
    event_kind: &'a str,
    event_value: &'a Value,
    actor: Uuid,
}

async fn transition(
    pool: &PgPool,
    scope: OperationScope,
    spec: Transition<'_>,
) -> Result<OperationView, BffOperationError> {
    let result_json = spec.result.map(canonical).transpose()?;
    let event_json = canonical(spec.event_value)?;
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let changed = sqlx::query("UPDATE niccolo.bff_operations SET state=$4,state_version=state_version+1,result_json=$5,error_code=$6,error_message=$7,updated_at=clock_timestamp() WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND state_version=$8 AND state=ANY($9)")
        .bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(spec.to).bind(result_json).bind(spec.error.map(|x| x.0)).bind(spec.error.map(|x| x.1)).bind(spec.expected_version).bind(spec.from).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(BffOperationError::StateConflict);
    }
    sqlx::query("INSERT INTO niccolo.bff_operation_events(workspace_id,situation_id,operation_id,event_kind,event_json,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(spec.event_kind).bind(event_json).bind(spec.actor).execute(&mut *tx).await?;
    tx.commit().await?;
    get(pool, scope).await
}

pub async fn start(
    pool: &PgPool,
    scope: OperationScope,
    expected_version: i64,
    actor: Uuid,
) -> Result<OperationView, BffOperationError> {
    transition(
        pool,
        scope,
        Transition {
            expected_version,
            from: &["QUEUED"],
            to: "RUNNING",
            result: None,
            error: None,
            event_kind: "RUNNING",
            event_value: &serde_json::json!({}),
            actor,
        },
    )
    .await
}

pub async fn fail(
    pool: &PgPool,
    scope: OperationScope,
    expected_version: i64,
    actor: Uuid,
    code: &str,
    message: &str,
) -> Result<OperationView, BffOperationError> {
    if code.trim().is_empty()
        || code.len() > 128
        || message.trim().is_empty()
        || message.len() > 4096
    {
        return Err(BffOperationError::Invalid);
    }
    transition(
        pool,
        scope,
        Transition {
            expected_version,
            from: &["QUEUED", "RUNNING", "WAITING_FOR_HUMAN"],
            to: "FAILED",
            result: None,
            error: Some((code, message)),
            event_kind: "FAILED",
            event_value: &serde_json::json!({"code":code}),
            actor,
        },
    )
    .await
}

pub async fn cancel(
    pool: &PgPool,
    scope: OperationScope,
    expected_version: i64,
    actor: Uuid,
    reason: &str,
) -> Result<OperationView, BffOperationError> {
    if reason.trim().is_empty() || reason.len() > 4096 {
        return Err(BffOperationError::Invalid);
    }
    transition(
        pool,
        scope,
        Transition {
            expected_version,
            from: &["QUEUED", "RUNNING", "WAITING_FOR_HUMAN"],
            to: "CANCELLED",
            result: None,
            error: None,
            event_kind: "CANCELLED",
            event_value: &serde_json::json!({"reason":reason}),
            actor,
        },
    )
    .await
}

pub async fn wait_for_human(
    pool: &PgPool,
    scope: OperationScope,
    expected_version: i64,
    actor: Uuid,
    reason: &str,
) -> Result<OperationView, BffOperationError> {
    transition(
        pool,
        scope,
        Transition {
            expected_version,
            from: &["RUNNING"],
            to: "WAITING_FOR_HUMAN",
            result: None,
            error: None,
            event_kind: "WAITING_FOR_HUMAN",
            event_value: &serde_json::json!({"reason":reason}),
            actor,
        },
    )
    .await
}

pub async fn resume(
    pool: &PgPool,
    scope: OperationScope,
    expected_version: i64,
    actor: Uuid,
) -> Result<OperationView, BffOperationError> {
    transition(
        pool,
        scope,
        Transition {
            expected_version,
            from: &["WAITING_FOR_HUMAN"],
            to: "RUNNING",
            result: None,
            error: None,
            event_kind: "RESUMED",
            event_value: &serde_json::json!({}),
            actor,
        },
    )
    .await
}

pub async fn events(
    pool: &PgPool,
    scope: OperationScope,
    after: i64,
    limit: i64,
) -> Result<EventPage, BffOperationError> {
    if after < 0 || !(1..=200).contains(&limit) {
        return Err(BffOperationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    type Row = (i64, String, String);
    let rows:Vec<Row>=sqlx::query_as("SELECT event_seq,event_kind,event_json FROM niccolo.bff_operation_events WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND event_seq>$4 ORDER BY event_seq LIMIT $5")
        .bind(scope.workspace).bind(scope.situation).bind(scope.operation).bind(after).bind(limit+1).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let events = rows
        .into_iter()
        .take(limit as usize)
        .map(|(seq, kind, json)| {
            Ok(OperationEventView {
                seq,
                kind,
                value: serde_json::from_str(&json)?,
            })
        })
        .collect::<Result<Vec<_>, BffOperationError>>()?;
    let next_after = more.then(|| events.last().expect("non-empty cursor page").seq);
    Ok(EventPage { events, next_after })
}

pub async fn execute_deterministic(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
) -> Result<OperationView, BffOperationError> {
    let current = get(pool, scope).await?;
    if terminal(&current.state) {
        return Ok(current);
    }
    let running = if current.state == "QUEUED" {
        transition(
            pool,
            scope,
            Transition {
                expected_version: current.state_version,
                from: &["QUEUED"],
                to: "RUNNING",
                result: None,
                error: None,
                event_kind: "RUNNING",
                event_value: &serde_json::json!({}),
                actor,
            },
        )
        .await?
    } else if current.state == "RUNNING" {
        current
    } else {
        return Err(BffOperationError::StateConflict);
    };
    let analysis = running
        .target_analysis_id
        .ok_or(BffOperationError::Invalid)?;
    let analysis_scope = AnalysisScope {
        workspace: scope.workspace,
        situation: scope.situation,
        analysis,
    };
    let result = match running.kind.as_str() {
        "BUILD_BRIEFING" => serde_json::to_value(
            human_briefing::build(pool, analysis_scope)
                .await
                .map_err(|e| BffOperationError::Task(e.to_string()))?,
        )?,
        "BUILD_HANDOFF" => serde_json::to_value(
            reasoning_handoff::bundle(pool, analysis_scope)
                .await
                .map_err(|e| BffOperationError::Task(e.to_string()))?,
        )?,
        _ => return Err(BffOperationError::UnsupportedKind),
    };
    transition(
        pool,
        scope,
        Transition {
            expected_version: running.state_version,
            from: &["RUNNING"],
            to: "SUCCEEDED",
            result: Some(&result),
            error: None,
            event_kind: "SUCCEEDED",
            event_value: &serde_json::json!({"result_sha256":digest(&canonical(&result)?)}),
            actor,
        },
    )
    .await
}

/// v0.251-v0.260: pull-oriented bounded wait. This is transport behavior only;
/// it does not alter operation semantics or create a second task implementation.
pub async fn wait_for_version(
    pool: &PgPool,
    scope: OperationScope,
    after_version: i64,
    timeout_ms: u64,
) -> Result<OperationView, BffOperationError> {
    if after_version < 0 || !(1..=10_000).contains(&timeout_ms) {
        return Err(BffOperationError::Invalid);
    }
    let start = tokio::time::Instant::now();
    loop {
        let view = get(pool, scope).await?;
        if view.state_version > after_version
            || view.terminal
            || start.elapsed().as_millis() >= u128::from(timeout_ms)
        {
            return Ok(view);
        }
        let remaining = timeout_ms.saturating_sub(start.elapsed().as_millis() as u64);
        tokio::time::sleep(std::time::Duration::from_millis(remaining.min(100))).await;
    }
}

/// Shared terminal-success transition for agentic and deterministic runtimes.
pub async fn succeed(
    pool: &PgPool,
    scope: OperationScope,
    expected_version: i64,
    actor: Uuid,
    result: &Value,
) -> Result<OperationView, BffOperationError> {
    transition(
        pool,
        scope,
        Transition {
            expected_version,
            from: &["RUNNING"],
            to: "SUCCEEDED",
            result: Some(result),
            error: None,
            event_kind: "SUCCEEDED",
            event_value: &serde_json::json!({"result_sha256":digest(&canonical(result)?)}),
            actor,
        },
    )
    .await
}

/// Internal task input for server-side runtimes. This is not a second domain implementation.
pub async fn task_input(pool: &PgPool, scope: OperationScope) -> Result<Value, BffOperationError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let raw:Option<String>=sqlx::query_scalar("SELECT input_json FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3")
  .bind(scope.workspace).bind(scope.situation).bind(scope.operation).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    let raw = raw.ok_or(BffOperationError::Invisible)?;
    Ok(serde_json::from_str(&raw)?)
}

/// v0.311-v0.320: claim an execution lease without changing task semantics.
pub async fn claim_execution(
    pool: &PgPool,
    scope: OperationScope,
    owner: &str,
    lease_ms: i64,
) -> Result<bool, BffOperationError> {
    if owner.trim().is_empty() || owner.len() > 128 || !(1_000..=300_000).contains(&lease_ms) {
        return Err(BffOperationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let changed = sqlx::query(
        "UPDATE niccolo.bff_operations \
         SET execution_owner=$4, \
             execution_lease_until=clock_timestamp()+($5::bigint * interval '1 millisecond'), \
             execution_attempts=execution_attempts+1, updated_at=clock_timestamp() \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 \
           AND state IN ('QUEUED','RUNNING') \
           AND (execution_owner IS NULL OR execution_lease_until < clock_timestamp() OR execution_owner=$4)",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(owner)
    .bind(lease_ms)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(changed == 1)
}

pub async fn heartbeat_execution(
    pool: &PgPool,
    scope: OperationScope,
    owner: &str,
    lease_ms: i64,
) -> Result<bool, BffOperationError> {
    if owner.trim().is_empty() || owner.len() > 128 || !(1_000..=300_000).contains(&lease_ms) {
        return Err(BffOperationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let changed = sqlx::query(
        "UPDATE niccolo.bff_operations \
         SET execution_lease_until=clock_timestamp()+($5::bigint * interval '1 millisecond'), updated_at=clock_timestamp() \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 \
           AND execution_owner=$4 AND execution_lease_until >= clock_timestamp() \
           AND state IN ('QUEUED','RUNNING')",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(owner)
    .bind(lease_ms)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(changed == 1)
}

pub async fn release_execution(
    pool: &PgPool,
    scope: OperationScope,
    owner: &str,
) -> Result<bool, BffOperationError> {
    if owner.trim().is_empty() || owner.len() > 128 {
        return Err(BffOperationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, false).await?;
    let changed = sqlx::query(
        "UPDATE niccolo.bff_operations \
         SET execution_owner=NULL, execution_lease_until=NULL, updated_at=clock_timestamp() \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 AND execution_owner=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(owner)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(changed == 1)
}

/// v0.321-v0.330: claim the oldest executable operation in one authorized workspace.
/// Workers remain workspace-scoped; this does not introduce a privileged cross-tenant scanner.
pub async fn claim_next_in_workspace(
    pool: &PgPool,
    workspace: Uuid,
    owner: &str,
    lease_ms: i64,
) -> Result<Option<OperationScope>, BffOperationError> {
    claim_next_in_workspace_inner(pool, workspace, owner, lease_ms, None).await
}

/// Atomically enforce a workspace-wide active execution lease ceiling before
/// claiming another queued/recoverable operation. The workspace row lock
/// serializes competing admission decisions so several workers cannot all
/// observe the same capacity and oversubscribe it.
pub async fn claim_next_in_workspace_limited(
    pool: &PgPool,
    workspace: Uuid,
    owner: &str,
    lease_ms: i64,
    max_active_leases: u16,
) -> Result<Option<OperationScope>, BffOperationError> {
    if !(1..=512).contains(&max_active_leases) {
        return Err(BffOperationError::Invalid);
    }
    claim_next_in_workspace_inner(
        pool,
        workspace,
        owner,
        lease_ms,
        Some(i64::from(max_active_leases)),
    )
    .await
}

async fn claim_next_in_workspace_inner(
    pool: &PgPool,
    workspace: Uuid,
    owner: &str,
    lease_ms: i64,
    max_active_leases: Option<i64>,
) -> Result<Option<OperationScope>, BffOperationError> {
    if owner.trim().is_empty() || owner.len() > 128 || !(1_000..=300_000).contains(&lease_ms) {
        return Err(BffOperationError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;

    if let Some(limit) = max_active_leases {
        let visible: Option<Uuid> = sqlx::query_scalar(
            "SELECT workspace_id FROM niccolo.workspaces WHERE workspace_id=$1 FOR UPDATE",
        )
        .bind(workspace)
        .fetch_optional(&mut *tx)
        .await?;
        if visible.is_none() {
            return Err(BffOperationError::Invisible);
        }
        let active: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM niccolo.bff_operations              WHERE workspace_id=$1 AND execution_owner IS NOT NULL                AND execution_lease_until >= clock_timestamp()",
        )
        .bind(workspace)
        .fetch_one(&mut *tx)
        .await?;
        if active >= limit {
            tx.commit().await?;
            return Ok(None);
        }
    }

    let claimed: Option<(Uuid, Uuid)> = sqlx::query_as(
        "WITH candidate AS (            SELECT situation_id, operation_id FROM niccolo.bff_operations            WHERE workspace_id=$1 AND state IN ('QUEUED','RUNNING')              AND (execution_owner IS NULL OR execution_lease_until < clock_timestamp())            ORDER BY created_at, operation_id            FOR UPDATE SKIP LOCKED LIMIT 1          )          UPDATE niccolo.bff_operations o          SET execution_owner=$2,              execution_lease_until=clock_timestamp()+($3::bigint * interval '1 millisecond'),              execution_attempts=execution_attempts+1, updated_at=clock_timestamp()          FROM candidate c          WHERE o.workspace_id=$1 AND o.situation_id=c.situation_id AND o.operation_id=c.operation_id          RETURNING o.situation_id,o.operation_id",
    )
    .bind(workspace)
    .bind(owner)
    .bind(lease_ms)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(claimed.map(|(situation, operation)| OperationScope {
        workspace,
        situation,
        operation,
    }))
}

pub async fn execution_owned_by(
    pool: &PgPool,
    scope: OperationScope,
    owner: &str,
) -> Result<bool, BffOperationError> {
    if owner.trim().is_empty() || owner.len() > 128 {
        return Err(BffOperationError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.bff_operations \
         WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3 \
           AND execution_owner=$4 AND execution_lease_until >= clock_timestamp())",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(owner)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(owned)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationObservation {
    pub operation: OperationView,
    pub events: Vec<OperationEventView>,
    pub next_after_event: i64,
    pub operation_changed: bool,
    pub events_changed: bool,
    pub timed_out: bool,
}

/// v0.341-v0.360: one bounded PULL observation for state + event cursor.
/// The browser may stay simple: wait until either the operation version or event stream changes.
pub async fn observe(
    pool: &PgPool,
    scope: OperationScope,
    after_version: i64,
    after_event: i64,
    timeout_ms: u64,
    event_limit: i64,
) -> Result<OperationObservation, BffOperationError> {
    if after_version < 0
        || after_event < 0
        || !(1..=10_000).contains(&timeout_ms)
        || !(1..=200).contains(&event_limit)
    {
        return Err(BffOperationError::Invalid);
    }
    let started = tokio::time::Instant::now();
    loop {
        let operation = get(pool, scope).await?;
        let page = events(pool, scope, after_event, event_limit).await?;
        let operation_changed = operation.state_version > after_version;
        let events_changed = !page.events.is_empty();
        let timed_out = started.elapsed().as_millis() >= u128::from(timeout_ms);
        if operation_changed || events_changed || operation.terminal || timed_out {
            let next_after_event = page
                .events
                .last()
                .map(|event| event.seq)
                .unwrap_or(after_event);
            return Ok(OperationObservation {
                operation,
                events: page.events,
                next_after_event,
                operation_changed,
                events_changed,
                timed_out,
            });
        }
        let remaining = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
        tokio::time::sleep(std::time::Duration::from_millis(remaining.min(100))).await;
    }
}
