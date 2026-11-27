//! v0.351-v0.400: canonical reasoning packet for model-facing steps.
//! Frozen analysis input is separated from current human control so later priorities/constraints
//! can govern present execution without being misrepresented as historical analysis input.
use crate::analysis_structure::AnalysisScope;
use crate::{
    actors, agentic_policy, analysis_assumptions, analysis_claims, analysis_snapshot,
    analysis_workbench_support,
    clarification, collection_provenance, decision_points, decision_readiness,
    epistemic_policy_store, human_agency, human_control, human_credence, hypothesis_portfolio,
    integrity_scope, observation_query_ledger, proactive_briefing, source_scope, temporal_state,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PacketError {
    #[error("reasoning packet dependency failed: {0}")]
    Dependency(String),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenContribution {
    pub id: Uuid,
    pub author: Uuid,
    pub kind: String,
    pub text: String,
    pub scope: String,
    pub related_evidence_id: Option<Uuid>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenIdentityBinding {
    pub document_id: Uuid,
    pub speaker_label: String,
    pub action: String,
    pub person_id: Option<Uuid>,
    pub display_label: Option<String>,
    pub event_id: Uuid,
    pub event_no: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenSourceFragment {
    pub document_id: Uuid,
    pub evidence_id: Uuid,
    pub ordinal: i32,
    pub format: String,
    pub locator: String,
    pub speaker_label: String,
    pub content: String,
    pub source_class: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenArtifact {
    pub artifact_id: Uuid,
    pub artifact_type: String,
    pub label: String,
    pub declared_mime_type: Option<String>,
    pub declared_byte_length: Option<i64>,
    pub declared_sha256: Option<String>,
    pub source_bytes_included: bool,
    pub artifact_was_inspected: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenArtifactStorageReference {
    pub artifact_id: Uuid,
    pub storage_reference_id: Uuid,
    pub storage_type: String,
    pub locator: String,
    pub access_status: String,
    pub access_status_basis: String,
    pub locator_was_dereferenced: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenArtifactRepresentation {
    pub artifact_id: Uuid,
    pub representation_id: Uuid,
    pub representation_type: String,
    pub producer_type: String,
    pub producer_label: Option<String>,
    pub production_method: Option<String>,
    pub scope_locator: String,
    pub content: String,
    pub completeness_status: String,
    pub epistemic_qualification: Option<String>,
    pub limitations: Option<String>,
    pub content_was_explicitly_supplied: bool,
    pub artifact_was_parsed: bool,
    pub representation_establishes_truth: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenArtifactRelationship {
    pub relationship_id: Uuid,
    pub subject_artifact_id: Uuid,
    pub object_artifact_id: Uuid,
    pub relationship_type: String,
    pub qualification: Option<String>,
    pub relationship_is_verified: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenInputPacket {
    pub revision: i64,
    pub fingerprint: String,
    pub focus: String,
    pub goal: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbench_support: Option<analysis_workbench_support::Basis>,
    pub sources: Vec<Uuid>,
    pub identity_event_no_max: i64,
    pub identity_bindings: Vec<FrozenIdentityBinding>,
    pub actor_kind_event_no_max: i64,
    pub actor_binding_event_no_max: i64,
    pub actor_bindings: Vec<actors::FrozenActorBinding>,
    pub identity_bindings_are_human_confirmed: bool,
    pub matching_speaker_labels_imply_same_person: bool,
    pub actor_kind_is_investigation_gate: bool,
    pub source_fragments: Vec<FrozenSourceFragment>,
    pub source_fragment_count_total: i64,
    pub source_fragment_count_included: usize,
    pub source_bytes_included: usize,
    pub source_packet_truncated: bool,
    pub artifacts: Vec<FrozenArtifact>,
    pub artifact_storage_references: Vec<FrozenArtifactStorageReference>,
    pub artifact_representations: Vec<FrozenArtifactRepresentation>,
    pub artifact_relationships: Vec<FrozenArtifactRelationship>,
    pub artifact_representation_count_total: i64,
    pub artifact_representation_count_included: usize,
    pub artifact_representation_bytes_included: usize,
    pub artifact_packet_truncated: bool,
    pub artifact_original_bytes_included: usize,
    pub contributions: Vec<FrozenContribution>,
    pub input_is_current: bool,
    pub current_revision: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentHumanControl {
    pub goals: Vec<human_agency::GoalView>,
    pub priorities: Vec<human_control::PriorityView>,
    pub constraints: Vec<human_control::ConstraintView>,
    pub proposed_next_steps: Vec<human_agency::NextStepView>,
    /// Current human judgments, not part of the frozen analysis input.
    pub claim_credence: Vec<human_credence::HumanCredenceView>,
    /// Current user-configured heuristics; not retroactive documentary evidence.
    pub epistemic_policy: epistemic_policy_store::PolicyView,
    pub current_control_was_part_of_frozen_input: bool,
    pub system_owns_project: bool,
    pub system_may_execute_proposed_steps_without_human: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PacketClarification {
    pub question_id: Uuid,
    pub text: String,
    pub reason: String,
    pub input_revision: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningPacket {
    pub format: String,
    pub version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub frozen_input: FrozenInputPacket,
    pub claims: Vec<PacketClaim>,
    pub candidate_interpretations: Vec<analysis_assumptions::CandidateInterpretationView>,
    pub hypothesis_portfolio: hypothesis_portfolio::HypothesisPortfolio,
    pub temporal_assertions: Vec<temporal_state::TemporalAssertion>,
    pub source_scope: Vec<source_scope::SourceScopeAssertion>,
    pub collection_events: Vec<collection_provenance::CollectionEvent>,
    pub integrity_assessments: Vec<integrity_scope::IntegrityAssessment>,
    pub decisions: decision_points::DecisionPackage,
    pub decision_readiness: Vec<decision_readiness::DecisionReadinessView>,
    pub lens: analysis_assumptions::AnalysisLens,
    pub evidence_gaps: Vec<human_agency::EvidenceGapView>,
    pub clarifications: Vec<PacketClarification>,
    pub clarifications_truncated: bool,
    pub policy_signals: PolicySignalPacket,
    pub proactive_leads: proactive_briefing::ProactiveBriefing,
    /// Model-requested searches, grounded in the frozen revision; not facts.
    pub observation_queries: Vec<observation_query_ledger::QueryTrace>,
    pub observation_query_packet_truncated: bool,
    pub current_human_control: CurrentHumanControl,
    pub packet_asserts_truth: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySignalPacket {
    pub open_evidence_gaps: i64,
    pub conflict_sets: i64,
    pub triggered_stop_conditions: i64,
    pub source_requests: i64,
}

fn dep<E: std::fmt::Display>(e: E) -> PacketError {
    PacketError::Dependency(e.to_string())
}
const MAX_SOURCE_FRAGMENTS: i64 = 256;
const MAX_SOURCE_BYTES: usize = 384 * 1024;
const MAX_ARTIFACT_REPRESENTATIONS: i64 = 256;
const MAX_ARTIFACT_REPRESENTATION_BYTES: usize = 384 * 1024;

type FrozenFragmentRow = (Uuid, Uuid, i32, String, String, String, String, String);
type FrozenArtifactRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
);
type FrozenArtifactStorageRow = (Uuid, Uuid, String, String, String, String);
type FrozenArtifactRepresentationRow = (
    Uuid,
    Uuid,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);
type FrozenArtifactRelationshipRow = (Uuid, Uuid, Uuid, String, Option<String>);

async fn packet_tx(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, PacketError> {
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
        return Err(PacketError::Dependency(
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

type FrozenIdentityRow = (
    Uuid,
    String,
    String,
    Option<Uuid>,
    Option<String>,
    Uuid,
    i64,
);

async fn frozen_identity_bindings(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    revision: i64,
    identity_event_no_max: i64,
) -> Result<Vec<FrozenIdentityBinding>, PacketError> {
    let mut tx = packet_tx(pool, workspace).await?;
    let rows: Vec<FrozenIdentityRow> = sqlx::query_as(
        "SELECT DISTINCT ON (e.document_id,e.speaker_label) \
         e.document_id,e.speaker_label,e.operation AS action,e.person_id,s.display_label,e.event_id,e.event_no \
         FROM niccolo.situation_revision_sources rs \
         JOIN niccolo.identity_events e ON e.workspace_id=rs.workspace_id AND e.document_id=rs.document_id \
         LEFT JOIN niccolo.identity_subjects s ON s.workspace_id=e.workspace_id AND s.person_id=e.person_id \
         WHERE rs.workspace_id=$1 AND rs.situation_id=$2 AND rs.revision=$3 AND e.event_no <= $4 \
         ORDER BY e.document_id,e.speaker_label,e.event_no DESC",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .bind(identity_event_no_max)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    tx.commit().await.map_err(dep)?;
    Ok(rows
        .into_iter()
        .map(
            |(document_id, speaker_label, action, person_id, display_label, event_id, event_no)| {
                FrozenIdentityBinding {
                    document_id,
                    speaker_label,
                    action,
                    person_id,
                    display_label,
                    event_id,
                    event_no,
                }
            },
        )
        .collect())
}

async fn frozen_source_fragments(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    revision: i64,
) -> Result<(Vec<FrozenSourceFragment>, i64, usize, bool), PacketError> {
    let mut tx = packet_tx(pool, workspace).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint \
         FROM niccolo.situation_revision_sources rs \
         JOIN niccolo.source_fragments f ON f.workspace_id=rs.workspace_id AND f.document_id=rs.document_id \
         WHERE rs.workspace_id=$1 AND rs.situation_id=$2 AND rs.revision=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_one(&mut *tx)
    .await
    .map_err(dep)?;
    let rows: Vec<FrozenFragmentRow> = sqlx::query_as(
        "SELECT d.document_id,f.evidence_id,f.ordinal,d.format,d.locator,f.speaker_label,e.content,e.source_class \
         FROM niccolo.situation_revision_sources rs \
         JOIN niccolo.source_documents d ON d.workspace_id=rs.workspace_id AND d.document_id=rs.document_id \
         JOIN niccolo.source_fragments f ON f.workspace_id=d.workspace_id AND f.document_id=d.document_id \
         JOIN niccolo.evidence e ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id \
         WHERE rs.workspace_id=$1 AND rs.situation_id=$2 AND rs.revision=$3 \
         ORDER BY d.document_id,f.ordinal \
         LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .bind(MAX_SOURCE_FRAGMENTS + 1)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    tx.commit().await.map_err(dep)?;

    let mut fragments = Vec::new();
    let mut bytes = 0usize;
    let mut truncated = total > MAX_SOURCE_FRAGMENTS;
    for (
        document_id,
        evidence_id,
        ordinal,
        format,
        locator,
        speaker_label,
        content,
        source_class,
    ) in rows
    {
        if fragments.len() >= MAX_SOURCE_FRAGMENTS as usize {
            truncated = true;
            break;
        }
        let next = bytes.saturating_add(content.len());
        if next > MAX_SOURCE_BYTES {
            truncated = true;
            break;
        }
        bytes = next;
        fragments.push(FrozenSourceFragment {
            document_id,
            evidence_id,
            ordinal,
            format,
            locator,
            speaker_label,
            content,
            source_class,
        });
    }
    if fragments.len() as i64 != total {
        truncated = true;
    }
    Ok((fragments, total, bytes, truncated))
}

async fn frozen_artifact_material(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    revision: i64,
) -> Result<
    (
        Vec<FrozenArtifact>,
        Vec<FrozenArtifactStorageReference>,
        Vec<FrozenArtifactRepresentation>,
        Vec<FrozenArtifactRelationship>,
        i64,
        usize,
        bool,
    ),
    PacketError,
> {
    let mut tx = packet_tx(pool, workspace).await?;
    let artifacts: Vec<FrozenArtifactRow> = sqlx::query_as(
        "SELECT a.artifact_id,a.artifact_type,a.label,a.declared_mime_type,
                a.declared_byte_length,a.declared_sha256
         FROM niccolo.situation_revision_artifacts ra
         JOIN niccolo.artifacts a USING(workspace_id,artifact_id)
         WHERE ra.workspace_id=$1 AND ra.situation_id=$2 AND ra.revision=$3
         ORDER BY a.artifact_id LIMIT 1001",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    let storage: Vec<FrozenArtifactStorageRow> = sqlx::query_as(
        "SELECT rr.artifact_id,rr.storage_reference_id,r.storage_type,r.locator,
                e.access_status,e.status_basis
         FROM niccolo.situation_revision_artifact_storage_references rr
         JOIN niccolo.artifact_storage_references r
           ON r.workspace_id=rr.workspace_id AND r.artifact_id=rr.artifact_id
          AND r.storage_reference_id=rr.storage_reference_id
         JOIN niccolo.artifact_storage_access_events e
           ON e.workspace_id=rr.workspace_id AND e.artifact_id=rr.artifact_id
          AND e.storage_reference_id=rr.storage_reference_id
          AND e.access_event_id=rr.access_event_id
         WHERE rr.workspace_id=$1 AND rr.situation_id=$2 AND rr.revision=$3
         ORDER BY rr.artifact_id,rr.storage_reference_id LIMIT 1001",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    let representation_total: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
         FROM niccolo.situation_revision_artifact_representations
         WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_one(&mut *tx)
    .await
    .map_err(dep)?;
    let representation_rows: Vec<FrozenArtifactRepresentationRow> = sqlx::query_as(
        "SELECT r.artifact_id,r.representation_id,r.representation_type,r.producer_type,
                r.producer_label,r.production_method,r.scope_locator,r.content,
                r.completeness_status,r.epistemic_qualification,r.limitations
         FROM niccolo.situation_revision_artifact_representations rr
         JOIN niccolo.situation_artifact_representations r
           ON r.workspace_id=rr.workspace_id AND r.situation_id=rr.situation_id
          AND r.representation_id=rr.representation_id
         WHERE rr.workspace_id=$1 AND rr.situation_id=$2 AND rr.revision=$3
         ORDER BY r.representation_id LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .bind(MAX_ARTIFACT_REPRESENTATIONS + 1)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    let relationships: Vec<FrozenArtifactRelationshipRow> = sqlx::query_as(
        "SELECT r.relationship_id,r.subject_artifact_id,r.object_artifact_id,
                r.relationship_type,r.qualification
         FROM niccolo.situation_revision_artifact_relationships rr
         JOIN niccolo.situation_artifact_relationships r
           ON r.workspace_id=rr.workspace_id AND r.situation_id=rr.situation_id
          AND r.relationship_id=rr.relationship_id
         WHERE rr.workspace_id=$1 AND rr.situation_id=$2 AND rr.revision=$3
         ORDER BY r.relationship_id LIMIT 1001",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_all(&mut *tx)
    .await
    .map_err(dep)?;
    tx.commit().await.map_err(dep)?;
    let mut packet_truncated =
        artifacts.len() > 1000 || storage.len() > 1000 || relationships.len() > 1000;
    let artifacts = artifacts
        .into_iter()
        .take(1000)
        .map(
            |(
                artifact_id,
                artifact_type,
                label,
                declared_mime_type,
                declared_byte_length,
                declared_sha256,
            )| FrozenArtifact {
                artifact_id,
                artifact_type,
                label,
                declared_mime_type,
                declared_byte_length,
                declared_sha256,
                source_bytes_included: false,
                artifact_was_inspected: false,
            },
        )
        .collect();
    let storage = storage
        .into_iter()
        .take(1000)
        .map(
            |(
                artifact_id,
                storage_reference_id,
                storage_type,
                locator,
                access_status,
                access_status_basis,
            )| FrozenArtifactStorageReference {
                artifact_id,
                storage_reference_id,
                storage_type,
                locator,
                access_status,
                access_status_basis,
                locator_was_dereferenced: false,
            },
        )
        .collect();
    let relationships = relationships
        .into_iter()
        .take(1000)
        .map(
            |(
                relationship_id,
                subject_artifact_id,
                object_artifact_id,
                relationship_type,
                qualification,
            )| FrozenArtifactRelationship {
                relationship_id,
                subject_artifact_id,
                object_artifact_id,
                relationship_type,
                qualification,
                relationship_is_verified: false,
            },
        )
        .collect();
    let mut representation_bytes = 0usize;
    let mut representations = Vec::new();
    for (
        artifact_id,
        representation_id,
        representation_type,
        producer_type,
        producer_label,
        production_method,
        scope_locator,
        content,
        completeness_status,
        epistemic_qualification,
        limitations,
    ) in representation_rows
    {
        if representations.len() >= MAX_ARTIFACT_REPRESENTATIONS as usize {
            packet_truncated = true;
            break;
        }
        let next = representation_bytes.saturating_add(content.len());
        if next > MAX_ARTIFACT_REPRESENTATION_BYTES {
            packet_truncated = true;
            break;
        }
        representation_bytes = next;
        representations.push(FrozenArtifactRepresentation {
            artifact_id,
            representation_id,
            representation_type,
            producer_type,
            producer_label,
            production_method,
            scope_locator,
            content,
            completeness_status,
            epistemic_qualification,
            limitations,
            content_was_explicitly_supplied: true,
            artifact_was_parsed: false,
            representation_establishes_truth: false,
        });
    }
    if representations.len() as i64 != representation_total {
        packet_truncated = true;
    }
    Ok((
        artifacts,
        storage,
        representations,
        relationships,
        representation_total,
        representation_bytes,
        packet_truncated,
    ))
}

pub async fn build(pool: &PgPool, scope: AnalysisScope) -> Result<ReasoningPacket, PacketError> {
    let frozen =
        analysis_snapshot::materialize(pool, scope.workspace, scope.situation, scope.analysis)
            .await
            .map_err(dep)?;
    let mut support_tx = packet_tx(pool, scope.workspace).await?;
    let workbench_support = analysis_workbench_support::read_in(&mut support_tx, scope)
        .await
        .map_err(dep)?;
    support_tx.commit().await.map_err(dep)?;
    let (
        source_fragments,
        source_fragment_count_total,
        source_bytes_included,
        source_packet_truncated,
    ) = frozen_source_fragments(
        pool,
        scope.workspace,
        scope.situation,
        frozen.input.revision,
    )
    .await?;
    let (
        artifacts,
        artifact_storage_references,
        artifact_representations,
        artifact_relationships,
        artifact_representation_count_total,
        artifact_representation_bytes_included,
        artifact_packet_truncated,
    ) = frozen_artifact_material(
        pool,
        scope.workspace,
        scope.situation,
        frozen.input.revision,
    )
    .await?;
    let identity_event_no_max = frozen.analysis.identity_event_no_max;
    let identity_bindings = frozen_identity_bindings(
        pool,
        scope.workspace,
        scope.situation,
        frozen.input.revision,
        identity_event_no_max,
    )
    .await?;
    let actor_kind_event_no_max = frozen.analysis.actor_kind_event_no_max;
    let actor_binding_event_no_max = frozen.analysis.actor_binding_event_no_max;
    let actor_bindings = actors::frozen_bindings(
        pool,
        scope.workspace,
        scope.situation,
        frozen.input.revision,
        actor_kind_event_no_max,
        actor_binding_event_no_max,
    )
    .await
    .map_err(dep)?;
    let claims =
        analysis_claims::list_claims(pool, scope.workspace, scope.situation, scope.analysis)
            .await
            .map_err(dep)?;
    let candidate_interpretations = analysis_assumptions::candidate_interpretations(pool, scope)
        .await
        .map_err(dep)?;
    let hypothesis_portfolio = hypothesis_portfolio::portfolio(pool, scope)
        .await
        .map_err(dep)?;
    let temporal_assertions = temporal_state::list(pool, scope).await.map_err(dep)?;
    let source_scope = source_scope::list(pool, scope).await.map_err(dep)?;
    let collection_events = collection_provenance::list(pool, scope)
        .await
        .map_err(dep)?;
    let integrity_assessments = integrity_scope::list(pool, scope).await.map_err(dep)?;
    let decisions = decision_points::package(pool, scope).await.map_err(dep)?;
    let decision_readiness = decision_readiness::latest(pool, scope).await.map_err(dep)?;
    let lens = analysis_assumptions::lens(pool, scope).await.map_err(dep)?;
    let evidence_gaps = human_agency::evidence_gaps(pool, scope)
        .await
        .map_err(dep)?;
    let (clarifications, next_clarification) = clarification::list(
        pool,
        scope.workspace,
        scope.situation,
        scope.analysis,
        None,
        100,
    )
    .await
    .map_err(dep)?;
    let clarifications = clarifications
        .into_iter()
        .map(|q| PacketClarification {
            question_id: q.question_id,
            text: q.text,
            reason: q.reason,
            input_revision: q.input_revision,
        })
        .collect();
    let goals = human_agency::goals(pool, scope.workspace, scope.situation)
        .await
        .map_err(dep)?;
    let proposed_next_steps = human_agency::next_steps(pool, scope)
        .await
        .map_err(dep)?
        .steps;
    let claim_credence = human_credence::list_for_analysis(pool, scope)
        .await
        .map_err(dep)?;
    let epistemic_policy =
        epistemic_policy_store::effective(pool, scope.workspace, Some(scope.situation))
            .await
            .map_err(dep)?;
    let priorities = human_control::priorities(pool, scope.workspace, scope.situation)
        .await
        .map_err(dep)?;
    let constraints = human_control::constraints(pool, scope.workspace, scope.situation)
        .await
        .map_err(dep)?;
    let signals = agentic_policy::signals(pool, scope.workspace, scope.situation, scope.analysis)
        .await
        .map_err(dep)?;
    let proactive_leads = proactive_briefing::build(pool, scope).await.map_err(dep)?;
    let (observation_queries, observation_query_packet_truncated) =
        observation_query_ledger::for_packet(pool, scope)
            .await
            .map_err(dep)?;
    Ok(ReasoningPacket {
        format: "niccolo.reasoning-packet".into(),
        version: 2,
        workspace_id: scope.workspace,
        situation_id: scope.situation,
        analysis_id: scope.analysis,
        frozen_input: FrozenInputPacket {
            revision: frozen.input.revision,
            fingerprint: frozen.input.fingerprint,
            focus: frozen.input.focus,
            goal: frozen.analysis.goal,
            workbench_support,
            sources: frozen.input.sources,
            identity_event_no_max,
            identity_bindings,
            actor_kind_event_no_max,
            actor_binding_event_no_max,
            actor_bindings,
            identity_bindings_are_human_confirmed: true,
            matching_speaker_labels_imply_same_person: false,
            actor_kind_is_investigation_gate: false,
            source_fragment_count_included: source_fragments.len(),
            source_fragments,
            source_fragment_count_total,
            source_bytes_included,
            source_packet_truncated,
            artifacts,
            artifact_storage_references,
            artifact_representation_count_included: artifact_representations.len(),
            artifact_representations,
            artifact_relationships,
            artifact_representation_count_total,
            artifact_representation_bytes_included,
            artifact_packet_truncated,
            artifact_original_bytes_included: 0,
            contributions: frozen
                .input
                .contributions
                .into_iter()
                .map(|c| FrozenContribution {
                    id: c.id,
                    author: c.author,
                    kind: c.kind,
                    text: c.text,
                    scope: c.scope,
                    related_evidence_id: c.related_evidence_id,
                })
                .collect(),
            input_is_current: frozen.input_is_current,
            current_revision: frozen.current_revision,
        },
        claims: claims
            .into_iter()
            .map(|c| PacketClaim {
                claim_id: c.claim_id,
                ordinal: c.ordinal,
                kind: c.kind,
                text: c.text,
                qualification: c.qualification,
            })
            .collect(),
        candidate_interpretations,
        hypothesis_portfolio,
        temporal_assertions,
        source_scope,
        collection_events,
        integrity_assessments,
        decisions,
        decision_readiness,
        lens,
        evidence_gaps,
        clarifications,
        clarifications_truncated: next_clarification.is_some(),
        policy_signals: PolicySignalPacket {
            open_evidence_gaps: signals.open_evidence_gaps,
            conflict_sets: signals.conflict_sets,
            triggered_stop_conditions: signals.triggered_stop_conditions,
            source_requests: signals.source_requests,
        },
        proactive_leads,
        observation_queries,
        observation_query_packet_truncated,
        current_human_control: CurrentHumanControl {
            goals,
            priorities,
            constraints,
            proposed_next_steps,
            claim_credence,
            epistemic_policy,
            current_control_was_part_of_frozen_input: false,
            system_owns_project: false,
            system_may_execute_proposed_steps_without_human: false,
        },
        packet_asserts_truth: false,
    })
}
pub fn canonical_json(packet: &ReasoningPacket) -> Result<String, PacketError> {
    Ok(serde_json::to_string(packet)?)
}
pub fn sha256(packet: &ReasoningPacket) -> Result<String, PacketError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_json(packet)?.as_bytes())
    ))
}
