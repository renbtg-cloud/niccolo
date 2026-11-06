//! Human-readable epistemic read model for the first-party UI.
//! Grouping happens server-side so the browser renders domain state rather than inventing it.
use crate::{analysis_assumptions, analysis_claims, analysis_structure::AnalysisScope, human_agency};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

type FragmentRow = (
    Uuid,
    Uuid,
    String,
    Uuid,
    String,
    String,
    i32,
    String,
    String,
    String,
);
type ContextRow = (Uuid, Uuid, String, String, String, String);

#[derive(Debug, thiserror::Error)]
pub enum EpistemicError {
    #[error("epistemic view dependency failed: {0}")]
    Dependency(String),
}

fn dep<E: std::fmt::Display>(e: E) -> EpistemicError {
    EpistemicError::Dependency(e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceProvenanceLink {
    pub evidence_id: Uuid,
    pub relation: String,
    pub document_id: Uuid,
    pub format: String,
    pub locator: String,
    pub ordinal: i32,
    pub speaker_label: String,
    pub content: String,
    pub source_class: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanContextLink {
    pub contribution_id: Uuid,
    pub relation: String,
    pub kind: String,
    pub text: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpistemicClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
    pub assumptions: Vec<analysis_assumptions::AssumptionView>,
    pub alternatives: Vec<analysis_assumptions::AlternativeView>,
    pub evidence_gaps: Vec<human_agency::EvidenceGapView>,
    pub source_provenance: Vec<SourceProvenanceLink>,
    pub human_context: Vec<HumanContextLink>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpistemicView {
    pub analysis_id: Uuid,
    pub claims: Vec<EpistemicClaim>,
    pub unlinked_assumptions: Vec<analysis_assumptions::AssumptionView>,
    pub unlinked_evidence_gaps: Vec<human_agency::EvidenceGapView>,
    pub claims_are_verified_facts: bool,
    pub assumptions_are_facts: bool,
    pub alternatives_are_automatically_ranked: bool,
    pub satisfied_gap_proves_claim: bool,
    pub browser_is_domain_authority: bool,
    pub claim_fragment_relations_establish_truth: bool,
    pub claim_fragment_link_count_is_independent_evidence_count: bool,
    pub human_context_is_evidence: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, EpistemicError> {
    let mut tx = pool.begin().await.map_err(dep)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(dep)?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(dep)?;
    if privileged {
        return Err(EpistemicError::Dependency(
            "privileged SQL login prohibited".into(),
        ));
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(dep)?;
    crate::authz::bind_auth_digest(&mut tx).await.map_err(dep)?;
    Ok(tx)
}

async fn provenance(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<
    (
        Vec<(Uuid, SourceProvenanceLink)>,
        Vec<(Uuid, HumanContextLink)>,
    ),
    EpistemicError,
> {
    let mut tx = scoped(pool, scope.workspace).await?;
    let fragment_rows: Vec<FragmentRow> = sqlx::query_as(
        "SELECT l.claim_id,l.evidence_id,l.relation,f.document_id,d.format,d.locator,f.ordinal,f.speaker_label,e.content,e.source_class \
         FROM niccolo.situation_analysis_claim_fragments l \
         JOIN niccolo.source_fragments f ON f.workspace_id=l.workspace_id AND f.evidence_id=l.evidence_id \
         JOIN niccolo.source_documents d ON d.workspace_id=f.workspace_id AND d.document_id=f.document_id \
         JOIN niccolo.evidence e ON e.workspace_id=l.workspace_id AND e.evidence_id=l.evidence_id \
         WHERE l.workspace_id=$1 AND l.situation_id=$2 AND l.analysis_id=$3 \
         ORDER BY l.claim_id,d.document_id,f.ordinal,l.relation",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    let context_rows: Vec<ContextRow> = sqlx::query_as(
        "SELECT l.claim_id,l.contribution_id,l.relation,c.kind,c.contribution_text,c.status \
         FROM niccolo.situation_analysis_claim_contributions l \
         JOIN niccolo.situation_contributions c ON c.workspace_id=l.workspace_id AND c.situation_id=l.situation_id AND c.contribution_id=l.contribution_id \
         WHERE l.workspace_id=$1 AND l.situation_id=$2 AND l.analysis_id=$3 \
         ORDER BY l.claim_id,l.contribution_id,l.relation",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    tx.commit().await.map_err(dep)?;
    Ok((
        fragment_rows
            .into_iter()
            .map(
                |(
                    claim_id,
                    evidence_id,
                    relation,
                    document_id,
                    format,
                    locator,
                    ordinal,
                    speaker_label,
                    content,
                    source_class,
                )| {
                    (
                        claim_id,
                        SourceProvenanceLink {
                            evidence_id,
                            relation,
                            document_id,
                            format,
                            locator,
                            ordinal,
                            speaker_label,
                            content,
                            source_class,
                        },
                    )
                },
            )
            .collect(),
        context_rows
            .into_iter()
            .map(
                |(claim_id, contribution_id, relation, kind, text, status)| {
                    (
                        claim_id,
                        HumanContextLink {
                            contribution_id,
                            relation,
                            kind,
                            text,
                            status,
                        },
                    )
                },
            )
            .collect(),
    ))
}

pub async fn get(pool: &PgPool, scope: AnalysisScope) -> Result<EpistemicView, EpistemicError> {
    let claims = analysis_claims::list_claims(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
    )
    .await
    .map_err(dep)?;
    let lens = analysis_assumptions::lens(pool, scope).await.map_err(dep)?;
    let gaps = human_agency::evidence_gaps(pool, scope)
        .await
        .map_err(dep)?;
    let (source_links, context_links) = provenance(pool, scope).await?;

    let grouped = claims
        .into_iter()
        .map(|claim| EpistemicClaim {
            claim_id: claim.claim_id,
            ordinal: claim.ordinal,
            kind: claim.kind,
            text: claim.text,
            qualification: claim.qualification,
            assumptions: lens
                .assumptions
                .iter()
                .filter(|item| item.claim_id == Some(claim.claim_id))
                .cloned()
                .collect(),
            alternatives: lens
                .alternatives
                .iter()
                .filter(|item| item.claim_id == claim.claim_id)
                .cloned()
                .collect(),
            evidence_gaps: gaps
                .iter()
                .filter(|item| item.claim_id == Some(claim.claim_id))
                .cloned()
                .collect(),
            source_provenance: source_links
                .iter()
                .filter(|(claim_id, _)| *claim_id == claim.claim_id)
                .map(|(_, link)| link.clone())
                .collect(),
            human_context: context_links
                .iter()
                .filter(|(claim_id, _)| *claim_id == claim.claim_id)
                .map(|(_, link)| link.clone())
                .collect(),
        })
        .collect();

    Ok(EpistemicView {
        analysis_id: scope.analysis,
        claims: grouped,
        unlinked_assumptions: lens
            .assumptions
            .into_iter()
            .filter(|item| item.claim_id.is_none())
            .collect(),
        unlinked_evidence_gaps: gaps
            .into_iter()
            .filter(|item| item.claim_id.is_none())
            .collect(),
        claims_are_verified_facts: false,
        assumptions_are_facts: lens.assumptions_are_facts,
        alternatives_are_automatically_ranked: lens.alternatives_are_automatically_ranked,
        satisfied_gap_proves_claim: false,
        browser_is_domain_authority: false,
        claim_fragment_relations_establish_truth: false,
        claim_fragment_link_count_is_independent_evidence_count: false,
        human_context_is_evidence: false,
    })
}
