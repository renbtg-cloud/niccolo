//! Immutable anytime-operation progress checkpoints.
//!
//! Checkpoints are durable observations of work already completed. A provisional
//! checkpoint is not a verdict, proof, completeness claim, or permission to act.
use crate::bff_operations::OperationScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const MAX_CHECKPOINTS_PER_OPERATION: i64 = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("checkpoint serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid operation checkpoint")]
    Invalid,
    #[error("operation checkpoint replay conflicts with recorded artifact")]
    Conflict,
    #[error("operation checkpoint unavailable")]
    Invisible,
    #[error("operation checkpoint limit reached")]
    TooMany,
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug)]
pub struct NewCheckpoint {
    pub checkpoint_id: Uuid,
    pub analysis_id: Option<Uuid>,
    pub checkpoint_kind: String,
    pub step_id: Option<Uuid>,
    pub summary: Value,
    pub provisional: bool,
    pub terminal: bool,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointView {
    pub checkpoint_id: Uuid,
    pub checkpoint_seq: i64,
    pub analysis_id: Option<Uuid>,
    pub checkpoint_kind: String,
    pub step_id: Option<Uuid>,
    pub summary: Value,
    pub summary_sha256: String,
    pub provisional: bool,
    pub terminal: bool,
    pub recorded_by_principal_id: Uuid,
    pub created_at: String,
    pub establishes_truth: bool,
    pub proves_completeness: bool,
    pub authorizes_real_world_action: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointPage {
    pub checkpoints: Vec<CheckpointView>,
    pub next_after: Option<i64>,
}

type Row = (
    Uuid,
    i64,
    Option<Uuid>,
    String,
    Option<Uuid>,
    Value,
    String,
    bool,
    bool,
    Uuid,
    String,
);

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, CheckpointError> {
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
        return Err(CheckpointError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid(input: &NewCheckpoint) -> bool {
    let kind_ok = matches!(
        input.checkpoint_kind.as_str(),
        "STEP_COMPLETED" | "HUMAN_BOUNDARY" | "BUDGET_BOUNDARY" | "FINAL"
    );
    let state_ok = match input.checkpoint_kind.as_str() {
        "FINAL" => input.terminal && !input.provisional,
        "BUDGET_BOUNDARY" => input.terminal && input.provisional,
        "STEP_COMPLETED" | "HUMAN_BOUNDARY" => !input.terminal && input.provisional,
        _ => false,
    };
    kind_ok
        && state_ok
        && !input.checkpoint_id.is_nil()
        && !input.recorded_by.is_nil()
        && input.analysis_id.is_none_or(|id| !id.is_nil())
        && input.step_id.is_none_or(|id| !id.is_nil())
        && input.summary.is_object()
}

fn digest(summary: &Value) -> Result<String, serde_json::Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(summary)?)))
}

fn view(row: Row) -> Result<CheckpointView, CheckpointError> {
    let actual = digest(&row.5)?;
    if actual != row.6 {
        return Err(CheckpointError::Conflict);
    }
    Ok(CheckpointView {
        checkpoint_id: row.0,
        checkpoint_seq: row.1,
        analysis_id: row.2,
        checkpoint_kind: row.3,
        step_id: row.4,
        summary: row.5,
        summary_sha256: row.6,
        provisional: row.7,
        terminal: row.8,
        recorded_by_principal_id: row.9,
        created_at: row.10,
        establishes_truth: false,
        proves_completeness: false,
        authorizes_real_world_action: false,
    })
}

pub async fn record(
    pool: &PgPool,
    scope: OperationScope,
    input: &NewCheckpoint,
) -> Result<(CheckpointView, bool), CheckpointError> {
    if !valid(input) {
        return Err(CheckpointError::Invalid);
    }
    let summary_sha256 = digest(&input.summary)?;
    let summary_bytes = serde_json::to_vec(&input.summary)?;
    if summary_bytes.len() > 262_144 {
        return Err(CheckpointError::Invalid);
    }

    let mut tx = scoped(pool, scope.workspace, false).await?;
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT checkpoint_id
           FROM niccolo.bff_operation_checkpoints
          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
            AND checkpoint_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(input.checkpoint_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing.is_none() {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint
               FROM niccolo.bff_operation_checkpoints
              WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.operation)
        .fetch_one(&mut *tx)
        .await?;
        if count >= MAX_CHECKPOINTS_PER_OPERATION {
            return Err(CheckpointError::TooMany);
        }
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.bff_operation_checkpoints
         (workspace_id,situation_id,operation_id,checkpoint_id,analysis_id,
          checkpoint_kind,step_id,summary_json,summary_sha256,provisional,terminal,
          recorded_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(input.checkpoint_id)
    .bind(input.analysis_id)
    .bind(&input.checkpoint_kind)
    .bind(input.step_id)
    .bind(&input.summary)
    .bind(&summary_sha256)
    .bind(input.provisional)
    .bind(input.terminal)
    .bind(input.recorded_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: Row = sqlx::query_as(
        "SELECT checkpoint_id,checkpoint_seq,analysis_id,checkpoint_kind,step_id,
                summary_json,summary_sha256,provisional,terminal,
                recorded_by_principal_id,created_at::text
           FROM niccolo.bff_operation_checkpoints
          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
            AND checkpoint_id=$4",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(input.checkpoint_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(CheckpointError::Conflict)?;

    if row.2 != input.analysis_id
        || row.3 != input.checkpoint_kind
        || row.4 != input.step_id
        || row.5 != input.summary
        || row.6 != summary_sha256
        || row.7 != input.provisional
        || row.8 != input.terminal
        || row.9 != input.recorded_by
    {
        return Err(CheckpointError::Conflict);
    }
    let stored = view(row)?;
    tx.commit().await?;
    Ok((stored, !inserted))
}

pub async fn list(
    pool: &PgPool,
    scope: OperationScope,
    after: i64,
    limit: i64,
) -> Result<CheckpointPage, CheckpointError> {
    if after < 0 || !(1..=200).contains(&limit) {
        return Err(CheckpointError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT checkpoint_id,checkpoint_seq,analysis_id,checkpoint_kind,step_id,
                summary_json,summary_sha256,provisional,terminal,
                recorded_by_principal_id,created_at::text
           FROM niccolo.bff_operation_checkpoints
          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3
            AND checkpoint_seq>$4
          ORDER BY checkpoint_seq
          LIMIT $5",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let checkpoints = rows
        .into_iter()
        .take(limit as usize)
        .map(view)
        .collect::<Result<Vec<_>, CheckpointError>>()?;
    let next_after = more.then(|| checkpoints.last().expect("non-empty checkpoint page").checkpoint_seq);
    Ok(CheckpointPage {
        checkpoints,
        next_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semantic_flags_never_turn_progress_into_truth() {
        let row: Row = (
            Uuid::from_u128(1),
            1,
            Some(Uuid::from_u128(2)),
            "STEP_COMPLETED".into(),
            Some(Uuid::from_u128(3)),
            json!({"completed_steps":1}),
            digest(&json!({"completed_steps":1})).unwrap(),
            true,
            false,
            Uuid::from_u128(4),
            "2026-11-12 00:00:00+00".into(),
        );
        let checkpoint = view(row).unwrap();
        assert!(!checkpoint.establishes_truth);
        assert!(!checkpoint.proves_completeness);
        assert!(!checkpoint.authorizes_real_world_action);
    }

    #[test]
    fn final_checkpoint_must_not_be_provisional() {
        let invalid = NewCheckpoint {
            checkpoint_id: Uuid::from_u128(1),
            analysis_id: None,
            checkpoint_kind: "FINAL".into(),
            step_id: None,
            summary: json!({}),
            provisional: true,
            terminal: true,
            recorded_by: Uuid::from_u128(2),
        };
        assert!(!valid(&invalid));
    }
}
