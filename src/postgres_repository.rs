//! Read-only PostgreSQL adapter for the multi-parent evidence graph.
//!
//! WORKSPACE AUTHORIZATION BOUNDARY: `workspace` must be selected from trusted,
//! authenticated server context. This module does not authenticate a caller.
//! The pool MUST use a non-superuser, non-BYPASSRLS role granted `niccolo_app`.
//! It deliberately rejects privileged DB connections. All reads happen in one
//! REPEATABLE READ, READ ONLY transaction with a transaction-local RLS setting.
//! No SQL result is interpreted as evidence of factual truth or witness independence.
use crate::EvidenceId;
use crate::provenance_dag::{CommonOrigin, SourceAssessment, SourceGraph, SourceNode, TraceError};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("PostgreSQL error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("the database role bypasses row-level security")]
    PrivilegedRole,
    #[error("workspace graph exceeds configured {kind} limit ({limit})")]
    LimitExceeded { kind: &'static str, limit: i64 },
    #[error("graph database is inconsistent: {0}")]
    Inconsistent(String),
    #[error("report {0} is not visible in this workspace")]
    InvisibleReport(Uuid),
    #[error("provenance trace failed: {0:?}")]
    Trace(TraceError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedOrigins {
    pub left_report_index: usize,
    pub right_report_index: usize,
    pub origins: BTreeSet<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAssessment {
    pub report_count: usize,
    /// A shared uploaded file is a source-context overlap, not evidence of
    /// copied testimony, truth, or coordinated behavior.
    pub document_ids_per_report: Vec<BTreeSet<Uuid>>,
    pub shared_source_documents: Vec<SharedDocuments>,
    pub roots_per_report: Vec<BTreeSet<Uuid>>,
    pub traceable_roots: BTreeSet<Uuid>,
    pub shared_origins: Vec<SharedOrigins>,
    /// Distinct roots do not establish statistical independence.
    pub independence_established: bool,
}

/// Detached, immutable snapshot. Do not cache beyond an authorization decision
/// or pretend it reflects revocations committed after it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedDocuments {
    pub left_report_index: usize,
    pub right_report_index: usize,
    pub documents: BTreeSet<Uuid>,
}

pub struct WorkspaceGraphSnapshot {
    graph: SourceGraph,
    evidence_documents: BTreeMap<Uuid, BTreeSet<Uuid>>,
    ids: BTreeMap<Uuid, EvidenceId>,
    reverse: Vec<Uuid>,
    revision: i64,
}

impl WorkspaceGraphSnapshot {
    /// Transaction-consistent graph revision. Compare against a newly loaded
    /// snapshot before acting; detached snapshots are never live subscriptions.
    pub fn node_count(&self) -> usize {
        self.ids.len()
    }
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn revision(&self) -> i64 {
        self.revision
    }

    fn id(&self, uuid: Uuid) -> Result<EvidenceId, RepositoryError> {
        self.ids
            .get(&uuid)
            .copied()
            .ok_or(RepositoryError::InvisibleReport(uuid))
    }

    fn uuids(&self, ids: &BTreeSet<EvidenceId>) -> Result<BTreeSet<Uuid>, RepositoryError> {
        ids.iter()
            .map(|id| {
                self.reverse
                    .get((id.0 - 1) as usize)
                    .copied()
                    .ok_or_else(|| {
                        RepositoryError::Inconsistent(format!("invalid graph id {}", id.0))
                    })
            })
            .collect()
    }

    pub fn assess(&self, reports: &[Uuid]) -> Result<WorkspaceAssessment, RepositoryError> {
        let ids = reports
            .iter()
            .copied()
            .map(|id| self.id(id))
            .collect::<Result<Vec<_>, _>>()?;
        let SourceAssessment {
            report_count,
            traceable_roots,
            roots_per_report,
            shared_origins,
            independence_established,
        } = self.graph.assess(&ids).map_err(RepositoryError::Trace)?;
        let roots_per_report = roots_per_report
            .iter()
            .map(|roots| self.uuids(roots))
            .collect::<Result<Vec<BTreeSet<Uuid>>, _>>()?;
        let document_ids_per_report: Vec<BTreeSet<Uuid>> = roots_per_report
            .iter()
            .map(|roots| {
                roots
                    .iter()
                    .flat_map(|origin| {
                        self.evidence_documents
                            .get(origin)
                            .into_iter()
                            .flat_map(|docs| docs.iter().copied())
                    })
                    .collect()
            })
            .collect();
        let mut shared_source_documents = Vec::new();
        for left in 0..document_ids_per_report.len() {
            for right in left + 1..document_ids_per_report.len() {
                let documents: BTreeSet<Uuid> = document_ids_per_report[left]
                    .intersection(&document_ids_per_report[right])
                    .copied()
                    .collect();
                if !documents.is_empty() {
                    shared_source_documents.push(SharedDocuments {
                        left_report_index: left,
                        right_report_index: right,
                        documents,
                    });
                }
            }
        }
        let shared_origins = shared_origins
            .iter()
            .map(
                |CommonOrigin {
                     left_report_index,
                     right_report_index,
                     origins,
                 }| {
                    Ok(SharedOrigins {
                        left_report_index: *left_report_index,
                        right_report_index: *right_report_index,
                        origins: self.uuids(origins)?,
                    })
                },
            )
            .collect::<Result<_, RepositoryError>>()?;
        Ok(WorkspaceAssessment {
            report_count,
            document_ids_per_report,
            shared_source_documents,
            roots_per_report,
            traceable_roots: self.uuids(&traceable_roots)?,
            shared_origins,
            independence_established,
        })
    }
}

/// The maximum number of records the prototype will load from a single workspace.
/// These are resource safeguards, not pagination or a substitute for query planning.
#[derive(Clone, Copy, Debug)]
pub struct GraphLoadLimits {
    pub max_nodes: i64,
    pub max_edges: i64,
}
impl Default for GraphLoadLimits {
    fn default() -> Self {
        Self {
            max_nodes: 50_000,
            max_edges: 150_000,
        }
    }
}

/// Read all visible evidence and derivations in a single consistent snapshot.
/// The caller must supply an *authorized* workspace id, never a raw client claim.
/// A missing, cyclic, or withdrawn parent fails closed at assessment time.
/// The adapter currently loads the whole workspace: it is a bounded POC, not
/// a scalable incremental/streaming graph implementation.
pub async fn load_workspace_graph(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<WorkspaceGraphSnapshot, RepositoryError> {
    load_workspace_graph_with_limits(pool, workspace, GraphLoadLimits::default()).await
}

/// Restrict memory use and fail closed instead of returning a partial graph.
pub async fn load_workspace_graph_with_limits(
    pool: &PgPool,
    workspace: Uuid,
    limits: GraphLoadLimits,
) -> Result<WorkspaceGraphSnapshot, RepositoryError> {
    if limits.max_nodes < 1
        || limits.max_edges < 0
        || limits.max_nodes == i64::MAX
        || limits.max_edges == i64::MAX
    {
        return Err(RepositoryError::Inconsistent("invalid graph limits".into()));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(RepositoryError::PrivilegedRole);
    }
    // LOCAL is automatically discarded by transaction commit/rollback.
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id', $1, true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    let evidence: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT evidence_id, admissible FROM niccolo.evidence ORDER BY evidence_id LIMIT $1",
    )
    .bind(limits.max_nodes + 1)
    .fetch_all(&mut *tx)
    .await?;
    if evidence.len() as i64 > limits.max_nodes {
        return Err(RepositoryError::LimitExceeded {
            kind: "node",
            limit: limits.max_nodes,
        });
    }
    let edges: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT child_id, parent_id FROM niccolo.evidence_derivation ORDER BY child_id, parent_id LIMIT $1",
    )
    .bind(limits.max_edges + 1)
    .fetch_all(&mut *tx)
    .await?;
    if edges.len() as i64 > limits.max_edges {
        return Err(RepositoryError::LimitExceeded {
            kind: "edge",
            limit: limits.max_edges,
        });
    }
    // At most one fragment per evidence id (UNIQUE constraint in migration 0003).
    let source_rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT evidence_id, document_id FROM niccolo.source_fragments ORDER BY evidence_id LIMIT $1"
    ).bind(limits.max_nodes + 1).fetch_all(&mut *tx).await?;
    if source_rows.len() as i64 > limits.max_nodes {
        return Err(RepositoryError::LimitExceeded {
            kind: "source fragment",
            limit: limits.max_nodes,
        });
    }
    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM niccolo.graph_revisions WHERE workspace_id = $1")
            .bind(workspace)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
    tx.commit().await?;

    let mut evidence_documents: BTreeMap<Uuid, BTreeSet<Uuid>> = BTreeMap::new();
    for (evidence_id, document_id) in source_rows {
        evidence_documents
            .entry(evidence_id)
            .or_default()
            .insert(document_id);
    }
    let mut ids = BTreeMap::new();
    let mut reverse = Vec::with_capacity(evidence.len());
    for (uuid, _) in &evidence {
        let id = u64::try_from(reverse.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| RepositoryError::Inconsistent("too many evidence nodes".into()))?;
        ids.insert(*uuid, EvidenceId(id));
        reverse.push(*uuid);
    }
    let mut parents: BTreeMap<EvidenceId, BTreeSet<EvidenceId>> = BTreeMap::new();
    for (child, parent) in edges {
        let c = ids
            .get(&child)
            .ok_or_else(|| RepositoryError::Inconsistent(format!("invisible child {child}")))?;
        let p = ids
            .get(&parent)
            .ok_or_else(|| RepositoryError::Inconsistent(format!("invisible parent {parent}")))?;
        parents.entry(*c).or_default().insert(*p);
    }
    let mut graph = SourceGraph::default();
    for (uuid, admissible) in evidence {
        let id = ids[&uuid];
        let node = SourceNode {
            id,
            parents: parents.remove(&id).unwrap_or_default(),
            admissible,
        };
        graph
            .insert(node)
            .map_err(|err| RepositoryError::Inconsistent(format!("{err:?}")))?;
    }
    Ok(WorkspaceGraphSnapshot {
        graph,
        evidence_documents,
        ids,
        reverse,
        revision,
    })
}
