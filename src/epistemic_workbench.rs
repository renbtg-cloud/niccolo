//! Expert-authored epistemic workbench for the structured MVP.
//!
//! Investigator-approved abstractions and relationships live here. Model-assisted
//! interpretation runs separately and produces a draft for the investigator.
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[path = "epistemic_assessment.rs"]
pub mod assessment;
#[path = "epistemic_inquiry_response.rs"]
pub mod inquiry_response;
#[path = "epistemic_inquiry_decision.rs"]
pub mod inquiry_decision;
#[path = "epistemic_inquiry_decision_review.rs"]
pub mod inquiry_decision_review;
#[path = "epistemic_action_recommendation.rs"]
pub mod action_recommendation;
#[path = "epistemic_action_decision.rs"]
pub mod action_decision;
#[path = "epistemic_intervention.rs"]
pub mod intervention;
#[path = "epistemic_actor_state.rs"]
pub mod actor_state;
#[path = "epistemic_normative_assessment.rs"]
pub mod normative_assessment;
#[path = "epistemic_actor_act.rs"]
pub mod actor_act;
#[path = "epistemic_interpretation_dependency.rs"]
pub mod interpretation_dependency;

#[derive(Debug, thiserror::Error)]
pub enum WorkbenchError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid expert-workbench request")]
    Invalid,
    #[error("epistemic object is not visible in this Situation")]
    Invisible,
    #[error("epistemic authoring replay conflict")]
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpertNodeKind {
    Proposition,
    Hypothesis,
    Assumption,
    InformationGap,
    Question,
    Finding,
}
impl ExpertNodeKind {
    pub(crate) fn db_value(self) -> &'static str {
        match self {
            Self::Proposition => "PROPOSITION",
            Self::Hypothesis => "HYPOTHESIS",
            Self::Assumption => "ASSUMPTION",
            Self::InformationGap => "INFORMATION_GAP",
            Self::Question => "QUESTION",
            Self::Finding => "FINDING",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpertNode {
    pub object_id: Uuid,
    pub kind: ExpertNodeKind,
    pub situation_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub created_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactProvenance {
    pub object_id: Uuid,
    pub artifact_id: Uuid,
    pub representation_id: Uuid,
    pub artifact_label: String,
    pub scope_locator: String,
    pub qualification: Option<String>,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationFamily {
    Semantic,
    Genealogy,
    Contextual,
    Causal,
}
impl RelationFamily {
    fn db_value(self) -> &'static str {
        match self {
            Self::Semantic => "SEMANTIC",
            Self::Genealogy => "GENEALOGY",
            Self::Contextual => "CONTEXTUAL",
            Self::Causal => "CAUSAL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationKind {
    EquivalentTo,
    MoreSpecificThan,
    Entails,
    Contradicts,
    PartialOverlap,
    Refines,
    Supersedes,
    Retracts,
    Consolidates,
    Separates,
    Reinterprets,
    RelevantTo,
    ApplicableUnder,
    AddressesGap,
    TestsHypothesis,
    Presupposes,
    Causes,
    ContributesTo,
    Enables,
    Prevents,
    Intensifies,
    Attenuates,
}
impl RelationKind {
    fn db_value(self) -> &'static str {
        match self {
            Self::EquivalentTo => "EQUIVALENT_TO",
            Self::MoreSpecificThan => "MORE_SPECIFIC_THAN",
            Self::Entails => "ENTAILS",
            Self::Contradicts => "CONTRADICTS",
            Self::PartialOverlap => "PARTIAL_OVERLAP",
            Self::Refines => "REFINES",
            Self::Supersedes => "SUPERSEDES",
            Self::Retracts => "RETRACTS",
            Self::Consolidates => "CONSOLIDATES",
            Self::Separates => "SEPARATES",
            Self::Reinterprets => "REINTERPRETS",
            Self::RelevantTo => "RELEVANT_TO",
            Self::ApplicableUnder => "APPLICABLE_UNDER",
            Self::AddressesGap => "ADDRESSES_GAP",
            Self::TestsHypothesis => "TESTS_HYPOTHESIS",
            Self::Presupposes => "PRESUPPOSES",
            Self::Causes => "CAUSES",
            Self::ContributesTo => "CONTRIBUTES_TO",
            Self::Enables => "ENABLES",
            Self::Prevents => "PREVENTS",
            Self::Intensifies => "INTENSIFIES",
            Self::Attenuates => "ATTENUATES",
        }
    }
    pub(crate) fn family(self) -> RelationFamily {
        match self {
            Self::EquivalentTo
            | Self::MoreSpecificThan
            | Self::Entails
            | Self::Contradicts
            | Self::PartialOverlap => RelationFamily::Semantic,
            Self::Refines
            | Self::Supersedes
            | Self::Retracts
            | Self::Consolidates
            | Self::Separates
            | Self::Reinterprets => RelationFamily::Genealogy,
            Self::RelevantTo
            | Self::ApplicableUnder
            | Self::AddressesGap
            | Self::TestsHypothesis
            | Self::Presupposes => RelationFamily::Contextual,
            Self::Causes
            | Self::ContributesTo
            | Self::Enables
            | Self::Prevents
            | Self::Intensifies
            | Self::Attenuates => RelationFamily::Causal,
        }
    }
    fn symmetric(self) -> bool {
        matches!(
            self,
            Self::EquivalentTo | Self::Contradicts | Self::PartialOverlap
        )
    }
    pub(crate) fn accepts_endpoints(self, left: &str, right: &str) -> bool {
        match self {
            Self::AddressesGap => left == "QUESTION" && right == "INFORMATION_GAP",
            Self::TestsHypothesis => left == "QUESTION" && right == "HYPOTHESIS",
            Self::Presupposes => {
                left == "QUESTION" && matches!(right, "PROPOSITION" | "ASSUMPTION" | "HYPOTHESIS")
            }
            _ if left == "QUESTION" || right == "QUESTION" => {
                self == Self::RelevantTo
                    || (self.family() == RelationFamily::Genealogy && left == right)
            }
            _ => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchEvidence {
    pub evidence_id: Uuid,
    pub document_id: Uuid,
    pub ordinal: i32,
    pub speaker_label: String,
    pub content: String,
    pub source_class: String,
    pub source_locator: String,
    pub document_locator: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpertRelation {
    pub relation_id: Uuid,
    pub situation_id: Uuid,
    pub family: RelationFamily,
    pub kind: RelationKind,
    pub left_object_id: Uuid,
    pub right_object_id: Uuid,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArgumentMode {
    Deductive,
    Inductive,
    Abductive,
    Causal,
    Analogical,
    Policy,
    Other,
}
impl ArgumentMode {
    fn db_value(self) -> &'static str {
        match self {
            Self::Deductive => "DEDUCTIVE",
            Self::Inductive => "INDUCTIVE",
            Self::Abductive => "ABDUCTIVE",
            Self::Causal => "CAUSAL",
            Self::Analogical => "ANALOGICAL",
            Self::Policy => "POLICY",
            Self::Other => "OTHER",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpertArgument {
    pub argument_id: Uuid,
    pub situation_id: Uuid,
    pub conclusion_object_id: Uuid,
    pub mode: ArgumentMode,
    pub rationale: String,
    pub created_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroundingEffect {
    Supports,
    Weakens,
    Challenges,
    Conditions,
}
impl GroundingEffect {
    fn db_value(self) -> &'static str {
        match self {
            Self::Supports => "SUPPORTS",
            Self::Weakens => "WEAKENS",
            Self::Challenges => "CHALLENGES",
            Self::Conditions => "CONDITIONS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroundingAdequacy {
    Unspecified,
    Partial,
    Sufficient,
    NecessaryCondition,
}
impl GroundingAdequacy {
    fn db_value(self) -> &'static str {
        match self {
            Self::Unspecified => "UNSPECIFIED",
            Self::Partial => "PARTIAL",
            Self::Sufficient => "SUFFICIENT",
            Self::NecessaryCondition => "NECESSARY_CONDITION",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroundingMemberRole {
    Premise,
    Assumption,
    Evidence,
    Context,
}
impl GroundingMemberRole {
    fn db_value(self) -> &'static str {
        match self {
            Self::Premise => "PREMISE",
            Self::Assumption => "ASSUMPTION",
            Self::Evidence => "EVIDENCE",
            Self::Context => "CONTEXT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewGroundingMember {
    pub member_id: Uuid,
    pub role: GroundingMemberRole,
    pub ground_object_id: Option<Uuid>,
    pub evidence_id: Option<Uuid>,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingMemberView {
    pub member_id: Uuid,
    pub role: GroundingMemberRole,
    pub ground_object_id: Option<Uuid>,
    pub evidence_id: Option<Uuid>,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingSetView {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub effect: GroundingEffect,
    pub adequacy: GroundingAdequacy,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub members: Vec<GroundingMemberView>,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReassessmentImpactKind {
    GroundMember,
    ApplicabilityCondition,
    Conclusion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReassessmentDisposition {
    Retained,
    RevisionRequired,
    Deferred,
}
impl ReassessmentDisposition {
    fn db_value(self) -> &'static str {
        match self {
            Self::Retained => "RETAINED",
            Self::RevisionRequired => "REVISION_REQUIRED",
            Self::Deferred => "DEFERRED",
        }
    }
    fn from_db(value: &str) -> Result<Self, WorkbenchError> {
        match value {
            "RETAINED" => Ok(Self::Retained),
            "REVISION_REQUIRED" => Ok(Self::RevisionRequired),
            "DEFERRED" => Ok(Self::Deferred),
            _ => Err(WorkbenchError::Invalid),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassessmentReview {
    pub review_event_id: Uuid,
    pub trigger_relation_id: Uuid,
    pub argument_id: Uuid,
    pub grounding_set_id: Option<Uuid>,
    pub disposition: ReassessmentDisposition,
    pub note: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassessmentImpact {
    pub trigger_relation_id: Uuid,
    pub relation_kind: RelationKind,
    pub revision_object_id: Uuid,
    pub prior_object_id: Uuid,
    pub argument_id: Uuid,
    pub grounding_set_id: Option<Uuid>,
    pub conclusion_object_id: Uuid,
    pub impact_kind: ReassessmentImpactKind,
    pub depth: u16,
    pub latest_review: Option<ReassessmentReview>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GroundingRouteRef {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingCycleDiagnostic {
    pub object_ids: Vec<Uuid>,
    pub argument_ids: Vec<Uuid>,
    pub external_grounding_routes: Vec<GroundingRouteRef>,
    pub external_only_grounding_routes: Vec<GroundingRouteRef>,
    pub has_external_anchor: bool,
    pub unanchored_recursive_support: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassessmentAttentionSummary {
    pub impacted: usize,
    pub unreviewed: usize,
    pub deferred: usize,
    pub revision_required: usize,
    pub retained: usize,
}

impl ReassessmentAttentionSummary {
    fn record(&mut self, disposition: Option<ReassessmentDisposition>) {
        self.impacted += 1;
        match disposition {
            None => self.unreviewed += 1,
            Some(ReassessmentDisposition::Deferred) => self.deferred += 1,
            Some(ReassessmentDisposition::RevisionRequired) => self.revision_required += 1,
            Some(ReassessmentDisposition::Retained) => self.retained += 1,
        }
    }
}

/// Counts distinct dependency/revision obligations, not strength or validity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssessmentReassessmentAttention {
    pub directly_relying: ReassessmentAttentionSummary,
    pub downstream: ReassessmentAttentionSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningRouteDiagnostic {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub effect: GroundingEffect,
    pub adequacy: GroundingAdequacy,
    pub member_count: usize,
    pub non_context_member_count: usize,
    pub applicability_conditions: Vec<RouteApplicabilityAssessment>,
    pub reassessment: ReassessmentAttentionSummary,
    pub assessment_reassessment: AssessmentReassessmentAttention,
    pub interpretation_reassessment: AssessmentReassessmentAttention,
    pub recursive_support_cycle: bool,
    pub unanchored_recursive_support: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApplicabilityAssessment {
    Unassessed,
    Satisfied,
    NotSatisfied,
    Contested,
    Unknown,
}
impl ApplicabilityAssessment {
    fn db_value(self) -> &'static str {
        match self {
            Self::Unassessed => "UNASSESSED",
            Self::Satisfied => "SATISFIED",
            Self::NotSatisfied => "NOT_SATISFIED",
            Self::Contested => "CONTESTED",
            Self::Unknown => "UNKNOWN",
        }
    }
    fn from_db(value: &str) -> Result<Self, WorkbenchError> {
        match value {
            "UNASSESSED" => Ok(Self::Unassessed),
            "SATISFIED" => Ok(Self::Satisfied),
            "NOT_SATISFIED" => Ok(Self::NotSatisfied),
            "CONTESTED" => Ok(Self::Contested),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(WorkbenchError::Invalid),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteApplicabilityAssessment {
    pub event_id: Uuid,
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub condition_object_id: Uuid,
    pub assessment: ApplicabilityAssessment,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedGroundDiagnostic {
    pub ground_object_id: Option<Uuid>,
    pub evidence_id: Option<Uuid>,
    pub routes: Vec<GroundingRouteRef>,
}

/// An authored contradiction whose two endpoints both occur as non-contextual
/// grounds in supporting routes for the same conclusion. This is a structural
/// review candidate, not evidence that the contradiction is true or self-sealing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContradictorySupportDiagnostic {
    pub relation_id: Uuid,
    pub left_object_id: Uuid,
    pub right_object_id: Uuid,
    pub left_supporting_routes: Vec<GroundingRouteRef>,
    pub right_supporting_routes: Vec<GroundingRouteRef>,
}

/// An explicit information-gap object used as a non-contextual supporting
/// ground. This does not establish absence of evidence or verified absence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InformationGapSupportDiagnostic {
    pub gap_object_id: Uuid,
    pub supporting_routes: Vec<GroundingRouteRef>,
}

/// Diagnostic of explicitly authored documentary grounds only. Different
/// components do not establish independence; object-ground ancestry is not
/// inferred by this projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceLineageStatus {
    NoDirectEvidence,
    RecordedLineageResolved,
    MissingOrInvisible,
    Withdrawn,
    Cycle,
    InvalidOrLimit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceDependenceDiagnostic {
    pub evidence_ids: Vec<Uuid>,
    pub routes: Vec<GroundingRouteRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectEvidenceLineageDiagnostic {
    pub status: EvidenceLineageStatus,
    pub selected_evidence_count: usize,
    pub components: Vec<EvidenceDependenceDiagnostic>,
    pub establishes_independence: bool,
}

impl Default for DirectEvidenceLineageDiagnostic {
    fn default() -> Self {
        Self {
            status: EvidenceLineageStatus::NoDirectEvidence,
            selected_evidence_count: 0,
            components: Vec::new(),
            establishes_independence: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConclusionReasoningDiagnostic {
    pub conclusion_object_id: Uuid,
    pub argument_ids: Vec<Uuid>,
    pub routes: Vec<ReasoningRouteDiagnostic>,
    pub shared_grounds: Vec<SharedGroundDiagnostic>,
    #[serde(default)]
    pub contradictory_supports: Vec<ContradictorySupportDiagnostic>,
    #[serde(default)]
    pub information_gap_supports: Vec<InformationGapSupportDiagnostic>,
    pub direct_evidence_lineage: DirectEvidenceLineageDiagnostic,
    pub has_supporting_routes: bool,
    pub has_opposing_routes: bool,
    pub has_mixed_directional_grounding: bool,
    pub conclusion_reassessment: ReassessmentAttentionSummary,
}

/// One authored grounding route reached from a human-authored hypothesis or
/// assumption.  Reachability is structural: it says that the source
/// participates, directly or through another authored conclusion, in this
/// route.  It does not say that the source alone establishes the conclusion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredObjectImpactRoute {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub conclusion_object_id: Uuid,
    pub effect: GroundingEffect,
    pub adequacy: GroundingAdequacy,
    pub depth: u16,
    pub reachable_ground_object_ids: Vec<Uuid>,
    pub non_context_member_count: usize,
    pub source_is_direct_ground: bool,
    /// Other authored grounding routes to this same conclusion that are not
    /// structurally reached from the source. Their presence prevents an
    /// impacted route from being mistaken for the conclusion's only basis.
    pub unreached_parallel_route_count: usize,
}

/// Deterministic impact projection for structure explicitly authored by the
/// investigator.  Niccolo does not generate the hypothesis or counterfactual,
/// promote an assumption into fact, compose directional effects, or infer
/// that a reachable conclusion is true.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoredObjectImpactAnalysis {
    pub source_object_id: Uuid,
    pub source_kind: ExpertNodeKind,
    pub route_impacts: Vec<AuthoredObjectImpactRoute>,
    pub reached_conclusion_object_ids: Vec<Uuid>,
    pub contradicted_object_ids: Vec<Uuid>,
    pub tested_by_question_ids: Vec<Uuid>,
    pub structural_routes_only: bool,
    pub establishes_truth: bool,
    pub directional_effects_are_aggregated: bool,
    pub treats_assumption_as_fact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryLink {
    pub relation_id: Uuid,
    pub target_object_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryCriterionView {
    pub dimension: assessment::AssessmentDimension,
    pub current_events: Vec<Uuid>,
}

/// A projection of authored links and unrevised judgments, not an answer,
/// a gap resolution, or a ranking of investigative questions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionInquiryView {
    pub question_object_id: Uuid,
    pub addressed_gaps: Vec<InquiryLink>,
    pub tested_hypotheses: Vec<InquiryLink>,
    pub presuppositions: Vec<InquiryLink>,
    pub current_relevance_events: Vec<Uuid>,
    pub current_discriminatory_power_events: Vec<Uuid>,
    pub selection_criteria: Vec<InquiryCriterionView>,
}

fn question_inquiries(
    nodes: &[ExpertNode],
    relations: &[ExpertRelation],
    assessments: &[assessment::AssessmentEvent],
) -> Vec<QuestionInquiryView> {
    let mut questions: BTreeMap<_, _> = nodes
        .iter()
        .filter(|n| n.kind == ExpertNodeKind::Question)
        .map(|n| {
            (
                n.object_id,
                QuestionInquiryView {
                    question_object_id: n.object_id,
                    selection_criteria: assessment::AssessmentDimension::QUESTION_SELECTION_CRITERIA
                        .into_iter().map(|dimension| InquiryCriterionView {dimension, current_events: vec![]}).collect(),
                    ..Default::default()
                },
            )
        })
        .collect();
    for relation in relations {
        let Some(question) = questions.get_mut(&relation.left_object_id) else {
            continue;
        };
        let links = match relation.kind {
            RelationKind::AddressesGap => &mut question.addressed_gaps,
            RelationKind::TestsHypothesis => &mut question.tested_hypotheses,
            RelationKind::Presupposes => &mut question.presuppositions,
            _ => continue,
        };
        links.push(InquiryLink {
            relation_id: relation.relation_id,
            target_object_id: relation.right_object_id,
        });
    }
    let superseded: BTreeSet<_> = assessments
        .iter()
        .filter_map(|a| a.input.revises_event_id)
        .collect();
    for event in assessments {
        if superseded.contains(&event.event_id) {
            continue;
        }
        let assessment::AssessmentTarget::Object { object_id } = event.input.target else {
            continue;
        };
        let Some(question) = questions.get_mut(&object_id) else {
            continue;
        };
        match event.input.dimension {
            assessment::AssessmentDimension::Relevance => {
                question.current_relevance_events.push(event.event_id)
            }
            assessment::AssessmentDimension::DiscriminatoryPower => question
                .current_discriminatory_power_events
                .push(event.event_id),
            dimension if dimension.is_question_selection_criterion() => {
                if let Some(criterion) = question.selection_criteria.iter_mut().find(|c| c.dimension == dimension) {
                    criterion.current_events.push(event.event_id);
                }
            }
            _ => {}
        }
    }
    questions.into_values().collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchDraftSource {
    pub draft_id: Uuid,
    pub provider: String,
    pub model: String,
    pub object_ids: Vec<Uuid>,
    pub relation_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchSnapshot {
    pub situation_id: Uuid,
    pub nodes: Vec<ExpertNode>,
    pub evidence: Vec<WorkbenchEvidence>,
    pub evidence_truncated: bool,
    pub relations: Vec<ExpertRelation>,
    pub arguments: Vec<ExpertArgument>,
    pub grounding_sets: Vec<GroundingSetView>,
    pub reassessment_impacts: Vec<ReassessmentImpact>,
    pub grounding_cycles: Vec<GroundingCycleDiagnostic>,
    pub reasoning_diagnostics: Vec<ConclusionReasoningDiagnostic>,
    pub authored_object_impacts: Vec<AuthoredObjectImpactAnalysis>,
    pub question_inquiries: Vec<QuestionInquiryView>,
    pub question_decisions: Vec<inquiry_decision::InquiryDecision>,
    pub question_decision_basis_changes: Vec<inquiry_decision_review::DecisionBasisChange>,
    pub action_recommendations: Vec<action_recommendation::Recommendation>,
    pub recommendation_basis_impacts: Vec<action_recommendation::RecommendationBasisImpact>,
    pub action_decisions: Vec<action_decision::Decision>,
    pub intervention_records: Vec<intervention::InterventionRecord>,
    pub intervention_causal_assessments: Vec<intervention::CausalAssessmentView>,
    pub actor_epistemic_states: Vec<actor_state::StateEvent>,
    pub normative_assessments: Vec<normative_assessment::NormativeEvent>,
    pub actor_epistemic_acts: Vec<actor_act::ActEvent>,
    pub question_responses: Vec<inquiry_response::QuestionResponse>,
    pub response_interpretations: Vec<inquiry_response::ResponseInterpretationView>,
    pub interpretation_dependencies: Vec<interpretation_dependency::InterpretationDependencyView>,
    pub assessments: Vec<assessment::AssessmentEvent>,
    pub assessment_dependencies: Vec<assessment::AssessmentDependencyView>,
    pub draft_sources: Vec<WorkbenchDraftSource>,
    #[serde(default)]
    pub artifact_provenance: Vec<ArtifactProvenance>,
    pub investigator_free_text_compiler_enabled: bool,
}

type WorkbenchEvidenceRow = (Uuid, Uuid, i32, String, String, String, String, String);
type StoredGroundingMemberRow = (String, Option<Uuid>, Option<Uuid>, String, Uuid);
type ReassessmentReviewRow = (Uuid, Uuid, Option<Uuid>, Uuid, String, String, Uuid);
type GroundingMemberSnapshotRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    Option<Uuid>,
    Option<Uuid>,
    String,
    Uuid,
);

pub(crate) async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, WorkbenchError> {
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
        return Err(WorkbenchError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

async fn situation_visible(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<bool, WorkbenchError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut **tx)
    .await?)
}

async fn ensure_object(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    object_id: Uuid,
    object_kind: &str,
    actor: Uuid,
) -> Result<bool, WorkbenchError> {
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_objects(workspace_id,object_id,object_kind,created_by_principal_id) \
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(object_id)
    .bind(object_kind)
    .bind(actor)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT object_kind,created_by_principal_id FROM niccolo.epistemic_objects \
         WHERE workspace_id=$1 AND object_id=$2",
    )
    .bind(workspace)
    .bind(object_id)
    .fetch_optional(&mut **tx)
    .await?;
    match stored {
        Some((kind, by)) if kind == object_kind && by == actor => Ok(inserted),
        Some(_) => Err(WorkbenchError::Conflict),
        None => Err(WorkbenchError::Invisible),
    }
}

async fn object_available_in_situation(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    object_id: Uuid,
) -> Result<Option<String>, WorkbenchError> {
    let kind: Option<String> = sqlx::query_scalar(
        "SELECT object_kind FROM niccolo.epistemic_objects WHERE workspace_id=$1 AND object_id=$2",
    )
    .bind(workspace)
    .bind(object_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(kind) = kind else {
        return Ok(None);
    };
    let available: bool = match kind.as_str() {
        "PROPOSITION" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_situation_propositions WHERE workspace_id=$1 AND situation_id=$2 AND proposition_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "HYPOTHESIS" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_hypotheses WHERE workspace_id=$1 AND situation_id=$2 AND hypothesis_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "ASSUMPTION" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_assumptions WHERE workspace_id=$1 AND situation_id=$2 AND assumption_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "INFORMATION_GAP" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_information_gaps WHERE workspace_id=$1 AND situation_id=$2 AND gap_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "QUESTION" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_questions WHERE workspace_id=$1 AND situation_id=$2 AND question_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "FINDING" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_findings WHERE workspace_id=$1 AND situation_id=$2 AND finding_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "ARGUMENT" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_arguments WHERE workspace_id=$1 AND situation_id=$2 AND argument_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        _ => false,
    };
    Ok(available.then_some(kind))
}

pub async fn link_artifact_provenance(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    object: Uuid,
    representation: Uuid,
    rationale: &str,
) -> Result<bool, WorkbenchError> {
    let rationale = rationale.trim();
    if !valid_text(rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    match object_available_in_situation(&mut tx, workspace, situation, object).await? {
        None => return Err(WorkbenchError::Invisible),
        Some(kind) if kind == "ARGUMENT" => return Err(WorkbenchError::Invalid),
        Some(_) => (),
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.workbench_artifact_provenance
            (workspace_id,situation_id,object_id,representation_id,rationale,recorded_by)
         SELECT $1,$2,$3,$4,$5,$6 FROM niccolo.situation_artifact_representations
         WHERE workspace_id=$1 AND situation_id=$2 AND representation_id=$4
         ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(object)
    .bind(representation)
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected() == 1;
    if !inserted {
        let prior: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT rationale,recorded_by FROM niccolo.workbench_artifact_provenance
             WHERE workspace_id=$1 AND situation_id=$2 AND object_id=$3 AND representation_id=$4",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object)
        .bind(representation)
        .fetch_optional(&mut *tx)
        .await?;
        match prior {
            None => return Err(WorkbenchError::Invisible),
            Some((note, by)) if note != rationale || by != actor => return Err(WorkbenchError::Conflict),
            Some(_) => (),
        }
    }
    tx.commit().await?;
    Ok(inserted)
}

#[expect(
    clippy::too_many_arguments,
    reason = "node identity, scope, semantics and authorship are independent inputs"
)]
pub async fn create_node(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    object_id: Uuid,
    kind: ExpertNodeKind,
    text: &str,
    rationale: &str,
) -> Result<ExpertNode, WorkbenchError> {
    let mut tx = scoped(pool, workspace, false).await?;
    let node = create_node_in_tx(
        &mut tx, workspace, situation, actor, object_id, kind, text, rationale,
    ).await?;
    tx.commit().await?;
    Ok(node)
}

#[expect(
    clippy::too_many_arguments,
    reason = "node identity, scope, semantics and authorship are independent inputs"
)]
pub(crate) async fn create_node_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    object_id: Uuid,
    kind: ExpertNodeKind,
    text: &str,
    rationale: &str,
) -> Result<ExpertNode, WorkbenchError> {
    if !valid_text(text, 8192) || !valid_text(rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let text = text.trim();
    let rationale = rationale.trim();
    if !situation_visible(tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let object_inserted =
        ensure_object(tx, workspace, object_id, kind.db_value(), actor).await?;
    let mut request_replayed = !object_inserted;
    if kind == ExpertNodeKind::Proposition {
        sqlx::query(
            "INSERT INTO niccolo.epistemic_propositions(workspace_id,proposition_id,statement_text,language) \
             VALUES($1,$2,$3,'en') ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(object_id)
        .bind(text)
        .execute(&mut **tx)
        .await?;
        let stored_text: Option<String> = sqlx::query_scalar(
            "SELECT statement_text FROM niccolo.epistemic_propositions WHERE workspace_id=$1 AND proposition_id=$2",
        )
        .bind(workspace)
        .bind(object_id)
        .fetch_optional(&mut **tx)
        .await?;
        if stored_text.as_deref() != Some(text) {
            return Err(WorkbenchError::Conflict);
        }
        let membership_inserted = sqlx::query(
            "INSERT INTO niccolo.epistemic_situation_propositions \
             (workspace_id,situation_id,proposition_id,rationale,recorded_by_principal_id) \
             VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .bind(rationale)
        .bind(actor)
        .execute(&mut **tx)
        .await?
        .rows_affected()
            == 1;
        request_replayed = !membership_inserted;
        let membership: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT rationale,recorded_by_principal_id FROM niccolo.epistemic_situation_propositions \
             WHERE workspace_id=$1 AND situation_id=$2 AND proposition_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_optional(&mut **tx)
        .await?;
        if membership != Some((rationale.to_owned(), actor)) {
            return Err(WorkbenchError::Conflict);
        }
    } else {
        let sql = match kind {
            ExpertNodeKind::Proposition => unreachable!(),
            ExpertNodeKind::Hypothesis => {
                "INSERT INTO niccolo.epistemic_hypotheses(workspace_id,hypothesis_id,situation_id,statement_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::Assumption => {
                "INSERT INTO niccolo.epistemic_assumptions(workspace_id,assumption_id,situation_id,statement_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::InformationGap => {
                "INSERT INTO niccolo.epistemic_information_gaps(workspace_id,gap_id,situation_id,description,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::Question => {
                "INSERT INTO niccolo.epistemic_questions(workspace_id,question_id,situation_id,question_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::Finding => {
                "INSERT INTO niccolo.epistemic_findings(workspace_id,finding_id,situation_id,statement_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
        };
        sqlx::query(sql)
            .bind(workspace)
            .bind(object_id)
            .bind(situation)
            .bind(text)
            .bind(rationale)
            .execute(&mut **tx)
            .await?;
        let query = match kind {
            ExpertNodeKind::Proposition => unreachable!(),
            ExpertNodeKind::Hypothesis => {
                "SELECT situation_id,statement_text,rationale FROM niccolo.epistemic_hypotheses WHERE workspace_id=$1 AND hypothesis_id=$2"
            }
            ExpertNodeKind::Assumption => {
                "SELECT situation_id,statement_text,rationale FROM niccolo.epistemic_assumptions WHERE workspace_id=$1 AND assumption_id=$2"
            }
            ExpertNodeKind::InformationGap => {
                "SELECT situation_id,description,rationale FROM niccolo.epistemic_information_gaps WHERE workspace_id=$1 AND gap_id=$2"
            }
            ExpertNodeKind::Question => {
                "SELECT situation_id,question_text,rationale FROM niccolo.epistemic_questions WHERE workspace_id=$1 AND question_id=$2"
            }
            ExpertNodeKind::Finding => {
                "SELECT situation_id,statement_text,rationale FROM niccolo.epistemic_findings WHERE workspace_id=$1 AND finding_id=$2"
            }
        };
        let stored: Option<(Uuid, String, String)> = sqlx::query_as(query)
            .bind(workspace)
            .bind(object_id)
            .fetch_optional(&mut **tx)
            .await?;
        if stored != Some((situation, text.to_owned(), rationale.to_owned())) {
            return Err(WorkbenchError::Conflict);
        }
    }
    Ok(ExpertNode {
        object_id,
        kind,
        situation_id: situation,
        text: text.to_owned(),
        rationale: rationale.to_owned(),
        created_by: actor,
        replayed: request_replayed,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "relation identity, endpoints, semantics and authorship are independent inputs"
)]
pub async fn create_relation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    relation_id: Uuid,
    family: RelationFamily,
    kind: RelationKind,
    left_object_id: Uuid,
    right_object_id: Uuid,
    rationale: &str,
) -> Result<ExpertRelation, WorkbenchError> {
    let mut tx = scoped(pool, workspace, false).await?;
    let relation = create_relation_in_tx(
        &mut tx, workspace, situation, actor, relation_id, family, kind,
        left_object_id, right_object_id, rationale,
    ).await?;
    tx.commit().await?;
    Ok(relation)
}

#[expect(
    clippy::too_many_arguments,
    reason = "relation identity, endpoints, semantics and authorship are independent inputs"
)]
pub(crate) async fn create_relation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    relation_id: Uuid,
    family: RelationFamily,
    kind: RelationKind,
    mut left_object_id: Uuid,
    mut right_object_id: Uuid,
    rationale: &str,
) -> Result<ExpertRelation, WorkbenchError> {
    if kind.family() != family || left_object_id == right_object_id || !valid_text(rationale, 4096)
    {
        return Err(WorkbenchError::Invalid);
    }
    if kind.symmetric() && right_object_id < left_object_id {
        std::mem::swap(&mut left_object_id, &mut right_object_id);
    }
    let rationale = rationale.trim();
    let left_kind = object_available_in_situation(tx, workspace, situation, left_object_id)
        .await?
        .ok_or(WorkbenchError::Invisible)?;
    let right_kind = object_available_in_situation(tx, workspace, situation, right_object_id)
        .await?
        .ok_or(WorkbenchError::Invisible)?;
    if !kind.accepts_endpoints(&left_kind, &right_kind) {
        return Err(WorkbenchError::Invalid);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_relations \
         (workspace_id,situation_id,relation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(relation_id)
    .bind(family.db_value())
    .bind(kind.db_value())
    .bind(left_object_id)
    .bind(right_object_id)
    .bind(rationale)
    .bind(actor)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(Uuid, String, String, Uuid, Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT situation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_relations WHERE workspace_id=$1 AND relation_id=$2",
    )
    .bind(workspace)
    .bind(relation_id)
    .fetch_optional(&mut **tx)
    .await?;
    if stored
        != Some((
            situation,
            family.db_value().to_owned(),
            kind.db_value().to_owned(),
            left_object_id,
            right_object_id,
            rationale.to_owned(),
            actor,
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    Ok(ExpertRelation {
        relation_id,
        situation_id: situation,
        family,
        kind,
        left_object_id,
        right_object_id,
        rationale: rationale.to_owned(),
        recorded_by: actor,
        replayed: !inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "argument identity, conclusion, method and authorship are independent inputs"
)]
pub async fn create_argument(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    argument_id: Uuid,
    conclusion_object_id: Uuid,
    mode: ArgumentMode,
    rationale: &str,
) -> Result<ExpertArgument, WorkbenchError> {
    if argument_id == conclusion_object_id || !valid_text(rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let conclusion_kind =
        object_available_in_situation(&mut tx, workspace, situation, conclusion_object_id).await?;
    if conclusion_kind.is_none() || conclusion_kind.as_deref() == Some("ARGUMENT") {
        return Err(WorkbenchError::Invisible);
    }
    if conclusion_kind.as_deref() == Some("QUESTION") {
        return Err(WorkbenchError::Invalid);
    }
    let object_inserted = ensure_object(&mut tx, workspace, argument_id, "ARGUMENT", actor).await?;
    sqlx::query(
        "INSERT INTO niccolo.epistemic_arguments \
         (workspace_id,argument_id,situation_id,conclusion_object_id,argument_mode,rationale) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(situation)
    .bind(conclusion_object_id)
    .bind(mode.db_value())
    .bind(rationale)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT situation_id,conclusion_object_id,argument_mode,rationale \
         FROM niccolo.epistemic_arguments WHERE workspace_id=$1 AND argument_id=$2",
    )
    .bind(workspace)
    .bind(argument_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            situation,
            conclusion_object_id,
            mode.db_value().to_owned(),
            rationale.to_owned(),
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(ExpertArgument {
        argument_id,
        situation_id: situation,
        conclusion_object_id,
        mode,
        rationale: rationale.to_owned(),
        created_by: actor,
        replayed: !object_inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "grounding identity, semantics and members are independent authored inputs"
)]
pub async fn create_grounding_set(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    argument_id: Uuid,
    grounding_set_id: Uuid,
    effect: GroundingEffect,
    adequacy: GroundingAdequacy,
    rationale: &str,
    members: &[NewGroundingMember],
) -> Result<GroundingSetView, WorkbenchError> {
    if !valid_text(rationale, 4096)
        || members.is_empty()
        || members.len() > 64
        || members
            .iter()
            .map(|m| m.member_id)
            .collect::<BTreeSet<_>>()
            .len()
            != members.len()
    {
        return Err(WorkbenchError::Invalid);
    }
    for member in members {
        let one_target = member.ground_object_id.is_some() ^ member.evidence_id.is_some();
        if !one_target
            || !valid_text(&member.rationale, 2048)
            || (member.role == GroundingMemberRole::Evidence) != member.evidence_id.is_some()
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let argument_visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_arguments \
         WHERE workspace_id=$1 AND situation_id=$2 AND argument_id=$3)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(argument_id)
    .fetch_one(&mut *tx)
    .await?;
    if !argument_visible {
        return Err(WorkbenchError::Invisible);
    }
    for member in members {
        if let Some(object_id) = member.ground_object_id {
            let kind = object_available_in_situation(&mut tx, workspace, situation, object_id)
                .await?
                .ok_or(WorkbenchError::Invisible)?;
            if kind == "QUESTION" && member.role != GroundingMemberRole::Context {
                return Err(WorkbenchError::Invalid);
            }
        }
        if let Some(evidence_id) = member.evidence_id {
            let visible: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM niccolo.source_fragments f \
                 JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id \
                 WHERE f.workspace_id=$1 AND s.situation_id=$2 AND f.evidence_id=$3)",
            )
            .bind(workspace)
            .bind(situation)
            .bind(evidence_id)
            .fetch_one(&mut *tx)
            .await?;
            if !visible {
                return Err(WorkbenchError::Invisible);
            }
        }
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_grounding_sets \
         (workspace_id,argument_id,grounding_set_id,effect,adequacy,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(grounding_set_id)
    .bind(effect.db_value())
    .bind(adequacy.db_value())
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored_set: Option<(String, String, String, Uuid)> = sqlx::query_as(
        "SELECT effect,adequacy,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_grounding_sets \
         WHERE workspace_id=$1 AND argument_id=$2 AND grounding_set_id=$3",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(grounding_set_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored_set
        != Some((
            effect.db_value().to_owned(),
            adequacy.db_value().to_owned(),
            rationale.to_owned(),
            actor,
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    for member in members {
        sqlx::query(
            "INSERT INTO niccolo.epistemic_grounding_members \
             (workspace_id,argument_id,grounding_set_id,member_id,member_role,ground_object_id,evidence_id,rationale,recorded_by_principal_id) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(argument_id)
        .bind(grounding_set_id)
        .bind(member.member_id)
        .bind(member.role.db_value())
        .bind(member.ground_object_id)
        .bind(member.evidence_id)
        .bind(member.rationale.trim())
        .bind(actor)
        .execute(&mut *tx)
        .await?;
        let stored: Option<StoredGroundingMemberRow> = sqlx::query_as(
            "SELECT member_role,ground_object_id,evidence_id,rationale,recorded_by_principal_id \
             FROM niccolo.epistemic_grounding_members \
             WHERE workspace_id=$1 AND argument_id=$2 AND grounding_set_id=$3 AND member_id=$4",
        )
        .bind(workspace)
        .bind(argument_id)
        .bind(grounding_set_id)
        .bind(member.member_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored
            != Some((
                member.role.db_value().to_owned(),
                member.ground_object_id,
                member.evidence_id,
                member.rationale.trim().to_owned(),
                actor,
            ))
        {
            return Err(WorkbenchError::Conflict);
        }
    }
    let stored_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.epistemic_grounding_members \
         WHERE workspace_id=$1 AND argument_id=$2 AND grounding_set_id=$3",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(grounding_set_id)
    .fetch_one(&mut *tx)
    .await?;
    if usize::try_from(stored_count).ok() != Some(members.len()) {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(GroundingSetView {
        argument_id,
        grounding_set_id,
        effect,
        adequacy,
        rationale: rationale.to_owned(),
        recorded_by: actor,
        members: members
            .iter()
            .map(|m| GroundingMemberView {
                member_id: m.member_id,
                role: m.role,
                ground_object_id: m.ground_object_id,
                evidence_id: m.evidence_id,
                rationale: m.rationale.trim().to_owned(),
                recorded_by: actor,
            })
            .collect(),
        replayed: !inserted,
    })
}

fn node_kind_from_db(value: &str) -> Result<ExpertNodeKind, WorkbenchError> {
    match value {
        "PROPOSITION" => Ok(ExpertNodeKind::Proposition),
        "HYPOTHESIS" => Ok(ExpertNodeKind::Hypothesis),
        "ASSUMPTION" => Ok(ExpertNodeKind::Assumption),
        "INFORMATION_GAP" => Ok(ExpertNodeKind::InformationGap),
        "QUESTION" => Ok(ExpertNodeKind::Question),
        "FINDING" => Ok(ExpertNodeKind::Finding),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn relation_family_from_db(value: &str) -> Result<RelationFamily, WorkbenchError> {
    match value {
        "SEMANTIC" => Ok(RelationFamily::Semantic),
        "GENEALOGY" => Ok(RelationFamily::Genealogy),
        "CONTEXTUAL" => Ok(RelationFamily::Contextual),
        "CAUSAL" => Ok(RelationFamily::Causal),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn relation_kind_from_db(value: &str) -> Result<RelationKind, WorkbenchError> {
    match value {
        "EQUIVALENT_TO" => Ok(RelationKind::EquivalentTo),
        "MORE_SPECIFIC_THAN" => Ok(RelationKind::MoreSpecificThan),
        "ENTAILS" => Ok(RelationKind::Entails),
        "CONTRADICTS" => Ok(RelationKind::Contradicts),
        "PARTIAL_OVERLAP" => Ok(RelationKind::PartialOverlap),
        "REFINES" => Ok(RelationKind::Refines),
        "SUPERSEDES" => Ok(RelationKind::Supersedes),
        "RETRACTS" => Ok(RelationKind::Retracts),
        "CONSOLIDATES" => Ok(RelationKind::Consolidates),
        "SEPARATES" => Ok(RelationKind::Separates),
        "REINTERPRETS" => Ok(RelationKind::Reinterprets),
        "RELEVANT_TO" => Ok(RelationKind::RelevantTo),
        "APPLICABLE_UNDER" => Ok(RelationKind::ApplicableUnder),
        "ADDRESSES_GAP" => Ok(RelationKind::AddressesGap),
        "TESTS_HYPOTHESIS" => Ok(RelationKind::TestsHypothesis),
        "PRESUPPOSES" => Ok(RelationKind::Presupposes),
        "CAUSES" => Ok(RelationKind::Causes),
        "CONTRIBUTES_TO" => Ok(RelationKind::ContributesTo),
        "ENABLES" => Ok(RelationKind::Enables),
        "PREVENTS" => Ok(RelationKind::Prevents),
        "INTENSIFIES" => Ok(RelationKind::Intensifies),
        "ATTENUATES" => Ok(RelationKind::Attenuates),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn argument_mode_from_db(value: &str) -> Result<ArgumentMode, WorkbenchError> {
    match value {
        "DEDUCTIVE" => Ok(ArgumentMode::Deductive),
        "INDUCTIVE" => Ok(ArgumentMode::Inductive),
        "ABDUCTIVE" => Ok(ArgumentMode::Abductive),
        "CAUSAL" => Ok(ArgumentMode::Causal),
        "ANALOGICAL" => Ok(ArgumentMode::Analogical),
        "POLICY" => Ok(ArgumentMode::Policy),
        "OTHER" => Ok(ArgumentMode::Other),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn grounding_effect_from_db(value: &str) -> Result<GroundingEffect, WorkbenchError> {
    match value {
        "SUPPORTS" => Ok(GroundingEffect::Supports),
        "WEAKENS" => Ok(GroundingEffect::Weakens),
        "CHALLENGES" => Ok(GroundingEffect::Challenges),
        "CONDITIONS" => Ok(GroundingEffect::Conditions),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn grounding_adequacy_from_db(value: &str) -> Result<GroundingAdequacy, WorkbenchError> {
    match value {
        "UNSPECIFIED" => Ok(GroundingAdequacy::Unspecified),
        "PARTIAL" => Ok(GroundingAdequacy::Partial),
        "SUFFICIENT" => Ok(GroundingAdequacy::Sufficient),
        "NECESSARY_CONDITION" => Ok(GroundingAdequacy::NecessaryCondition),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn member_role_from_db(value: &str) -> Result<GroundingMemberRole, WorkbenchError> {
    match value {
        "PREMISE" => Ok(GroundingMemberRole::Premise),
        "ASSUMPTION" => Ok(GroundingMemberRole::Assumption),
        "EVIDENCE" => Ok(GroundingMemberRole::Evidence),
        "CONTEXT" => Ok(GroundingMemberRole::Context),
        _ => Err(WorkbenchError::Invalid),
    }
}

pub async fn search_evidence(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    query: &str,
    limit: i64,
) -> Result<Vec<WorkbenchEvidence>, WorkbenchError> {
    if !valid_text(query, 512) || !(1..=50).contains(&limit) {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let rows: Vec<WorkbenchEvidenceRow> = sqlx::query_as(
        "SELECT f.evidence_id,f.document_id,f.ordinal,f.speaker_label,e.content,e.source_class,e.source_locator,d.locator \
         FROM niccolo.situation_sources ss \
         JOIN niccolo.source_fragments f ON f.workspace_id=ss.workspace_id AND f.document_id=ss.document_id \
         JOIN niccolo.evidence e ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id \
         JOIN niccolo.source_documents d ON d.workspace_id=f.workspace_id AND d.document_id=f.document_id \
         WHERE ss.workspace_id=$1 AND ss.situation_id=$2 AND e.admissible \
           AND to_tsvector('simple',e.content) @@ plainto_tsquery('simple',$3) \
         ORDER BY ts_rank_cd(to_tsvector('simple',e.content),plainto_tsquery('simple',$3)) DESC, \
                  d.created_at,f.document_id,f.ordinal LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(query.trim())
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                evidence_id,
                document_id,
                ordinal,
                speaker_label,
                content,
                source_class,
                source_locator,
                document_locator,
            )| WorkbenchEvidence {
                evidence_id,
                document_id,
                ordinal,
                speaker_label,
                content,
                source_class,
                source_locator,
                document_locator,
            },
        )
        .collect())
}

#[expect(
    clippy::too_many_arguments,
    reason = "reassessment identity, target and attributed disposition are independent audit inputs"
)]
pub async fn record_reassessment_review(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    review_event_id: Uuid,
    trigger_relation_id: Uuid,
    argument_id: Uuid,
    grounding_set_id: Option<Uuid>,
    disposition: ReassessmentDisposition,
    note: &str,
) -> Result<ReassessmentReview, WorkbenchError> {
    if !valid_text(note, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let snapshot = snapshot(pool, workspace, situation).await?;
    let impact_visible = snapshot.reassessment_impacts.iter().any(|impact| {
        impact.trigger_relation_id == trigger_relation_id
            && impact.argument_id == argument_id
            && impact.grounding_set_id == grounding_set_id
    });
    if !impact_visible {
        return Err(WorkbenchError::Invisible);
    }

    let note = note.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_reassessment_review_events \
         (workspace_id,situation_id,review_event_id,trigger_relation_id,argument_id,grounding_set_id,disposition,note,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(review_event_id)
    .bind(trigger_relation_id)
    .bind(argument_id)
    .bind(grounding_set_id)
    .bind(disposition.db_value())
    .bind(note)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(Uuid, Uuid, Option<Uuid>, String, String, Uuid)> = sqlx::query_as(
        "SELECT trigger_relation_id,argument_id,grounding_set_id,disposition,note,recorded_by_principal_id \
         FROM niccolo.epistemic_reassessment_review_events \
         WHERE workspace_id=$1 AND situation_id=$2 AND review_event_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(review_event_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        stored_trigger,
        stored_argument,
        stored_grounding,
        stored_disposition,
        stored_note,
        stored_actor,
    )) = stored
    else {
        return Err(WorkbenchError::Invisible);
    };
    if stored_trigger != trigger_relation_id
        || stored_argument != argument_id
        || stored_grounding != grounding_set_id
        || stored_disposition != disposition.db_value()
        || stored_note != note
        || stored_actor != actor
    {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(ReassessmentReview {
        review_event_id,
        trigger_relation_id,
        argument_id,
        grounding_set_id,
        disposition,
        note: note.to_owned(),
        recorded_by: actor,
        replayed: !inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "condition assessment identity, route and authorship are independent audit inputs"
)]
pub async fn record_route_applicability(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    event_id: Uuid,
    argument_id: Uuid,
    grounding_set_id: Uuid,
    condition_object_id: Uuid,
    assessment: ApplicabilityAssessment,
    rationale: &str,
) -> Result<RouteApplicabilityAssessment, WorkbenchError> {
    if !valid_text(rationale, 4096) || event_id.is_nil() {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let conclusion: Option<Uuid> = sqlx::query_scalar("SELECT a.conclusion_object_id FROM niccolo.epistemic_arguments a JOIN niccolo.epistemic_grounding_sets g ON a.workspace_id=g.workspace_id AND a.argument_id=g.argument_id WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.argument_id=$3 AND g.grounding_set_id=$4")
        .bind(workspace).bind(situation).bind(argument_id).bind(grounding_set_id).fetch_optional(&mut *tx).await?;
    let Some(conclusion) = conclusion else {
        return Err(WorkbenchError::Invisible);
    };
    let kind = object_available_in_situation(&mut tx, workspace, situation, condition_object_id)
        .await?
        .ok_or(WorkbenchError::Invisible)?;
    if condition_object_id == conclusion
        || !matches!(
            kind.as_str(),
            "PROPOSITION" | "HYPOTHESIS" | "ASSUMPTION" | "FINDING"
        )
    {
        return Err(WorkbenchError::Invalid);
    }
    let rationale = rationale.trim();
    let inserted = sqlx::query("INSERT INTO niccolo.epistemic_route_applicability_events (workspace_id,situation_id,event_id,argument_id,grounding_set_id,condition_object_id,assessment,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(workspace).bind(situation).bind(event_id).bind(argument_id).bind(grounding_set_id).bind(condition_object_id).bind(assessment.db_value()).bind(rationale).bind(actor).execute(&mut *tx).await?.rows_affected() == 1;
    let stored: Option<(Uuid,Uuid,Uuid,String,String,Uuid)> = sqlx::query_as("SELECT argument_id,grounding_set_id,condition_object_id,assessment,rationale,recorded_by_principal_id FROM niccolo.epistemic_route_applicability_events WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3")
        .bind(workspace).bind(situation).bind(event_id).fetch_optional(&mut *tx).await?;
    if stored
        != Some((
            argument_id,
            grounding_set_id,
            condition_object_id,
            assessment.db_value().to_owned(),
            rationale.to_owned(),
            actor,
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(RouteApplicabilityAssessment {
        event_id,
        argument_id,
        grounding_set_id,
        condition_object_id,
        assessment,
        rationale: rationale.to_owned(),
        recorded_by: actor,
        replayed: !inserted,
    })
}

async fn latest_route_applicability(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<RouteApplicabilityAssessment>, WorkbenchError> {
    let rows: Vec<(Uuid,Uuid,Uuid,Uuid,String,String,Uuid)> = sqlx::query_as("SELECT DISTINCT ON (argument_id,grounding_set_id,condition_object_id,recorded_by_principal_id) event_id,argument_id,grounding_set_id,condition_object_id,assessment,rationale,recorded_by_principal_id FROM niccolo.epistemic_route_applicability_events WHERE workspace_id=$1 AND situation_id=$2 ORDER BY argument_id,grounding_set_id,condition_object_id,recorded_by_principal_id,event_no DESC")
        .bind(workspace).bind(situation).fetch_all(&mut **tx).await?;
    rows.into_iter()
        .map(
            |(
                event_id,
                argument_id,
                grounding_set_id,
                condition_object_id,
                assessment,
                rationale,
                recorded_by,
            )| {
                Ok(RouteApplicabilityAssessment {
                    event_id,
                    argument_id,
                    grounding_set_id,
                    condition_object_id,
                    assessment: ApplicabilityAssessment::from_db(&assessment)?,
                    rationale,
                    recorded_by,
                    replayed: false,
                })
            },
        )
        .collect()
}

fn selective_reassessment_impacts(
    relations: &[ExpertRelation],
    arguments: &[ExpertArgument],
    grounding_sets: &[GroundingSetView],
    applicability: &[RouteApplicabilityAssessment],
) -> Vec<ReassessmentImpact> {
    let conclusion_by_argument = arguments
        .iter()
        .map(|a| (a.argument_id, a.conclusion_object_id))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut impacts = Vec::new();

    for relation in relations.iter().filter(|r| {
        r.family == RelationFamily::Genealogy
            && matches!(
                r.kind,
                RelationKind::Supersedes | RelationKind::Retracts | RelationKind::Reinterprets
            )
    }) {
        let mut seen_objects = BTreeSet::new();
        let mut queue = std::collections::VecDeque::from([(relation.right_object_id, 1u16)]);
        let mut seen_impacts = BTreeSet::new();

        for argument in arguments
            .iter()
            .filter(|a| a.conclusion_object_id == relation.right_object_id)
        {
            let key = (argument.argument_id, None);
            if seen_impacts.insert(key) {
                impacts.push(ReassessmentImpact {
                    trigger_relation_id: relation.relation_id,
                    relation_kind: relation.kind,
                    revision_object_id: relation.left_object_id,
                    prior_object_id: relation.right_object_id,
                    argument_id: argument.argument_id,
                    grounding_set_id: None,
                    conclusion_object_id: argument.conclusion_object_id,
                    impact_kind: ReassessmentImpactKind::Conclusion,
                    depth: 0,
                    latest_review: None,
                });
            }
        }

        while let Some((changed_object, depth)) = queue.pop_front() {
            if !seen_objects.insert(changed_object) {
                continue;
            }
            for set in grounding_sets {
                let ground_changed = set
                    .members
                    .iter()
                    .any(|member| member.ground_object_id == Some(changed_object));
                let condition_changed = applicability.iter().any(|condition| {
                    condition.argument_id == set.argument_id
                        && condition.grounding_set_id == set.grounding_set_id
                        && condition.condition_object_id == changed_object
                });
                if !ground_changed && !condition_changed {
                    continue;
                }
                let Some(&conclusion) = conclusion_by_argument.get(&set.argument_id) else {
                    continue;
                };
                let impact_kind = if ground_changed {
                    ReassessmentImpactKind::GroundMember
                } else {
                    ReassessmentImpactKind::ApplicabilityCondition
                };
                let key = (set.argument_id, Some(set.grounding_set_id));
                if seen_impacts.insert(key) {
                    impacts.push(ReassessmentImpact {
                        trigger_relation_id: relation.relation_id,
                        relation_kind: relation.kind,
                        revision_object_id: relation.left_object_id,
                        prior_object_id: relation.right_object_id,
                        argument_id: set.argument_id,
                        grounding_set_id: Some(set.grounding_set_id),
                        conclusion_object_id: conclusion,
                        impact_kind,
                        depth,
                        latest_review: None,
                    });
                }
                if !seen_objects.contains(&conclusion) {
                    queue.push_back((conclusion, depth.saturating_add(1)));
                }
            }
        }
    }
    impacts.sort_by_key(|impact| {
        (
            impact.trigger_relation_id,
            impact.depth,
            impact.argument_id,
            impact.grounding_set_id,
        )
    });
    impacts
}

fn grounding_cycle_diagnostics(
    arguments: &[ExpertArgument],
    grounding_sets: &[GroundingSetView],
) -> Vec<GroundingCycleDiagnostic> {
    let conclusion_by_argument = arguments
        .iter()
        .map(|a| (a.argument_id, a.conclusion_object_id))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut adjacency = std::collections::BTreeMap::<Uuid, BTreeSet<Uuid>>::new();
    let mut nodes = BTreeSet::new();
    for set in grounding_sets
        .iter()
        .filter(|s| s.effect == GroundingEffect::Supports)
    {
        let Some(&conclusion) = conclusion_by_argument.get(&set.argument_id) else {
            continue;
        };
        nodes.insert(conclusion);
        for member in &set.members {
            if member.role == GroundingMemberRole::Context {
                continue;
            }
            if let Some(ground) = member.ground_object_id {
                nodes.insert(ground);
                adjacency.entry(ground).or_default().insert(conclusion);
            }
        }
    }

    fn reachable(
        start: Uuid,
        target: Uuid,
        adjacency: &std::collections::BTreeMap<Uuid, BTreeSet<Uuid>>,
    ) -> bool {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if !seen.insert(node) {
                continue;
            }
            if let Some(next) = adjacency.get(&node) {
                stack.extend(next.iter().copied());
            }
        }
        false
    }

    let mut remaining = nodes;
    let mut diagnostics = Vec::new();
    while let Some(seed) = remaining.iter().next().copied() {
        let component = remaining
            .iter()
            .copied()
            .filter(|candidate| {
                reachable(seed, *candidate, &adjacency) && reachable(*candidate, seed, &adjacency)
            })
            .collect::<BTreeSet<_>>();
        for node in &component {
            remaining.remove(node);
        }
        let self_loop = component.len() == 1
            && adjacency
                .get(&seed)
                .is_some_and(|targets| targets.contains(&seed));
        if component.len() < 2 && !self_loop {
            continue;
        }

        let mut argument_ids = BTreeSet::new();
        let mut external_grounding_routes = BTreeSet::new();
        let mut external_only_grounding_routes = BTreeSet::new();
        for set in grounding_sets
            .iter()
            .filter(|s| s.effect == GroundingEffect::Supports)
        {
            let Some(&conclusion) = conclusion_by_argument.get(&set.argument_id) else {
                continue;
            };
            if !component.contains(&conclusion) {
                continue;
            }
            let participates = set.members.iter().any(|m| {
                m.role != GroundingMemberRole::Context
                    && m.ground_object_id
                        .is_some_and(|ground| component.contains(&ground))
            });
            if participates {
                argument_ids.insert(set.argument_id);
            }
            if set.members.iter().any(|m| {
                m.role != GroundingMemberRole::Context
                    && (m.evidence_id.is_some()
                        || m.ground_object_id
                            .is_some_and(|ground| !component.contains(&ground)))
            }) {
                let route = GroundingRouteRef {
                    argument_id: set.argument_id,
                    grounding_set_id: set.grounding_set_id,
                };
                external_grounding_routes.insert(route);
                if !participates {
                    external_only_grounding_routes.insert(route);
                }
            }
        }
        let has_external_anchor = !external_grounding_routes.is_empty();
        diagnostics.push(GroundingCycleDiagnostic {
            object_ids: component.into_iter().collect(),
            argument_ids: argument_ids.into_iter().collect(),
            external_grounding_routes: external_grounding_routes.into_iter().collect(),
            external_only_grounding_routes: external_only_grounding_routes.into_iter().collect(),
            has_external_anchor,
            unanchored_recursive_support: !has_external_anchor,
        });
    }
    diagnostics.sort_by_key(|d| d.object_ids.first().copied());
    diagnostics
}

fn reassessment_attention<'a>(
    impacts: impl Iterator<Item = &'a ReassessmentImpact>,
) -> ReassessmentAttentionSummary {
    let mut summary = ReassessmentAttentionSummary::default();
    for impact in impacts {
        summary.record(
            impact
                .latest_review
                .as_ref()
                .map(|review| review.disposition),
        );
    }
    summary
}

/// Deterministic structural reasoning over the expert-authored graph.
///
/// Routes stay separate: support is never arithmetically cancelled by weakening or challenge,
/// review dispositions are not truth values, and recursive support is exposed as a warning rather
/// than counted as additional corroboration.
fn reasoning_diagnostics(
    arguments: &[ExpertArgument],
    grounding_sets: &[GroundingSetView],
    reassessment_impacts: &[ReassessmentImpact],
    grounding_cycles: &[GroundingCycleDiagnostic],
    relations: &[ExpertRelation],
    nodes: &[ExpertNode],
) -> Vec<ConclusionReasoningDiagnostic> {
    let mut argument_ids_by_conclusion = BTreeMap::<Uuid, Vec<Uuid>>::new();
    let mut conclusion_by_argument = BTreeMap::new();
    for argument in arguments {
        argument_ids_by_conclusion
            .entry(argument.conclusion_object_id)
            .or_default()
            .push(argument.argument_id);
        conclusion_by_argument.insert(argument.argument_id, argument.conclusion_object_id);
    }
    let mut contradictions_by_left = BTreeMap::<Uuid, Vec<&ExpertRelation>>::new();
    for relation in relations.iter().filter(|r| r.kind == RelationKind::Contradicts) {
        contradictions_by_left.entry(relation.left_object_id).or_default().push(relation);
    }
    let gap_ids: BTreeSet<_> = nodes
        .iter()
        .filter(|node| node.kind == ExpertNodeKind::InformationGap)
        .map(|node| node.object_id)
        .collect();

    let mut diagnostics = Vec::with_capacity(argument_ids_by_conclusion.len());
    for (conclusion_object_id, mut argument_ids) in argument_ids_by_conclusion {
        argument_ids.sort();
        let argument_id_set = argument_ids.iter().copied().collect::<BTreeSet<_>>();
        let mut routes = Vec::new();
        let mut ground_routes =
            BTreeMap::<(Option<Uuid>, Option<Uuid>), BTreeSet<GroundingRouteRef>>::new();
        let mut supporting_ground_routes = BTreeMap::<Uuid, BTreeSet<GroundingRouteRef>>::new();
        for set in grounding_sets
            .iter()
            .filter(|set| argument_id_set.contains(&set.argument_id))
        {
            let route_ref = GroundingRouteRef {
                argument_id: set.argument_id,
                grounding_set_id: set.grounding_set_id,
            };
            for member in set
                .members
                .iter()
                .filter(|m| m.role != GroundingMemberRole::Context)
            {
                if member.ground_object_id.is_some() || member.evidence_id.is_some() {
                    ground_routes
                        .entry((member.ground_object_id, member.evidence_id))
                        .or_default()
                        .insert(route_ref);
                }
                if set.effect == GroundingEffect::Supports && let Some(id) = member.ground_object_id {
                    supporting_ground_routes.entry(id).or_default().insert(route_ref);
                }
            }
            let cycle_matches = grounding_cycles.iter().filter(|cycle| {
                if set.effect != GroundingEffect::Supports
                    || !cycle.argument_ids.contains(&set.argument_id)
                {
                    return false;
                }
                let Some(conclusion) = conclusion_by_argument.get(&set.argument_id) else {
                    return false;
                };
                cycle.object_ids.contains(conclusion)
                    && set.members.iter().any(|member| {
                        member.role != GroundingMemberRole::Context
                            && member
                                .ground_object_id
                                .is_some_and(|ground| cycle.object_ids.contains(&ground))
                    })
            });
            let cycle_matches = cycle_matches.collect::<Vec<_>>();
            let route_impacts = reassessment_impacts.iter().filter(|impact| {
                impact.argument_id == set.argument_id
                    && impact.grounding_set_id == Some(set.grounding_set_id)
            });
            routes.push(ReasoningRouteDiagnostic {
                argument_id: set.argument_id,
                grounding_set_id: set.grounding_set_id,
                effect: set.effect,
                adequacy: set.adequacy,
                member_count: set.members.len(),
                non_context_member_count: set
                    .members
                    .iter()
                    .filter(|m| m.role != GroundingMemberRole::Context)
                    .count(),
                applicability_conditions: Vec::new(),
                reassessment: reassessment_attention(route_impacts),
                assessment_reassessment: AssessmentReassessmentAttention::default(),
                interpretation_reassessment: AssessmentReassessmentAttention::default(),
                recursive_support_cycle: !cycle_matches.is_empty(),
                unanchored_recursive_support: cycle_matches
                    .iter()
                    .any(|cycle| cycle.unanchored_recursive_support),
            });
        }
        routes.sort_by_key(|route| (route.argument_id, route.grounding_set_id));
        let has_supporting_routes = routes.iter().any(|route| {
            route.non_context_member_count > 0 && route.effect == GroundingEffect::Supports
        });
        let has_opposing_routes = routes.iter().any(|route| {
            route.non_context_member_count > 0
                && matches!(
                    route.effect,
                    GroundingEffect::Weakens | GroundingEffect::Challenges
                )
        });
        let conclusion_reassessment =
            reassessment_attention(reassessment_impacts.iter().filter(|impact| {
                argument_id_set.contains(&impact.argument_id) && impact.grounding_set_id.is_none()
            }));
        let mut contradictory_supports = Vec::new();
        for (left, left_routes) in &supporting_ground_routes {
            for relation in contradictions_by_left.get(left).into_iter().flatten() {
                if let Some(right_routes) = supporting_ground_routes.get(&relation.right_object_id) {
                    contradictory_supports.push(ContradictorySupportDiagnostic {
                        relation_id: relation.relation_id,
                        left_object_id: *left,
                        right_object_id: relation.right_object_id,
                        left_supporting_routes: left_routes.iter().copied().collect(),
                        right_supporting_routes: right_routes.iter().copied().collect(),
                    });
                }
            }
        }
        contradictory_supports.sort_by_key(|candidate| candidate.relation_id);
        let information_gap_supports = supporting_ground_routes
            .iter()
            .filter(|(id, _)| gap_ids.contains(*id))
            .map(|(id, routes)| InformationGapSupportDiagnostic {
                gap_object_id: *id,
                supporting_routes: routes.iter().copied().collect(),
            })
            .collect();
        diagnostics.push(ConclusionReasoningDiagnostic {
            conclusion_object_id,
            argument_ids,
            routes,
            shared_grounds: ground_routes
                .into_iter()
                .filter(|(_, routes)| routes.len() > 1)
                .map(
                    |((ground_object_id, evidence_id), routes)| SharedGroundDiagnostic {
                        ground_object_id,
                        evidence_id,
                        routes: routes.into_iter().collect(),
                    },
                )
                .collect(),
            contradictory_supports,
            information_gap_supports,
            direct_evidence_lineage: DirectEvidenceLineageDiagnostic::default(),
            has_supporting_routes,
            has_opposing_routes,
            has_mixed_directional_grounding: has_supporting_routes && has_opposing_routes,
            conclusion_reassessment,
        });
    }
    diagnostics.sort_by_key(|diagnostic| diagnostic.conclusion_object_id);
    diagnostics
}

/// Follow only explicitly authored, non-contextual grounding dependencies.
///
/// The fixed point records the shortest structural depth at which an object is
/// reachable.  Route directions remain attached to their individual routes;
/// SUPPORTS followed by WEAKENS is deliberately not collapsed into a derived
/// sign or score.  Joint grounding also remains visible through the member
/// count, so a what-if assumption is never presented as sufficient merely
/// because it participates in a route.
fn authored_object_impacts(
    nodes: &[ExpertNode],
    relations: &[ExpertRelation],
    arguments: &[ExpertArgument],
    grounding_sets: &[GroundingSetView],
) -> Vec<AuthoredObjectImpactAnalysis> {
    let conclusions = arguments
        .iter()
        .map(|argument| (argument.argument_id, argument.conclusion_object_id))
        .collect::<BTreeMap<_, _>>();
    let sources = nodes.iter().filter(|node| {
        matches!(
            node.kind,
            ExpertNodeKind::Hypothesis | ExpertNodeKind::Assumption
        )
    });
    let mut analyses = Vec::new();

    for source in sources {
        let mut reachable_depth = BTreeMap::from([(source.object_id, 0_u16)]);
        // A shortest-depth fixed point over object -> argument conclusion edges.
        // At most one new minimum per object is needed; the extra pass confirms
        // stability in graphs containing cycles.
        for _ in 0..=nodes.len() {
            let mut changed = false;
            for set in grounding_sets {
                let Some(conclusion) = conclusions.get(&set.argument_id).copied() else {
                    continue;
                };
                let Some(minimum_ground_depth) = set
                    .members
                    .iter()
                    .filter(|member| member.role != GroundingMemberRole::Context)
                    .filter_map(|member| member.ground_object_id)
                    .filter_map(|ground| reachable_depth.get(&ground).copied())
                    .min()
                else {
                    continue;
                };
                let candidate = minimum_ground_depth.saturating_add(1);
                if reachable_depth
                    .get(&conclusion)
                    .is_none_or(|current| candidate < *current)
                {
                    reachable_depth.insert(conclusion, candidate);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut route_impacts = Vec::new();
        for set in grounding_sets {
            let Some(conclusion_object_id) = conclusions.get(&set.argument_id).copied() else {
                continue;
            };
            let mut reachable_ground_object_ids = set
                .members
                .iter()
                .filter(|member| member.role != GroundingMemberRole::Context)
                .filter_map(|member| member.ground_object_id)
                .filter(|ground| reachable_depth.contains_key(ground))
                .collect::<Vec<_>>();
            reachable_ground_object_ids.sort();
            reachable_ground_object_ids.dedup();
            let Some(minimum_ground_depth) = reachable_ground_object_ids
                .iter()
                .filter_map(|ground| reachable_depth.get(ground).copied())
                .min()
            else {
                continue;
            };
            route_impacts.push(AuthoredObjectImpactRoute {
                argument_id: set.argument_id,
                grounding_set_id: set.grounding_set_id,
                conclusion_object_id,
                effect: set.effect,
                adequacy: set.adequacy,
                depth: minimum_ground_depth.saturating_add(1),
                source_is_direct_ground: reachable_ground_object_ids.contains(&source.object_id),
                reachable_ground_object_ids,
                non_context_member_count: set
                    .members
                    .iter()
                    .filter(|member| member.role != GroundingMemberRole::Context)
                    .count(),
                unreached_parallel_route_count: 0,
            });
        }
        let reached_routes = route_impacts
            .iter()
            .map(|impact| (impact.argument_id, impact.grounding_set_id))
            .collect::<BTreeSet<_>>();
        for impact in &mut route_impacts {
            impact.unreached_parallel_route_count = grounding_sets
                .iter()
                .filter(|set| {
                    conclusions.get(&set.argument_id).copied()
                        == Some(impact.conclusion_object_id)
                        && !reached_routes.contains(&(set.argument_id, set.grounding_set_id))
                })
                .count();
        }
        route_impacts.sort_by_key(|impact| {
            (
                impact.depth,
                impact.conclusion_object_id,
                impact.argument_id,
                impact.grounding_set_id,
            )
        });

        let mut contradicted_object_ids = relations
            .iter()
            .filter(|relation| relation.kind == RelationKind::Contradicts)
            .filter_map(|relation| {
                if relation.left_object_id == source.object_id {
                    Some(relation.right_object_id)
                } else if relation.right_object_id == source.object_id {
                    Some(relation.left_object_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        contradicted_object_ids.sort();
        contradicted_object_ids.dedup();

        let mut tested_by_question_ids = relations
            .iter()
            .filter(|relation| {
                relation.kind == RelationKind::TestsHypothesis
                    && relation.right_object_id == source.object_id
            })
            .map(|relation| relation.left_object_id)
            .collect::<Vec<_>>();
        tested_by_question_ids.sort();
        tested_by_question_ids.dedup();

        let mut reached_conclusion_object_ids = route_impacts
            .iter()
            .map(|impact| impact.conclusion_object_id)
            .collect::<Vec<_>>();
        reached_conclusion_object_ids.sort();
        reached_conclusion_object_ids.dedup();

        analyses.push(AuthoredObjectImpactAnalysis {
            source_object_id: source.object_id,
            source_kind: source.kind,
            route_impacts,
            reached_conclusion_object_ids,
            contradicted_object_ids,
            tested_by_question_ids,
            structural_routes_only: true,
            establishes_truth: false,
            directional_effects_are_aggregated: false,
            treats_assumption_as_fact: false,
        });
    }
    analyses.sort_by_key(|analysis| analysis.source_object_id);
    analyses
}

async fn direct_evidence_lineage(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    sets: &[GroundingSetView],
    diagnostics: &mut [ConclusionReasoningDiagnostic],
) -> Result<(), WorkbenchError> {
    use crate::grounded_lineage_store::{self, StoredLineageError};
    use crate::provenance_clusters::ClusterError;
    let mut by_argument = BTreeMap::<Uuid, Vec<&GroundingSetView>>::new();
    for set in sets {
        by_argument.entry(set.argument_id).or_default().push(set);
    }
    for diagnostic in diagnostics {
        let mut by_evidence = BTreeMap::<Uuid, BTreeSet<GroundingRouteRef>>::new();
        for arg in &diagnostic.argument_ids {
            for set in by_argument.get(arg).into_iter().flatten() {
                for member in &set.members {
                    if member.role == GroundingMemberRole::Evidence
                        && let Some(id) = member.evidence_id
                    {
                        by_evidence
                            .entry(id)
                            .or_default()
                            .insert(GroundingRouteRef {
                                argument_id: set.argument_id,
                                grounding_set_id: set.grounding_set_id,
                            });
                    }
                }
            }
        }
        let ids = by_evidence.keys().copied().collect::<Vec<_>>();
        if ids.is_empty() {
            continue;
        }
        let report = &mut diagnostic.direct_evidence_lineage;
        report.selected_evidence_count = ids.len();
        match grounded_lineage_store::from_database(tx, workspace, &ids).await {
            Ok(clustered) => {
                report.status = EvidenceLineageStatus::RecordedLineageResolved;
                report.components = clustered
                    .clusters
                    .into_iter()
                    .map(|cluster| {
                        let routes = cluster
                            .selected_evidence_ids
                            .iter()
                            .flat_map(|id| by_evidence[id].iter().copied())
                            .collect::<BTreeSet<_>>();
                        EvidenceDependenceDiagnostic {
                            evidence_ids: cluster.selected_evidence_ids,
                            routes: routes.into_iter().collect(),
                        }
                    })
                    .collect();
            }
            Err(StoredLineageError::Database(err)) => return Err(err.into()),
            Err(err) => {
                report.status = match err {
                    StoredLineageError::Cluster(ClusterError::Missing) => {
                        EvidenceLineageStatus::MissingOrInvisible
                    }
                    StoredLineageError::Cluster(ClusterError::Withdrawn) => {
                        EvidenceLineageStatus::Withdrawn
                    }
                    StoredLineageError::Cluster(ClusterError::Cycle) => {
                        EvidenceLineageStatus::Cycle
                    }
                    _ => EvidenceLineageStatus::InvalidOrLimit,
                }
            }
        }
    }
    Ok(())
}

pub async fn snapshot(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<WorkbenchSnapshot, WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let node_rows: Vec<(Uuid, String, String, String, Uuid)> = sqlx::query_as(
        "SELECT object_id,kind,text,rationale,created_by FROM ( \
           SELECT p.proposition_id AS object_id,'PROPOSITION'::text AS kind,p.statement_text AS text,sp.rationale,o.created_by_principal_id AS created_by,sp.created_at \
             FROM niccolo.epistemic_situation_propositions sp \
             JOIN niccolo.epistemic_propositions p ON p.workspace_id=sp.workspace_id AND p.proposition_id=sp.proposition_id \
             JOIN niccolo.epistemic_objects o ON o.workspace_id=p.workspace_id AND o.object_id=p.proposition_id \
            WHERE sp.workspace_id=$1 AND sp.situation_id=$2 \
           UNION ALL \
           SELECT h.hypothesis_id AS object_id,'HYPOTHESIS'::text AS kind,h.statement_text AS text,h.rationale,o.created_by_principal_id AS created_by,h.created_at \
             FROM niccolo.epistemic_hypotheses h JOIN niccolo.epistemic_objects o ON o.workspace_id=h.workspace_id AND o.object_id=h.hypothesis_id \
            WHERE h.workspace_id=$1 AND h.situation_id=$2 \
           UNION ALL \
           SELECT a.assumption_id,'ASSUMPTION',a.statement_text,a.rationale,o.created_by_principal_id,a.created_at \
             FROM niccolo.epistemic_assumptions a JOIN niccolo.epistemic_objects o ON o.workspace_id=a.workspace_id AND o.object_id=a.assumption_id \
            WHERE a.workspace_id=$1 AND a.situation_id=$2 \
           UNION ALL \
           SELECT g.gap_id,'INFORMATION_GAP',g.description,g.rationale,o.created_by_principal_id,g.created_at \
             FROM niccolo.epistemic_information_gaps g JOIN niccolo.epistemic_objects o ON o.workspace_id=g.workspace_id AND o.object_id=g.gap_id \
            WHERE g.workspace_id=$1 AND g.situation_id=$2 \
           UNION ALL \
           SELECT q.question_id,'QUESTION',q.question_text,q.rationale,o.created_by_principal_id,q.created_at \
             FROM niccolo.epistemic_questions q JOIN niccolo.epistemic_objects o ON o.workspace_id=q.workspace_id AND o.object_id=q.question_id \
            WHERE q.workspace_id=$1 AND q.situation_id=$2 \
           UNION ALL \
           SELECT f.finding_id,'FINDING',f.statement_text,f.rationale,o.created_by_principal_id,f.created_at \
             FROM niccolo.epistemic_findings f JOIN niccolo.epistemic_objects o ON o.workspace_id=f.workspace_id AND o.object_id=f.finding_id \
            WHERE f.workspace_id=$1 AND f.situation_id=$2 \
         ) q ORDER BY created_at,object_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut nodes = Vec::with_capacity(node_rows.len());
    for (object_id, kind, text, rationale, created_by) in node_rows {
        nodes.push(ExpertNode {
            object_id,
            kind: node_kind_from_db(&kind)?,
            situation_id: situation,
            text,
            rationale,
            created_by,
            replayed: false,
        });
    }
    let evidence_rows: Vec<WorkbenchEvidenceRow> = sqlx::query_as(
        "SELECT f.evidence_id,f.document_id,f.ordinal,f.speaker_label,e.content,e.source_class,e.source_locator,d.locator \
         FROM niccolo.situation_sources ss \
         JOIN niccolo.source_fragments f ON f.workspace_id=ss.workspace_id AND f.document_id=ss.document_id \
         JOIN niccolo.evidence e ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id \
         JOIN niccolo.source_documents d ON d.workspace_id=f.workspace_id AND d.document_id=f.document_id \
         WHERE ss.workspace_id=$1 AND ss.situation_id=$2 AND e.admissible \
         ORDER BY d.created_at,f.document_id,f.ordinal LIMIT 257",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let evidence_truncated = evidence_rows.len() > 256;
    let evidence = evidence_rows
        .into_iter()
        .take(256)
        .map(
            |(
                evidence_id,
                document_id,
                ordinal,
                speaker_label,
                content,
                source_class,
                source_locator,
                document_locator,
            )| WorkbenchEvidence {
                evidence_id,
                document_id,
                ordinal,
                speaker_label,
                content,
                source_class,
                source_locator,
                document_locator,
            },
        )
        .collect();

    let relation_rows: Vec<(Uuid, String, String, Uuid, Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT relation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_relations WHERE workspace_id=$1 AND situation_id=$2 \
         ORDER BY created_at,relation_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut relations = Vec::with_capacity(relation_rows.len());
    for (relation_id, family, kind, left_object_id, right_object_id, rationale, recorded_by) in
        relation_rows
    {
        relations.push(ExpertRelation {
            relation_id,
            situation_id: situation,
            family: relation_family_from_db(&family)?,
            kind: relation_kind_from_db(&kind)?,
            left_object_id,
            right_object_id,
            rationale,
            recorded_by,
            replayed: false,
        });
    }
    let argument_rows: Vec<(Uuid, Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT a.argument_id,a.conclusion_object_id,a.argument_mode,a.rationale,o.created_by_principal_id \
         FROM niccolo.epistemic_arguments a JOIN niccolo.epistemic_objects o \
           ON o.workspace_id=a.workspace_id AND o.object_id=a.argument_id \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 ORDER BY a.created_at,a.argument_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut arguments = Vec::with_capacity(argument_rows.len());
    for (argument_id, conclusion_object_id, mode, rationale, created_by) in argument_rows {
        arguments.push(ExpertArgument {
            argument_id,
            situation_id: situation,
            conclusion_object_id,
            mode: argument_mode_from_db(&mode)?,
            rationale,
            created_by,
            replayed: false,
        });
    }
    let set_rows: Vec<(Uuid, Uuid, String, String, String, Uuid)> = sqlx::query_as(
        "SELECT s.argument_id,s.grounding_set_id,s.effect,s.adequacy,s.rationale,s.recorded_by_principal_id \
         FROM niccolo.epistemic_grounding_sets s JOIN niccolo.epistemic_arguments a \
           ON a.workspace_id=s.workspace_id AND a.argument_id=s.argument_id \
         WHERE s.workspace_id=$1 AND a.situation_id=$2 ORDER BY s.created_at,s.grounding_set_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let member_rows: Vec<GroundingMemberSnapshotRow> = sqlx::query_as(
            "SELECT m.argument_id,m.grounding_set_id,m.member_id,m.member_role,m.ground_object_id,m.evidence_id,m.rationale,m.recorded_by_principal_id \
             FROM niccolo.epistemic_grounding_members m JOIN niccolo.epistemic_arguments a \
               ON a.workspace_id=m.workspace_id AND a.argument_id=m.argument_id \
             WHERE m.workspace_id=$1 AND a.situation_id=$2 ORDER BY m.created_at,m.member_id",
        )
        .bind(workspace)
        .bind(situation)
        .fetch_all(&mut *tx)
        .await?;
    let mut grouped: BTreeMap<(Uuid, Uuid), Vec<GroundingMemberView>> = BTreeMap::new();
    for (
        argument_id,
        grounding_set_id,
        member_id,
        role,
        ground_object_id,
        evidence_id,
        rationale,
        recorded_by,
    ) in member_rows
    {
        grouped
            .entry((argument_id, grounding_set_id))
            .or_default()
            .push(GroundingMemberView {
                member_id,
                role: member_role_from_db(&role)?,
                ground_object_id,
                evidence_id,
                rationale,
                recorded_by,
            });
    }
    let mut grounding_sets = Vec::with_capacity(set_rows.len());
    for (argument_id, grounding_set_id, effect, adequacy, rationale, recorded_by) in set_rows {
        grounding_sets.push(GroundingSetView {
            argument_id,
            grounding_set_id,
            effect: grounding_effect_from_db(&effect)?,
            adequacy: grounding_adequacy_from_db(&adequacy)?,
            rationale,
            recorded_by,
            members: grouped
                .remove(&(argument_id, grounding_set_id))
                .unwrap_or_default(),
            replayed: false,
        });
    }
    let applicability = latest_route_applicability(&mut tx, workspace, situation).await?;
    let assessments = assessment::list_in_snapshot(&mut tx, workspace, situation, None).await?;
    let question_decisions = inquiry_decision::list_in_snapshot(&mut tx, workspace, situation).await?;
    let question_decision_basis_changes = inquiry_decision_review::changes_in_snapshot(&mut tx, workspace, situation, &question_decisions, &assessments).await?;
    let action_recommendations = action_recommendation::list_in_snapshot(&mut tx, workspace, situation).await?;
    let action_decisions = action_decision::list_in_snapshot(&mut tx, workspace, situation).await?;
    let (intervention_records, intervention_causal_assessments) = intervention::list_in_snapshot(&mut tx, workspace, situation).await?;
    let actor_epistemic_states = actor_state::list_in_snapshot(&mut tx, workspace, situation).await?;
    let normative_assessments = normative_assessment::list_in_snapshot(&mut tx, workspace, situation).await?;
    let actor_epistemic_acts = actor_act::list_in_snapshot(&mut tx, workspace, situation).await?;
    let question_responses =
        inquiry_response::list_in_snapshot(&mut tx, workspace, situation, None).await?;
    let response_interpretations =
        inquiry_response::interpretations_in_snapshot(&mut tx, workspace, situation).await?;
    let interpretation_dependencies = interpretation_dependency::views(
        &mut tx,
        workspace,
        situation,
        &response_interpretations,
        &arguments,
        &grounding_sets,
        &applicability,
    )
    .await?;
    let assessment_dependencies = assessment::dependency_views(
        &mut tx,
        workspace,
        situation,
        &assessments,
        &arguments,
        &grounding_sets,
        &applicability,
    )
    .await?;
    let mut reassessment_impacts =
        selective_reassessment_impacts(&relations, &arguments, &grounding_sets, &applicability);
    let recommendation_basis_impacts = action_recommendation::basis_impacts(&action_recommendations, &relations, &reassessment_impacts);
    let review_rows: Vec<ReassessmentReviewRow> = sqlx::query_as(
        "SELECT trigger_relation_id,argument_id,grounding_set_id,review_event_id,disposition,note,recorded_by_principal_id \
         FROM niccolo.epistemic_reassessment_review_events \
         WHERE workspace_id=$1 AND situation_id=$2 ORDER BY event_no",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut latest_reviews = BTreeMap::new();
    for (
        trigger_relation_id,
        argument_id,
        grounding_set_id,
        review_event_id,
        disposition,
        note,
        recorded_by,
    ) in review_rows
    {
        latest_reviews.insert(
            (trigger_relation_id, argument_id, grounding_set_id),
            ReassessmentReview {
                review_event_id,
                trigger_relation_id,
                argument_id,
                grounding_set_id,
                disposition: ReassessmentDisposition::from_db(&disposition)?,
                note,
                recorded_by,
                replayed: false,
            },
        );
    }
    for impact in &mut reassessment_impacts {
        impact.latest_review = latest_reviews
            .get(&(
                impact.trigger_relation_id,
                impact.argument_id,
                impact.grounding_set_id,
            ))
            .cloned();
    }
    let grounding_cycles = grounding_cycle_diagnostics(&arguments, &grounding_sets);
    let mut reasoning_diagnostics = reasoning_diagnostics(
        &arguments,
        &grounding_sets,
        &reassessment_impacts,
        &grounding_cycles,
        &relations,
        &nodes,
    );
    let mut conditions_by_route =
        BTreeMap::<(Uuid, Uuid), Vec<RouteApplicabilityAssessment>>::new();
    for condition in applicability {
        conditions_by_route
            .entry((condition.argument_id, condition.grounding_set_id))
            .or_default()
            .push(condition);
    }
    let mut assessment_attention_by_route =
        BTreeMap::<GroundingRouteRef, AssessmentReassessmentAttention>::new();
    for dependency in assessment_dependencies
        .iter()
        .filter(|d| d.assessment_revised)
    {
        let route = GroundingRouteRef {
            argument_id: dependency.dependency.input.argument_id,
            grounding_set_id: dependency.dependency.input.grounding_set_id,
        };
        assessment_attention_by_route
            .entry(route)
            .or_default()
            .directly_relying
            .record(
                dependency
                    .latest_review
                    .as_ref()
                    .map(|r| r.input.disposition),
            );
        for downstream in &dependency.downstream_reviews {
            assessment_attention_by_route
                .entry(downstream.route)
                .or_default()
                .downstream
                .record(
                    downstream
                        .latest_review
                        .as_ref()
                        .map(|r| r.input.disposition),
                );
        }
    }
    let mut interpretation_attention_by_route =
        BTreeMap::<GroundingRouteRef, AssessmentReassessmentAttention>::new();
    for dependency in interpretation_dependencies
        .iter()
        .filter(|d| d.interpretation_revised || d.meaning_confirmation_changed)
    {
        let route = GroundingRouteRef {
            argument_id: dependency.dependency.input.argument_id,
            grounding_set_id: dependency.dependency.input.grounding_set_id,
        };
        interpretation_attention_by_route
            .entry(route)
            .or_default()
            .directly_relying
            .record(
                dependency
                    .latest_review
                    .as_ref()
                    .map(|r| r.input.disposition),
            );
        for downstream in &dependency.downstream_reviews {
            interpretation_attention_by_route
                .entry(downstream.route)
                .or_default()
                .downstream
                .record(
                    downstream
                        .latest_review
                        .as_ref()
                        .map(|r| r.input.disposition),
                );
        }
    }
    for route in reasoning_diagnostics
        .iter_mut()
        .flat_map(|diagnostic| &mut diagnostic.routes)
    {
        route.applicability_conditions = conditions_by_route
            .remove(&(route.argument_id, route.grounding_set_id))
            .unwrap_or_default();
        route.assessment_reassessment = assessment_attention_by_route
            .remove(&GroundingRouteRef {
                argument_id: route.argument_id,
                grounding_set_id: route.grounding_set_id,
            })
            .unwrap_or_default();
        route.interpretation_reassessment = interpretation_attention_by_route
            .remove(&GroundingRouteRef {
                argument_id: route.argument_id,
                grounding_set_id: route.grounding_set_id,
            })
            .unwrap_or_default();
    }
    direct_evidence_lineage(
        &mut tx,
        workspace,
        &grounding_sets,
        &mut reasoning_diagnostics,
    )
    .await?;
    let draft_sources = sqlx::query_as(
        "SELECT a.draft_id,d.provider,d.model,a.object_ids,a.relation_ids
         FROM niccolo.investigator_input_draft_adoptions a
         JOIN niccolo.investigator_input_drafts d USING(workspace_id,situation_id,draft_id)
         WHERE a.workspace_id=$1 AND a.situation_id=$2
         ORDER BY a.created_at,a.draft_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(draft_id, provider, model, object_ids, relation_ids)| WorkbenchDraftSource {
        draft_id,
        provider,
        model,
        object_ids,
        relation_ids,
    })
    .collect();
    let artifact_provenance = sqlx::query_as(
        "SELECT p.object_id,r.artifact_id,p.representation_id,a.label,r.scope_locator,
                r.epistemic_qualification,p.rationale,p.recorded_by
         FROM niccolo.workbench_artifact_provenance p
         JOIN niccolo.situation_artifact_representations r USING(workspace_id,situation_id,representation_id)
         JOIN niccolo.artifacts a ON a.workspace_id=r.workspace_id AND a.artifact_id=r.artifact_id
         WHERE p.workspace_id=$1 AND p.situation_id=$2
         ORDER BY p.recorded_at,p.object_id,p.representation_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(object_id, artifact_id, representation_id, artifact_label, scope_locator, qualification, rationale, recorded_by)| ArtifactProvenance {
        object_id,
        artifact_id,
        representation_id,
        artifact_label,
        scope_locator,
        qualification,
        rationale,
        recorded_by,
    })
    .collect();
    tx.commit().await?;
    let question_inquiries = question_inquiries(&nodes, &relations, &assessments);
    let authored_object_impacts =
        authored_object_impacts(&nodes, &relations, &arguments, &grounding_sets);
    Ok(WorkbenchSnapshot {
        situation_id: situation,
        nodes,
        evidence,
        evidence_truncated,
        relations,
        arguments,
        grounding_sets,
        reassessment_impacts,
        grounding_cycles,
        reasoning_diagnostics,
        authored_object_impacts,
        question_inquiries,
        question_decisions,
        question_decision_basis_changes,
        action_recommendations,
        recommendation_basis_impacts,
        action_decisions,
        intervention_records,
        intervention_causal_assessments,
        actor_epistemic_states,
        normative_assessments,
        actor_epistemic_acts,
        question_responses,
        response_interpretations,
        interpretation_dependencies,
        assessments,
        assessment_dependencies,
        draft_sources,
        artifact_provenance,
        investigator_free_text_compiler_enabled: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_family_is_not_inferred_from_text() {
        assert_eq!(RelationKind::Contradicts.family(), RelationFamily::Semantic);
        assert_eq!(RelationKind::Causes.family(), RelationFamily::Causal);
        assert!(RelationKind::Contradicts.symmetric());
        assert!(!RelationKind::Causes.symmetric());
    }

    #[test]
    fn store_snapshot_does_not_claim_a_model_is_configured() {
        let value = serde_json::to_value(WorkbenchSnapshot {
            situation_id: Uuid::nil(),
            nodes: vec![],
            evidence: vec![],
            evidence_truncated: false,
            relations: vec![],
            arguments: vec![],
            grounding_sets: vec![],
            reassessment_impacts: vec![],
            grounding_cycles: vec![],
            reasoning_diagnostics: vec![],
            authored_object_impacts: vec![],
            question_inquiries: vec![],
            question_decisions: vec![],
            question_decision_basis_changes: vec![],
            action_recommendations: vec![],
            recommendation_basis_impacts: vec![],
            action_decisions: vec![],
            intervention_records: vec![],
            intervention_causal_assessments: vec![],
            actor_epistemic_states: vec![],
            normative_assessments: vec![],
            actor_epistemic_acts: vec![],
            question_responses: vec![],
            response_interpretations: vec![],
            interpretation_dependencies: vec![],
            assessments: vec![],
            assessment_dependencies: vec![],
            draft_sources: vec![],
            artifact_provenance: vec![],
            investigator_free_text_compiler_enabled: false,
        })
        .unwrap();
        assert_eq!(
            value["investigator_free_text_compiler_enabled"],
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn reassessment_dispositions_are_closed() {
        for disposition in [
            ReassessmentDisposition::Retained,
            ReassessmentDisposition::RevisionRequired,
            ReassessmentDisposition::Deferred,
        ] {
            assert_eq!(
                ReassessmentDisposition::from_db(disposition.db_value()).unwrap(),
                disposition
            );
        }
        assert!(ReassessmentDisposition::from_db("CONFIRMED").is_err());
    }

    #[test]
    fn reassessment_propagates_only_through_explicit_ground_dependencies() {
        let old = Uuid::from_u128(1);
        let revised = Uuid::from_u128(2);
        let middle = Uuid::from_u128(3);
        let downstream = Uuid::from_u128(4);
        let unrelated = Uuid::from_u128(5);
        let arg_middle = Uuid::from_u128(11);
        let arg_downstream = Uuid::from_u128(12);
        let arg_unrelated = Uuid::from_u128(13);
        let relations = vec![ExpertRelation {
            relation_id: Uuid::from_u128(21),
            situation_id: Uuid::nil(),
            family: RelationFamily::Genealogy,
            kind: RelationKind::Supersedes,
            left_object_id: revised,
            right_object_id: old,
            rationale: "revision".into(),
            recorded_by: Uuid::nil(),
            replayed: false,
        }];
        let arguments = vec![
            ExpertArgument {
                argument_id: arg_middle,
                situation_id: Uuid::nil(),
                conclusion_object_id: middle,
                mode: ArgumentMode::Deductive,
                rationale: "middle".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
            ExpertArgument {
                argument_id: arg_downstream,
                situation_id: Uuid::nil(),
                conclusion_object_id: downstream,
                mode: ArgumentMode::Deductive,
                rationale: "downstream".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
            ExpertArgument {
                argument_id: arg_unrelated,
                situation_id: Uuid::nil(),
                conclusion_object_id: unrelated,
                mode: ArgumentMode::Deductive,
                rationale: "unrelated".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
        ];
        let member = |id, ground| GroundingMemberView {
            member_id: id,
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(ground),
            evidence_id: None,
            rationale: "premise".into(),
            recorded_by: Uuid::nil(),
        };
        let sets = vec![
            GroundingSetView {
                argument_id: arg_middle,
                grounding_set_id: Uuid::from_u128(31),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "old -> middle".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(41), old)],
                replayed: false,
            },
            GroundingSetView {
                argument_id: arg_downstream,
                grounding_set_id: Uuid::from_u128(32),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "middle -> downstream".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(42), middle)],
                replayed: false,
            },
            GroundingSetView {
                argument_id: arg_unrelated,
                grounding_set_id: Uuid::from_u128(33),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "independent".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(43), revised)],
                replayed: false,
            },
        ];
        let impacts = selective_reassessment_impacts(&relations, &arguments, &sets, &[]);
        assert_eq!(impacts.len(), 2);
        assert_eq!(impacts[0].argument_id, arg_middle);
        assert_eq!(impacts[0].depth, 1);
        assert_eq!(impacts[1].argument_id, arg_downstream);
        assert_eq!(impacts[1].depth, 2);
        assert!(
            impacts
                .iter()
                .all(|impact| impact.argument_id != arg_unrelated)
        );
    }

    #[test]
    fn support_cycles_are_diagnostic_not_additional_corroboration() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let arg_a = Uuid::from_u128(11);
        let arg_b = Uuid::from_u128(12);
        let arguments = vec![
            ExpertArgument {
                argument_id: arg_a,
                situation_id: Uuid::nil(),
                conclusion_object_id: a,
                mode: ArgumentMode::Deductive,
                rationale: "A".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
            ExpertArgument {
                argument_id: arg_b,
                situation_id: Uuid::nil(),
                conclusion_object_id: b,
                mode: ArgumentMode::Deductive,
                rationale: "B".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
        ];
        let member = |id, ground| GroundingMemberView {
            member_id: id,
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(ground),
            evidence_id: None,
            rationale: "premise".into(),
            recorded_by: Uuid::nil(),
        };
        let sets = vec![
            GroundingSetView {
                argument_id: arg_a,
                grounding_set_id: Uuid::from_u128(21),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "B supports A".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(31), b)],
                replayed: false,
            },
            GroundingSetView {
                argument_id: arg_b,
                grounding_set_id: Uuid::from_u128(22),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "A supports B".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(32), a)],
                replayed: false,
            },
        ];
        let cycles = grounding_cycle_diagnostics(&arguments, &sets);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].object_ids, vec![a, b]);
        assert!(cycles[0].unanchored_recursive_support);
        assert!(!cycles[0].has_external_anchor);
    }

    #[test]
    fn support_cycle_reports_external_anchor_without_erasing_cycle() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let external = Uuid::from_u128(3);
        let arg_a = Uuid::from_u128(11);
        let arg_b = Uuid::from_u128(12);
        let arguments = vec![
            ExpertArgument {
                argument_id: arg_a,
                situation_id: Uuid::nil(),
                conclusion_object_id: a,
                mode: ArgumentMode::Deductive,
                rationale: "A".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
            ExpertArgument {
                argument_id: arg_b,
                situation_id: Uuid::nil(),
                conclusion_object_id: b,
                mode: ArgumentMode::Deductive,
                rationale: "B".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
        ];
        let make = |id, ground| GroundingMemberView {
            member_id: id,
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(ground),
            evidence_id: None,
            rationale: "premise".into(),
            recorded_by: Uuid::nil(),
        };
        let sets = vec![
            GroundingSetView {
                argument_id: arg_a,
                grounding_set_id: Uuid::from_u128(21),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "B and external support A".into(),
                recorded_by: Uuid::nil(),
                members: vec![
                    make(Uuid::from_u128(31), b),
                    make(Uuid::from_u128(33), external),
                ],
                replayed: false,
            },
            GroundingSetView {
                argument_id: arg_b,
                grounding_set_id: Uuid::from_u128(22),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "A supports B".into(),
                recorded_by: Uuid::nil(),
                members: vec![make(Uuid::from_u128(32), a)],
                replayed: false,
            },
        ];
        let cycles = grounding_cycle_diagnostics(&arguments, &sets);
        assert_eq!(cycles.len(), 1);
        assert!(cycles[0].has_external_anchor);
        assert!(!cycles[0].unanchored_recursive_support);
        assert_eq!(
            cycles[0].external_grounding_routes,
            vec![GroundingRouteRef {
                argument_id: arg_a,
                grounding_set_id: Uuid::from_u128(21)
            }]
        );
        assert!(cycles[0].external_only_grounding_routes.is_empty());
        let mut contextual = sets.clone();
        contextual[0].members[1].role = GroundingMemberRole::Context;
        assert!(!grounding_cycle_diagnostics(&arguments, &contextual)[0].has_external_anchor);
        contextual[1].members[0].role = GroundingMemberRole::Context;
        assert!(grounding_cycle_diagnostics(&arguments, &contextual).is_empty());
        let mut alternative = sets[0].clone();
        alternative.grounding_set_id = Uuid::from_u128(23);
        alternative.members.remove(0);
        let mut alternatives = sets.clone();
        alternatives.push(alternative);
        let cycles = grounding_cycle_diagnostics(&arguments, &alternatives);
        assert_eq!(cycles[0].external_grounding_routes.len(), 2);
        assert_eq!(
            cycles[0].external_only_grounding_routes,
            vec![GroundingRouteRef {
                argument_id: arg_a,
                grounding_set_id: Uuid::from_u128(23)
            }]
        );
    }
    #[test]
    fn reasoning_diagnostics_keep_support_and_opposition_separate_without_scoring() {
        let conclusion = Uuid::from_u128(100);
        let premise = Uuid::from_u128(101);
        let counter = Uuid::from_u128(102);
        let support_argument = Uuid::from_u128(110);
        let challenge_argument = Uuid::from_u128(111);
        let support_set = Uuid::from_u128(120);
        let challenge_set = Uuid::from_u128(121);
        let arguments = vec![
            ExpertArgument {
                argument_id: support_argument,
                situation_id: Uuid::nil(),
                conclusion_object_id: conclusion,
                mode: ArgumentMode::Abductive,
                rationale: "support route".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
            ExpertArgument {
                argument_id: challenge_argument,
                situation_id: Uuid::nil(),
                conclusion_object_id: conclusion,
                mode: ArgumentMode::Abductive,
                rationale: "challenge route".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
        ];
        let member = |id, ground| GroundingMemberView {
            member_id: id,
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(ground),
            evidence_id: None,
            rationale: "explicit ground".into(),
            recorded_by: Uuid::nil(),
        };
        let sets = vec![
            GroundingSetView {
                argument_id: support_argument,
                grounding_set_id: support_set,
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Sufficient,
                rationale: "authored support".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(130), premise)],
                replayed: false,
            },
            GroundingSetView {
                argument_id: challenge_argument,
                grounding_set_id: challenge_set,
                effect: GroundingEffect::Challenges,
                adequacy: GroundingAdequacy::Partial,
                rationale: "authored challenge".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(131), counter)],
                replayed: false,
            },
        ];
        let impacts = vec![ReassessmentImpact {
            trigger_relation_id: Uuid::from_u128(140),
            relation_kind: RelationKind::Supersedes,
            revision_object_id: Uuid::from_u128(141),
            prior_object_id: premise,
            argument_id: support_argument,
            grounding_set_id: Some(support_set),
            conclusion_object_id: conclusion,
            impact_kind: ReassessmentImpactKind::GroundMember,
            depth: 1,
            latest_review: None,
        }];

        let diagnostics = reasoning_diagnostics(&arguments, &sets, &impacts, &[], &[], &[]);
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert!(diagnostic.has_supporting_routes);
        assert!(diagnostic.has_opposing_routes);
        assert!(diagnostic.has_mixed_directional_grounding);
        assert_eq!(diagnostic.routes.len(), 2);
        assert_eq!(diagnostic.routes[0].effect, GroundingEffect::Supports);
        assert_eq!(diagnostic.routes[0].reassessment.unreviewed, 1);
        assert_eq!(diagnostic.routes[1].effect, GroundingEffect::Challenges);
        assert_eq!(diagnostic.routes[1].reassessment.impacted, 0);
        assert!(diagnostic.shared_grounds.is_empty());
        let mut shared = sets.clone();
        shared[1].members[0].ground_object_id = Some(premise);
        shared[1].grounding_set_id = support_set;
        let diagnostics = reasoning_diagnostics(&arguments, &shared, &impacts, &[], &[], &[]);
        assert_eq!(
            diagnostics[0].shared_grounds[0].ground_object_id,
            Some(premise)
        );
        assert_eq!(
            diagnostics[0].shared_grounds[0].routes,
            vec![
                GroundingRouteRef {
                    argument_id: support_argument,
                    grounding_set_id: support_set
                },
                GroundingRouteRef {
                    argument_id: challenge_argument,
                    grounding_set_id: support_set
                }
            ]
        );
        shared[1].members[0].role = GroundingMemberRole::Context;
        let diagnostics = reasoning_diagnostics(&arguments, &shared, &impacts, &[], &[], &[]);
        assert!(!diagnostics[0].has_opposing_routes);
        assert!(!diagnostics[0].has_mixed_directional_grounding);
        assert_eq!(diagnostics[0].routes[1].non_context_member_count, 0);
        assert!(diagnostics[0].shared_grounds.is_empty());
        for set in &mut shared {
            set.members[0].role = GroundingMemberRole::Evidence;
            set.members[0].ground_object_id = None;
            set.members[0].evidence_id = Some(premise);
        }
        let diagnostics = reasoning_diagnostics(&arguments, &shared, &impacts, &[], &[], &[]);
        assert_eq!(diagnostics[0].shared_grounds[0].evidence_id, Some(premise));

        let serialized = serde_json::to_value(diagnostic).unwrap();
        assert!(serialized.get("score").is_none());
        assert!(serialized.get("confidence").is_none());
        assert!(serialized.get("truth").is_none());
    }

    #[test]
    fn human_authored_assumptions_and_hypotheses_expose_structural_impacts() {
        let assumption = Uuid::from_u128(401);
        let other_ground = Uuid::from_u128(402);
        let hypothesis = Uuid::from_u128(403);
        let finding = Uuid::from_u128(404);
        let question = Uuid::from_u128(405);
        let node = |object_id, kind| ExpertNode {
            object_id,
            kind,
            situation_id: Uuid::nil(),
            text: format!("object {object_id}"),
            rationale: "human-authored".into(),
            created_by: Uuid::nil(),
            replayed: false,
        };
        let nodes = vec![
            node(assumption, ExpertNodeKind::Assumption),
            node(other_ground, ExpertNodeKind::Proposition),
            node(hypothesis, ExpertNodeKind::Hypothesis),
            node(finding, ExpertNodeKind::Finding),
            node(question, ExpertNodeKind::Question),
        ];
        let first_argument = Uuid::from_u128(411);
        let second_argument = Uuid::from_u128(412);
        let argument = |argument_id, conclusion_object_id| ExpertArgument {
            argument_id,
            situation_id: Uuid::nil(),
            conclusion_object_id,
            mode: ArgumentMode::Abductive,
            rationale: "authored argument".into(),
            created_by: Uuid::nil(),
            replayed: false,
        };
        let member = |member_id, ground_object_id| GroundingMemberView {
            member_id,
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(ground_object_id),
            evidence_id: None,
            rationale: "explicit member".into(),
            recorded_by: Uuid::nil(),
        };
        let sets = vec![
            GroundingSetView {
                argument_id: first_argument,
                grounding_set_id: Uuid::from_u128(421),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "joint route".into(),
                recorded_by: Uuid::nil(),
                members: vec![
                    member(Uuid::from_u128(431), assumption),
                    member(Uuid::from_u128(432), other_ground),
                ],
                replayed: false,
            },
            GroundingSetView {
                argument_id: second_argument,
                grounding_set_id: Uuid::from_u128(422),
                effect: GroundingEffect::Challenges,
                adequacy: GroundingAdequacy::Sufficient,
                rationale: "downstream challenge".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(433), hypothesis)],
                replayed: false,
            },
            GroundingSetView {
                argument_id: first_argument,
                grounding_set_id: Uuid::from_u128(423),
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Sufficient,
                rationale: "parallel route untouched by the what-if".into(),
                recorded_by: Uuid::nil(),
                members: vec![member(Uuid::from_u128(434), other_ground)],
                replayed: false,
            },
        ];
        let relation = |relation_id, kind: RelationKind, left, right| ExpertRelation {
            relation_id,
            situation_id: Uuid::nil(),
            family: kind.family(),
            kind,
            left_object_id: left,
            right_object_id: right,
            rationale: "human-authored relation".into(),
            recorded_by: Uuid::nil(),
            replayed: false,
        };
        let analyses = authored_object_impacts(
            &nodes,
            &[
                relation(
                    Uuid::from_u128(441),
                    RelationKind::Contradicts,
                    assumption,
                    other_ground,
                ),
                relation(
                    Uuid::from_u128(442),
                    RelationKind::TestsHypothesis,
                    question,
                    hypothesis,
                ),
            ],
            &[
                argument(first_argument, hypothesis),
                argument(second_argument, finding),
            ],
            &sets,
        );

        let assumption_analysis = analyses
            .iter()
            .find(|analysis| analysis.source_object_id == assumption)
            .unwrap();
        assert_eq!(assumption_analysis.route_impacts.len(), 2);
        assert_eq!(assumption_analysis.route_impacts[0].depth, 1);
        assert_eq!(assumption_analysis.route_impacts[0].non_context_member_count, 2);
        assert!(assumption_analysis.route_impacts[0].source_is_direct_ground);
        assert_eq!(
            assumption_analysis.route_impacts[0].unreached_parallel_route_count,
            1
        );
        assert_eq!(assumption_analysis.route_impacts[1].depth, 2);
        assert_eq!(
            assumption_analysis.route_impacts[1].unreached_parallel_route_count,
            0
        );
        assert_eq!(
            assumption_analysis.route_impacts[1].effect,
            GroundingEffect::Challenges
        );
        assert_eq!(
            assumption_analysis.reached_conclusion_object_ids,
            vec![hypothesis, finding]
        );
        assert_eq!(assumption_analysis.contradicted_object_ids, vec![other_ground]);
        assert!(!assumption_analysis.establishes_truth);
        assert!(!assumption_analysis.directional_effects_are_aggregated);
        assert!(!assumption_analysis.treats_assumption_as_fact);

        let hypothesis_analysis = analyses
            .iter()
            .find(|analysis| analysis.source_object_id == hypothesis)
            .unwrap();
        assert_eq!(hypothesis_analysis.route_impacts.len(), 1);
        assert_eq!(hypothesis_analysis.tested_by_question_ids, vec![question]);
    }

    #[test]
    fn authored_impact_projection_terminates_on_reasoning_cycles() {
        let assumption = Uuid::from_u128(501);
        let hypothesis = Uuid::from_u128(502);
        let nodes = vec![
            ExpertNode {
                object_id: assumption,
                kind: ExpertNodeKind::Assumption,
                situation_id: Uuid::nil(),
                text: "Assume the sighting occurred".into(),
                rationale: "counterfactual".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
            ExpertNode {
                object_id: hypothesis,
                kind: ExpertNodeKind::Hypothesis,
                situation_id: Uuid::nil(),
                text: "The suspect was present".into(),
                rationale: "human hypothesis".into(),
                created_by: Uuid::nil(),
                replayed: false,
            },
        ];
        let make_argument = |argument_id, conclusion_object_id| ExpertArgument {
            argument_id,
            situation_id: Uuid::nil(),
            conclusion_object_id,
            mode: ArgumentMode::Abductive,
            rationale: "cycle".into(),
            created_by: Uuid::nil(),
            replayed: false,
        };
        let make_set = |argument_id, grounding_set_id, member_id, ground_object_id| {
            GroundingSetView {
                argument_id,
                grounding_set_id,
                effect: GroundingEffect::Supports,
                adequacy: GroundingAdequacy::Partial,
                rationale: "cycle route".into(),
                recorded_by: Uuid::nil(),
                members: vec![GroundingMemberView {
                    member_id,
                    role: GroundingMemberRole::Premise,
                    ground_object_id: Some(ground_object_id),
                    evidence_id: None,
                    rationale: "cycle member".into(),
                    recorded_by: Uuid::nil(),
                }],
                replayed: false,
            }
        };
        let arguments = vec![
            make_argument(Uuid::from_u128(511), hypothesis),
            make_argument(Uuid::from_u128(512), assumption),
        ];
        let sets = vec![
            make_set(
                Uuid::from_u128(511),
                Uuid::from_u128(521),
                Uuid::from_u128(531),
                assumption,
            ),
            make_set(
                Uuid::from_u128(512),
                Uuid::from_u128(522),
                Uuid::from_u128(532),
                hypothesis,
            ),
        ];
        let analyses = authored_object_impacts(&nodes, &[], &arguments, &sets);
        let impact = analyses
            .iter()
            .find(|analysis| analysis.source_object_id == assumption)
            .unwrap();
        assert_eq!(impact.route_impacts.len(), 2);
        assert_eq!(impact.route_impacts[0].depth, 1);
        assert_eq!(impact.route_impacts[1].depth, 2);
    }
}
