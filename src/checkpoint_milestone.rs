//! Explicit human declaration that one durable operation checkpoint is a
//! meaningful historical milestone.
//!
//! Milestone declaration is deliberately orthogonal to epistemic status and
//! remote publication. It preserves *why a human chose this checkpoint* while
//! retaining the checkpoint's existing provisional/final state unchanged.
use crate::{
    authz,
    bff_operations::OperationScope,
    operation_checkpoint::{self, CheckpointError},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum MilestoneError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("checkpoint unavailable: {0}")]
    Checkpoint(#[from] CheckpointError),
    #[error("invalid checkpoint milestone")]
    Invalid,
    #[error("checkpoint already has a different milestone declaration")]
    Conflict,
    #[error("checkpoint milestone unavailable")]
    Invisible,
    #[error("privileged SQL login prohibited")]
    Privileged,
}

#[derive(Clone, Debug)]
pub struct NewMilestone {
    pub milestone_id: Uuid,
    pub milestone_kind: String,
    pub reason: String,
    pub declared_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneView {
    pub milestone_id: Uuid,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub operation_id: Uuid,
    pub checkpoint_id: Uuid,
    pub checkpoint_seq: i64,
    pub checkpoint_kind: String,
    pub checkpoint_provisional: bool,
    pub checkpoint_terminal: bool,
    pub checkpoint_summary_sha256: String,
    pub milestone_kind: String,
    pub reason: String,
    pub declared_by_principal_id: Uuid,
    pub created_at: String,
    pub remote_publish_performed: bool,
    pub establishes_truth: bool,
    pub proves_completeness: bool,
    pub authorizes_real_world_action: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, MilestoneError> {
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(MilestoneError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid(input: &NewMilestone) -> bool {
    !input.milestone_id.is_nil()
        && !input.declared_by.is_nil()
        && matches!(
            input.milestone_kind.as_str(),
            "TIMED_PARTIAL"
                | "FIRST_PASS"
                | "HUMAN_ACKNOWLEDGED_REVIEW"
                | "FINAL_SYNTHESIS"
                | "OTHER"
        )
        && !input.reason.trim().is_empty()
        && input.reason.trim().len() <= 1000
        && input.reason == input.reason.trim()
        && !input.reason.chars().any(char::is_control)
}

type Row = (
    Uuid,
    String,
    String,
    Uuid,
    String,
    i64,
    String,
    bool,
    bool,
    String,
);

fn view(scope: OperationScope, checkpoint_id: Uuid, row: Row) -> MilestoneView {
    MilestoneView {
        milestone_id: row.0,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        operation_id: scope.operation,
        checkpoint_id,
        milestone_kind: row.1,
        reason: row.2,
        declared_by_principal_id: row.3,
        created_at: row.4,
        checkpoint_seq: row.5,
        checkpoint_kind: row.6,
        checkpoint_provisional: row.7,
        checkpoint_terminal: row.8,
        checkpoint_summary_sha256: row.9,
        remote_publish_performed: false,
        establishes_truth: false,
        proves_completeness: false,
        authorizes_real_world_action: false,
    }
}

const SELECT_ONE: &str = "SELECT m.milestone_id,m.milestone_kind,m.reason,m.declared_by_principal_id,m.created_at::text,\
 c.checkpoint_seq,c.checkpoint_kind,c.provisional,c.terminal,c.summary_sha256 \
 FROM niccolo.operation_checkpoint_milestones m \
 JOIN niccolo.bff_operation_checkpoints c \
   ON c.workspace_id=m.workspace_id AND c.situation_id=m.situation_id \
  AND c.operation_id=m.operation_id AND c.checkpoint_id=m.checkpoint_id \
 WHERE m.workspace_id=$1 AND m.situation_id=$2 AND m.operation_id=$3 AND m.checkpoint_id=$4";

pub async fn declare(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
    input: &NewMilestone,
) -> Result<(MilestoneView, bool), MilestoneError> {
    if checkpoint_id.is_nil() || !valid(input) {
        return Err(MilestoneError::Invalid);
    }
    // Make the source checkpoint's visibility and integrity explicit before the
    // declaration transaction. The FK remains the final database invariant.
    operation_checkpoint::get(pool, scope, checkpoint_id).await?;

    let mut tx = scoped(pool, scope.workspace).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.operation_checkpoint_milestones \
         (workspace_id,situation_id,operation_id,checkpoint_id,milestone_id,milestone_kind,reason,declared_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.operation)
    .bind(checkpoint_id)
    .bind(input.milestone_id)
    .bind(&input.milestone_kind)
    .bind(&input.reason)
    .bind(input.declared_by)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let row: Option<Row> = sqlx::query_as(SELECT_ONE)
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.operation)
        .bind(checkpoint_id)
        .fetch_optional(&mut *tx)
        .await?;
    let row = row.ok_or(MilestoneError::Conflict)?;
    if row.0 != input.milestone_id
        || row.1 != input.milestone_kind
        || row.2 != input.reason
        || row.3 != input.declared_by
    {
        return Err(MilestoneError::Conflict);
    }
    let result = view(scope, checkpoint_id, row);
    tx.commit().await?;
    Ok((result, !inserted))
}

pub async fn get(
    pool: &PgPool,
    scope: OperationScope,
    checkpoint_id: Uuid,
) -> Result<MilestoneView, MilestoneError> {
    if checkpoint_id.is_nil() {
        return Err(MilestoneError::Invalid);
    }
    let mut tx = scoped(pool, scope.workspace).await?;
    let row: Option<Row> = sqlx::query_as(SELECT_ONE)
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.operation)
        .bind(checkpoint_id)
        .fetch_optional(&mut *tx)
        .await?;
    let row = row.ok_or(MilestoneError::Invisible)?;
    tx.commit().await?;
    Ok(view(scope, checkpoint_id, row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: &str, reason: &str) -> NewMilestone {
        NewMilestone {
            milestone_id: Uuid::from_u128(1),
            milestone_kind: kind.into(),
            reason: reason.into(),
            declared_by: Uuid::from_u128(2),
        }
    }

    #[test]
    fn milestone_requires_explicit_bounded_reason_and_known_kind() {
        assert!(valid(&input(
            "FIRST_PASS",
            "First coherent pass worth preserving"
        )));
        assert!(!valid(&input("FIRST_PASS", "")));
        assert!(!valid(&input("FIRST_PASS", " padded ")));
        assert!(!valid(&input(
            "VERIFIED_TRUTH",
            "Not a permitted semantic upgrade"
        )));
        assert!(!valid(&input("OTHER", &"x".repeat(1001))));
        for value in ["line\nforged", "line\rforged", "line\tforged", "x\0y"] {
            assert!(!valid(&input("OTHER", value)));
        }
    }
}
