#![cfg(feature = "pg-integration")]
use niccolo_evidence::postgres_repository::{
    GraphLoadLimits, RepositoryError, load_workspace_graph, load_workspace_graph_with_limits,
};
use niccolo_evidence::provenance_dag::TraceError;
use sqlx::{PgPool, QueryBuilder, postgres::PgPoolOptions};
use std::collections::BTreeSet;
use uuid::Uuid;

async fn pools() -> (PgPool, PgPool) {
    let admin = std::env::var("NICCOLO_PG_ADMIN_URL")
        .expect("set NICCOLO_PG_ADMIN_URL for integration tests");
    let service = std::env::var("NICCOLO_PG_SERVICE_URL")
        .expect("set NICCOLO_PG_SERVICE_URL for integration tests");
    let admin = PgPoolOptions::new()
        .max_connections(3)
        .connect(&admin)
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .max_connections(3)
        .connect(&service)
        .await
        .unwrap();
    (admin, service)
}
async fn workspace(db: &PgPool) -> Uuid {
    let w = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES ($1, $2)")
        .bind(w)
        .bind("integration isolated")
        .execute(db)
        .await
        .unwrap();
    w
}
async fn evidence(db: &PgPool, w: Uuid, id: Uuid, admissible: bool) {
    sqlx::query("INSERT INTO niccolo.evidence (workspace_id, evidence_id, source_locator, content, source_class, admissible) VALUES ($1, $2, 'fixture', 'synthetic', 'DIRECT_OBSERVATION', $3)")
        .bind(w).bind(id).bind(admissible).execute(db).await.unwrap();
}
async fn edge(db: &PgPool, w: Uuid, child: Uuid, parent: Uuid) {
    sqlx::query("INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) VALUES ($1,$2,$3,'DERIVED_FROM')")
        .bind(w).bind(child).bind(parent).execute(db).await.unwrap();
}

#[tokio::test]
async fn branches_copies_and_independence_not_asserted() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let (a, b, c, d) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    for id in [a, b, c, d] {
        evidence(&admin, w, id, true).await;
    }
    edge(&admin, w, c, a).await;
    edge(&admin, w, c, b).await;
    edge(&admin, w, d, a).await;
    let snapshot = load_workspace_graph(&service, w).await.unwrap();
    let result = snapshot.assess(&[c, d]).unwrap();
    assert_eq!(result.report_count, 2);
    assert_eq!(
        result.roots_per_report,
        vec![BTreeSet::from([a, b]), BTreeSet::from([a])]
    );
    assert_eq!(result.traceable_roots, BTreeSet::from([a, b]));
    assert_eq!(result.shared_origins.len(), 1);
    assert_eq!(result.shared_origins[0].origins, BTreeSet::from([a]));
    assert!(!result.independence_established);
}

#[tokio::test]
async fn withdrawn_source_fails_closed_and_new_snapshot_reflects_revocation() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let (root, copy) = (Uuid::new_v4(), Uuid::new_v4());
    evidence(&admin, w, root, true).await;
    evidence(&admin, w, copy, true).await;
    edge(&admin, w, copy, root).await;
    let old = load_workspace_graph(&service, w).await.unwrap();
    assert_eq!(
        old.assess(&[copy]).unwrap().traceable_roots,
        BTreeSet::from([root])
    );
    sqlx::query(
        "UPDATE niccolo.evidence SET admissible=false WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(w)
    .bind(root)
    .execute(&admin)
    .await
    .unwrap();
    let fresh = load_workspace_graph(&service, w).await.unwrap();
    assert!(matches!(
        fresh.assess(&[copy]),
        Err(RepositoryError::Trace(TraceError::Withdrawn(_)))
    ));
    // Existing detached snapshots aren't live subscriptions; don't reuse them after authorization changes.
    assert!(old.assess(&[copy]).is_ok());
}

#[tokio::test]
async fn cycles_fail_closed() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    evidence(&admin, w, a, true).await;
    evidence(&admin, w, b, true).await;
    edge(&admin, w, a, b).await;
    edge(&admin, w, b, a).await;
    let graph = load_workspace_graph(&service, w).await.unwrap();
    assert!(matches!(
        graph.assess(&[a]),
        Err(RepositoryError::Trace(TraceError::Cycle(_)))
    ));
}

#[tokio::test]
async fn tenant_isolation_even_when_ids_collide() {
    let (admin, service) = pools().await;
    let (w1, w2) = (workspace(&admin).await, workspace(&admin).await);
    let (shared, r1, r2, only2) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    evidence(&admin, w1, r1, true).await;
    evidence(&admin, w1, shared, true).await;
    edge(&admin, w1, shared, r1).await;
    evidence(&admin, w2, r2, true).await;
    evidence(&admin, w2, shared, true).await;
    evidence(&admin, w2, only2, true).await;
    edge(&admin, w2, shared, r2).await;
    let g1 = load_workspace_graph(&service, w1).await.unwrap();
    let g2 = load_workspace_graph(&service, w2).await.unwrap();
    assert_eq!(
        g1.assess(&[shared]).unwrap().traceable_roots,
        BTreeSet::from([r1])
    );
    assert_eq!(
        g2.assess(&[shared]).unwrap().traceable_roots,
        BTreeSet::from([r2])
    );
    assert!(matches!(
        g1.assess(&[only2]),
        Err(RepositoryError::InvisibleReport(_))
    ));
}

#[tokio::test]
async fn missing_rls_scope_denies_read_and_write() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    evidence(&admin, w, Uuid::new_v4(), true).await;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.evidence")
        .fetch_one(&service)
        .await
        .unwrap();
    assert_eq!(total, 0);
    assert!(sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES ($1,$2,'x','x','DIRECT_OBSERVATION')")
        .bind(w).bind(Uuid::new_v4()).execute(&service).await.is_err());
}

#[tokio::test]
async fn privileged_connections_are_explicitly_rejected() {
    let (admin, _service) = pools().await;
    let w = workspace(&admin).await;
    assert!(matches!(
        load_workspace_graph(&admin, w).await,
        Err(RepositoryError::PrivilegedRole)
    ));
}

#[tokio::test]
async fn concurrent_tenant_snapshots_do_not_leak_session_scope() {
    let (admin, service) = pools().await;
    let (w1, w2) = (workspace(&admin).await, workspace(&admin).await);
    let shared = Uuid::new_v4();
    let (r1, r2) = (Uuid::new_v4(), Uuid::new_v4());
    for (w, r) in [(w1, r1), (w2, r2)] {
        evidence(&admin, w, r, true).await;
        evidence(&admin, w, shared, true).await;
        edge(&admin, w, shared, r).await;
    }
    let mut handles = Vec::new();
    for i in 0..64 {
        let pool = service.clone();
        handles.push(tokio::spawn(async move {
            let (w, expected) = if i % 2 == 0 { (w1, r1) } else { (w2, r2) };
            let snapshot = load_workspace_graph(&pool, w).await.unwrap();
            assert_eq!(
                snapshot.assess(&[shared]).unwrap().traceable_roots,
                BTreeSet::from([expected])
            );
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.evidence")
        .fetch_one(&service)
        .await
        .unwrap();
    assert_eq!(
        total, 0,
        "transaction-local scope must be cleared on pool reuse"
    );
}

#[tokio::test]
async fn deep_chain_through_database_is_nonrecursive() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let chain: Vec<Uuid> = (0..1200).map(|_| Uuid::new_v4()).collect();
    // Batch inserts, don't do hundreds of sequential round trips.
    let mut qb = QueryBuilder::new(
        "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) ",
    );
    qb.push_values(chain.iter(), |mut q, id| {
        q.push_bind(w)
            .push_bind(id)
            .push_bind("fixture")
            .push_bind("synthetic")
            .push_bind("COPY");
    });
    qb.build().execute(&admin).await.unwrap();
    let mut qb = QueryBuilder::new(
        "INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) ",
    );
    qb.push_values(chain.windows(2), |mut q, link| {
        q.push_bind(w)
            .push_bind(link[1])
            .push_bind(link[0])
            .push_bind("DERIVED_FROM");
    });
    qb.build().execute(&admin).await.unwrap();
    let g = load_workspace_graph(&service, w).await.unwrap();
    assert_eq!(
        g.assess(&[*chain.last().unwrap()]).unwrap().traceable_roots,
        BTreeSet::from([chain[0]])
    );
}

#[tokio::test]
async fn resource_limits_fail_closed_without_partial_graph() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    for id in [a, b, c] {
        evidence(&admin, w, id, true).await;
    }
    edge(&admin, w, b, a).await;
    edge(&admin, w, c, b).await;
    assert!(matches!(
        load_workspace_graph_with_limits(
            &service,
            w,
            GraphLoadLimits {
                max_nodes: 2,
                max_edges: 2
            }
        )
        .await,
        Err(RepositoryError::LimitExceeded {
            kind: "node",
            limit: 2
        })
    ));
    assert!(matches!(
        load_workspace_graph_with_limits(
            &service,
            w,
            GraphLoadLimits {
                max_nodes: 3,
                max_edges: 1
            }
        )
        .await,
        Err(RepositoryError::LimitExceeded {
            kind: "edge",
            limit: 1
        })
    ));
    let graph = load_workspace_graph_with_limits(
        &service,
        w,
        GraphLoadLimits {
            max_nodes: 3,
            max_edges: 2,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        graph.assess(&[c]).unwrap().traceable_roots,
        BTreeSet::from([a])
    );
}

#[tokio::test]
async fn graph_revisions_are_snapshot_consistent_and_bumped_by_external_sql() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let before = load_workspace_graph(&service, w).await.unwrap();
    assert_eq!(before.revision(), 0);
    let a = Uuid::new_v4();
    evidence(&admin, w, a, true).await;
    let newer = load_workspace_graph(&service, w).await.unwrap();
    assert!(newer.revision() > before.revision());
    sqlx::query(
        "UPDATE niccolo.evidence SET admissible=false WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(w)
    .bind(a)
    .execute(&admin)
    .await
    .unwrap();
    let revoked = load_workspace_graph(&service, w).await.unwrap();
    assert!(revoked.revision() > newer.revision());
    assert!(matches!(
        revoked.assess(&[a]),
        Err(RepositoryError::Trace(TraceError::Withdrawn(_)))
    ));
    assert!(newer.assess(&[a]).is_ok());
}
