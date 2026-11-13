#![cfg(feature = "pg-integration")]
use niccolo_evidence::{authz, benchmark_budget, benchmark_budget::BenchmarkBudgetError};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

struct Harness {
    admin: PgPool,
    secure: PgPool,
    workspace: Uuid,
    actor: Uuid,
    digest: String,
    subject: String,
}

async fn setup() -> Harness {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(12)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let actor = Uuid::new_v4();
    let subject = format!("benchmark-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'budget-reservation')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, actor, workspace, true, true, 1)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.evaluation_policies
         (workspace_id,account_subject,background_evaluations_enabled,
          extra_model_probes_allowed,permitted_providers,prohibited_models,
          monthly_cap_micro_usd,monthly_spent_micro_usd,monthly_reserved_micro_usd,
          maximum_background_utilization_permille,billing_cycle_start)
         VALUES($1,$2,TRUE,TRUE,ARRAY['allowed-provider'],ARRAY['blocked-model'],
                10000,1000,500,500,current_date)",
    )
    .bind(workspace)
    .bind(&subject)
    .execute(&admin)
    .await
    .unwrap();
    Harness {
        admin,
        secure,
        workspace,
        actor,
        digest: authz::credential_digest(&token).unwrap(),
        subject,
    }
}

async fn seed_run(h: &Harness, run: Uuid, projected: i64) {
    sqlx::query(
        "INSERT INTO niccolo.benchmark_runs
         (workspace_id,run_id,account_subject,case_label,mode,frontier_provider,frontier_model,
          raw_original_source_ids,external_egress_authorized,extra_model_probe_authorized,
          projected_cost_micro_usd)
         VALUES($1,$2,$3,'case','CHEAP_VS_FRONTIER','allowed-provider','frontier-model',
                ARRAY['source-1'],TRUE,TRUE,$4)",
    )
    .bind(h.workspace)
    .bind(run)
    .bind(&h.subject)
    .bind(projected)
    .execute(&h.admin)
    .await
    .unwrap();
}

#[tokio::test]
async fn reservation_replay_settlement_and_release_adjust_policy_exactly_once() {
    let h = setup().await;
    let run = Uuid::new_v4();
    let reservation = Uuid::new_v4();
    seed_run(&h, run, 1000).await;
    let created = authz::with_auth_digest(
        h.digest.clone(),
        benchmark_budget::reserve(
            &h.secure,
            h.workspace,
            h.actor,
            reservation,
            run,
            &h.subject,
            1000,
        ),
    )
    .await
    .unwrap();
    assert!(!created.replayed);
    assert_eq!(created.status, "RESERVED");
    let replay = authz::with_auth_digest(
        h.digest.clone(),
        benchmark_budget::reserve(
            &h.secure,
            h.workspace,
            h.actor,
            reservation,
            run,
            &h.subject,
            1000,
        ),
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    let counters: (i64, i64) = sqlx::query_as(
        "SELECT monthly_spent_micro_usd,monthly_reserved_micro_usd
           FROM niccolo.evaluation_policies WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(h.workspace)
    .bind(&h.subject)
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(counters, (1000, 1500));

    let settled = authz::with_auth_digest(
        h.digest.clone(),
        benchmark_budget::settle(&h.secure, h.workspace, h.actor, reservation, 800),
    )
    .await
    .unwrap();
    assert_eq!(settled.status, "SETTLED");
    assert_eq!(settled.actual_micro_usd, Some(800));
    let settled_replay = authz::with_auth_digest(
        h.digest.clone(),
        benchmark_budget::settle(&h.secure, h.workspace, h.actor, reservation, 800),
    )
    .await
    .unwrap();
    assert!(settled_replay.replayed);
    let run_actual: Option<i64> = sqlx::query_scalar(
        "SELECT actual_cost_micro_usd FROM niccolo.benchmark_runs
          WHERE workspace_id=$1 AND run_id=$2",
    )
    .bind(h.workspace)
    .bind(run)
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(run_actual, Some(800));
    assert!(matches!(
        authz::with_auth_digest(
            h.digest.clone(),
            benchmark_budget::reserve(
                &h.secure,
                h.workspace,
                h.actor,
                reservation,
                run,
                &h.subject,
                1000,
            ),
        )
        .await,
        Err(BenchmarkBudgetError::ReservationClosed)
    ));
    let counters: (i64, i64) = sqlx::query_as(
        "SELECT monthly_spent_micro_usd,monthly_reserved_micro_usd
           FROM niccolo.evaluation_policies WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(h.workspace)
    .bind(&h.subject)
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(counters, (1800, 500));

    let run2 = Uuid::new_v4();
    let reservation2 = Uuid::new_v4();
    seed_run(&h, run2, 500).await;
    authz::with_auth_digest(
        h.digest.clone(),
        benchmark_budget::reserve(
            &h.secure,
            h.workspace,
            h.actor,
            reservation2,
            run2,
            &h.subject,
            500,
        ),
    )
    .await
    .unwrap();
    let released = authz::with_auth_digest(
        h.digest.clone(),
        benchmark_budget::release(&h.secure, h.workspace, h.actor, reservation2),
    )
    .await
    .unwrap();
    assert_eq!(released.status, "RELEASED");
    let counters: (i64, i64) = sqlx::query_as(
        "SELECT monthly_spent_micro_usd,monthly_reserved_micro_usd
           FROM niccolo.evaluation_policies WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(h.workspace)
    .bind(&h.subject)
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(counters, (1800, 500));
}

#[tokio::test]
async fn concurrent_reservations_cannot_double_spend_remaining_threshold() {
    let h = setup().await;
    sqlx::query(
        "UPDATE niccolo.evaluation_policies
            SET monthly_spent_micro_usd=0,monthly_reserved_micro_usd=0,
                maximum_background_utilization_permille=300
          WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(h.workspace)
    .bind(&h.subject)
    .execute(&h.admin)
    .await
    .unwrap();
    let run_a = Uuid::new_v4();
    let run_b = Uuid::new_v4();
    seed_run(&h, run_a, 2000).await;
    seed_run(&h, run_b, 2000).await;
    let secure_a = h.secure.clone();
    let secure_b = h.secure.clone();
    let subject_a = h.subject.clone();
    let subject_b = h.subject.clone();
    let digest_a = h.digest.clone();
    let digest_b = h.digest.clone();
    let workspace = h.workspace;
    let actor = h.actor;
    let task_a = tokio::spawn(async move {
        authz::with_auth_digest(
            digest_a,
            benchmark_budget::reserve(
                &secure_a,
                workspace,
                actor,
                Uuid::new_v4(),
                run_a,
                &subject_a,
                2000,
            ),
        )
        .await
    });
    let task_b = tokio::spawn(async move {
        authz::with_auth_digest(
            digest_b,
            benchmark_budget::reserve(
                &secure_b,
                workspace,
                actor,
                Uuid::new_v4(),
                run_b,
                &subject_b,
                2000,
            ),
        )
        .await
    });
    let results = [task_a.await.unwrap(), task_b.await.unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                matches!(
                    result,
                    Err(BenchmarkBudgetError::UtilizationThresholdExceeded)
                )
            })
            .count(),
        1
    );
    let reserved: i64 = sqlx::query_scalar(
        "SELECT monthly_reserved_micro_usd FROM niccolo.evaluation_policies
          WHERE workspace_id=$1 AND account_subject=$2",
    )
    .bind(h.workspace)
    .bind(&h.subject)
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(reserved, 2000);
}
