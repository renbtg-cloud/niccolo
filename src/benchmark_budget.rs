//! Transactional monetary-budget reservations for optional benchmark probes.
//!
//! Callers reserve before any paid provider side effect and settle/release
//! afterwards. PostgreSQL row locks serialize competing reservations so
//! concurrent workers cannot each spend the same remaining budget.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum BenchmarkBudgetError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid benchmark budget request")]
    Invalid,
    #[error("benchmark budget policy or run unavailable")]
    Invisible,
    #[error("benchmark budget reservation conflicts with stored scope")]
    Conflict,
    #[error("background evaluation disabled")]
    BackgroundDisabled,
    #[error("extra model probe not authorized")]
    ProbeNotAuthorized,
    #[error("provider is not permitted")]
    ProviderNotPermitted,
    #[error("model is prohibited")]
    ModelProhibited,
    #[error("original-data egress is not authorized")]
    DataEgressNotAuthorized,
    #[error("no monthly budget is configured")]
    NoConfiguredBudget,
    #[error("monthly budget would be exceeded")]
    MonthlyBudgetExceeded,
    #[error("background utilization threshold would be exceeded")]
    UtilizationThresholdExceeded,
    #[error("reservation is not open")]
    ReservationClosed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetReservation {
    pub reservation_id: Uuid,
    pub run_id: Uuid,
    pub account_subject: String,
    pub estimated_micro_usd: u64,
    pub status: String,
    pub actual_micro_usd: Option<u64>,
    pub created_by_principal_id: Uuid,
    pub closed_by_principal_id: Option<Uuid>,
    pub replayed: bool,
}

type ReservationRow = (Uuid, Uuid, String, i64, String, Option<i64>, Uuid, Option<Uuid>);

fn valid_subject(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 512
        && !value.chars().any(char::is_control)
}

fn to_u64(value: i64) -> Result<u64, BenchmarkBudgetError> {
    u64::try_from(value).map_err(|_| BenchmarkBudgetError::Conflict)
}

fn from_row(row: ReservationRow, replayed: bool) -> Result<BudgetReservation, BenchmarkBudgetError> {
    Ok(BudgetReservation {
        reservation_id: row.0,
        run_id: row.1,
        account_subject: row.2,
        estimated_micro_usd: to_u64(row.3)?,
        status: row.4,
        actual_micro_usd: row.5.map(to_u64).transpose()?,
        created_by_principal_id: row.6,
        closed_by_principal_id: row.7,
        replayed,
    })
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, BenchmarkBudgetError> {
    let mut tx = pool.begin().await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(BenchmarkBudgetError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

#[expect(
    clippy::too_many_arguments,
    reason = "reservation identity, run scope and monetary estimate are independent"
)]
pub async fn reserve(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    reservation_id: Uuid,
    run_id: Uuid,
    account_subject: &str,
    estimated_micro_usd: u64,
) -> Result<BudgetReservation, BenchmarkBudgetError> {
    if workspace.is_nil()
        || actor.is_nil()
        || reservation_id.is_nil()
        || run_id.is_nil()
        || !valid_subject(account_subject)
        || estimated_micro_usd == 0
        || estimated_micro_usd > i64::MAX as u64
    {
        return Err(BenchmarkBudgetError::Invalid);
    }
    let estimated = estimated_micro_usd as i64;
    let mut tx = scoped(pool, workspace).await?;

    let policy: Option<(bool, bool, Vec<String>, Vec<String>, Option<i64>, i64, i64, i16)> =
        sqlx::query_as(
            "SELECT background_evaluations_enabled,extra_model_probes_allowed,
                    permitted_providers,prohibited_models,monthly_cap_micro_usd,
                    monthly_spent_micro_usd,monthly_reserved_micro_usd,
                    maximum_background_utilization_permille
               FROM niccolo.evaluation_policies
              WHERE workspace_id=$1 AND account_subject=$2
              FOR UPDATE",
        )
        .bind(workspace)
        .bind(account_subject)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((
        background_enabled,
        probe_allowed,
        permitted_providers,
        prohibited_models,
        cap,
        spent,
        reserved,
        threshold,
    )) = policy
    else {
        return Err(BenchmarkBudgetError::Invisible);
    };

    let run: Option<(String, String, String, bool, bool, i64, String)> = sqlx::query_as(
        "SELECT account_subject,frontier_provider,frontier_model,
                external_egress_authorized,extra_model_probe_authorized,
                projected_cost_micro_usd,status
           FROM niccolo.benchmark_runs
          WHERE workspace_id=$1 AND run_id=$2
          FOR UPDATE",
    )
    .bind(workspace)
    .bind(run_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((run_subject, provider, model, egress, run_probe_authorized, projected, run_status)) =
        run
    else {
        return Err(BenchmarkBudgetError::Invisible);
    };
    if run_subject != account_subject
        || projected != estimated
        || !matches!(run_status.as_str(), "CREATED" | "RUNNING")
    {
        return Err(BenchmarkBudgetError::Conflict);
    }
    if !background_enabled {
        return Err(BenchmarkBudgetError::BackgroundDisabled);
    }
    if !probe_allowed || !run_probe_authorized {
        return Err(BenchmarkBudgetError::ProbeNotAuthorized);
    }
    if !permitted_providers.iter().any(|item| item == &provider) {
        return Err(BenchmarkBudgetError::ProviderNotPermitted);
    }
    if prohibited_models.iter().any(|item| item == &model) {
        return Err(BenchmarkBudgetError::ModelProhibited);
    }
    if !egress {
        return Err(BenchmarkBudgetError::DataEgressNotAuthorized);
    }

    let existing: Option<ReservationRow> = sqlx::query_as(
        "SELECT reservation_id,run_id,account_subject,estimated_micro_usd,status,
                actual_micro_usd,created_by_principal_id,closed_by_principal_id
           FROM niccolo.benchmark_budget_reservations
          WHERE workspace_id=$1 AND (reservation_id=$2 OR run_id=$3)",
    )
    .bind(workspace)
    .bind(reservation_id)
    .bind(run_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(row) = existing {
        if row.0 != reservation_id
            || row.1 != run_id
            || row.2 != account_subject
            || row.3 != estimated
            || row.6 != actor
        {
            return Err(BenchmarkBudgetError::Conflict);
        }
        tx.commit().await?;
        return from_row(row, true);
    }

    let cap = cap
        .filter(|value| *value > 0)
        .ok_or(BenchmarkBudgetError::NoConfiguredBudget)?;
    if !(1..=1000).contains(&threshold) || spent < 0 || reserved < 0 {
        return Err(BenchmarkBudgetError::Conflict);
    }
    let prospective = (spent as u128) + (reserved as u128) + (estimated as u128);
    if prospective > cap as u128 {
        return Err(BenchmarkBudgetError::MonthlyBudgetExceeded);
    }
    if prospective * 1000 > (cap as u128) * (threshold as u128) {
        return Err(BenchmarkBudgetError::UtilizationThresholdExceeded);
    }
    let new_reserved = reserved
        .checked_add(estimated)
        .ok_or(BenchmarkBudgetError::MonthlyBudgetExceeded)?;

    sqlx::query(
        "INSERT INTO niccolo.benchmark_budget_reservations
         (workspace_id,reservation_id,run_id,account_subject,estimated_micro_usd,created_by_principal_id)
         VALUES($1,$2,$3,$4,$5,$6)",
    )
    .bind(workspace)
    .bind(reservation_id)
    .bind(run_id)
    .bind(account_subject)
    .bind(estimated)
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE niccolo.evaluation_policies
            SET monthly_reserved_micro_usd=$3,updated_at=clock_timestamp()
          WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(workspace)
    .bind(account_subject)
    .bind(new_reserved)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(BudgetReservation {
        reservation_id,
        run_id,
        account_subject: account_subject.to_owned(),
        estimated_micro_usd,
        status: "RESERVED".into(),
        actual_micro_usd: None,
        created_by_principal_id: actor,
        closed_by_principal_id: None,
        replayed: false,
    })
}

pub async fn settle(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    reservation_id: Uuid,
    actual_micro_usd: u64,
) -> Result<BudgetReservation, BenchmarkBudgetError> {
    close(pool, workspace, actor, reservation_id, Some(actual_micro_usd)).await
}

pub async fn release(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    reservation_id: Uuid,
) -> Result<BudgetReservation, BenchmarkBudgetError> {
    close(pool, workspace, actor, reservation_id, None).await
}

async fn close(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    reservation_id: Uuid,
    actual_micro_usd: Option<u64>,
) -> Result<BudgetReservation, BenchmarkBudgetError> {
    if workspace.is_nil()
        || actor.is_nil()
        || reservation_id.is_nil()
        || actual_micro_usd.is_some_and(|value| value > i64::MAX as u64)
    {
        return Err(BenchmarkBudgetError::Invalid);
    }
    let mut tx = scoped(pool, workspace).await?;
    let row: Option<ReservationRow> = sqlx::query_as(
        "SELECT reservation_id,run_id,account_subject,estimated_micro_usd,status,
                actual_micro_usd,created_by_principal_id,closed_by_principal_id
           FROM niccolo.benchmark_budget_reservations
          WHERE workspace_id=$1 AND reservation_id=$2
          FOR UPDATE",
    )
    .bind(workspace)
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(BenchmarkBudgetError::Invisible);
    };
    let desired_status = if actual_micro_usd.is_some() {
        "SETTLED"
    } else {
        "RELEASED"
    };
    if row.4 != "RESERVED" {
        let actual_matches = row.5.map(to_u64).transpose()? == actual_micro_usd;
        if row.4 == desired_status && actual_matches && row.7 == Some(actor) {
            tx.commit().await?;
            return from_row(row, true);
        }
        return Err(BenchmarkBudgetError::ReservationClosed);
    }

    let policy: Option<(i64, i64)> = sqlx::query_as(
        "SELECT monthly_spent_micro_usd,monthly_reserved_micro_usd
           FROM niccolo.evaluation_policies
          WHERE workspace_id=$1 AND account_subject=$2
          FOR UPDATE",
    )
    .bind(workspace)
    .bind(&row.2)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((spent, reserved)) = policy else {
        return Err(BenchmarkBudgetError::Invisible);
    };
    if spent < 0 || reserved < row.3 {
        return Err(BenchmarkBudgetError::Conflict);
    }
    let new_reserved = reserved - row.3;
    let new_spent = match actual_micro_usd {
        Some(actual) => spent
            .checked_add(actual as i64)
            .ok_or(BenchmarkBudgetError::MonthlyBudgetExceeded)?,
        None => spent,
    };
    sqlx::query(
        "UPDATE niccolo.evaluation_policies
            SET monthly_spent_micro_usd=$3,monthly_reserved_micro_usd=$4,
                updated_at=clock_timestamp()
          WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(workspace)
    .bind(&row.2)
    .bind(new_spent)
    .bind(new_reserved)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE niccolo.benchmark_budget_reservations
            SET status=$3,actual_micro_usd=$4,closed_by_principal_id=$5,
                closed_at=clock_timestamp()
          WHERE workspace_id=$1 AND reservation_id=$2 AND status='RESERVED'",
    )
    .bind(workspace)
    .bind(reservation_id)
    .bind(desired_status)
    .bind(actual_micro_usd.map(|value| value as i64))
    .bind(actor)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(BudgetReservation {
        reservation_id: row.0,
        run_id: row.1,
        account_subject: row.2,
        estimated_micro_usd: to_u64(row.3)?,
        status: desired_status.into(),
        actual_micro_usd,
        created_by_principal_id: row.6,
        closed_by_principal_id: Some(actor),
        replayed: false,
    })
}
