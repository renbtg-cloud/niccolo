//! Controlled, blind benchmarking of Niccolò against independent frontier agents.
//!
//! This is an offline, std-only executable contract/POC. It DOES NOT call LLMs,
//! establish frontier superiority, or authorize network data transfer.
//! An actual agent runner must cross this module's typed blind-input boundary,
//! enforce egress authorization, reserve cost atomically, and audit inputs.

use std::collections::BTreeSet;

/// Original evidence only. Headers are metadata present in the original source,
/// not source-lineage conclusions reconstructed by Niccolò.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalRecord {
    pub source_id: String,
    pub text: String,
    pub original_headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawCase {
    pub case_id: String,
    pub question: String,
    pub original_records: Vec<OriginalRecord>,
}

/// Internal findings must NOT be serialized into the blind baseline request.
#[derive(Clone, Debug)]
pub struct NiccoloPrivateAnalysis {
    pub source_lineage: Vec<String>,
    pub suspect_hypotheses: Vec<String>,
    pub insights: Vec<String>,
    pub proactive_questions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tool {
    SearchOriginals,
    ReadOriginals,
    InspectOriginalMetadata,
    KeepPrivateNotes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentResources {
    pub allowed_tools: BTreeSet<Tool>,
    pub max_agent_steps: u16,
}

impl AgentResources {
    pub fn competent_frontier_baseline() -> Self {
        Self {
            allowed_tools: BTreeSet::from([
                Tool::SearchOriginals,
                Tool::ReadOriginals,
                Tool::InspectOriginalMetadata,
                Tool::KeepPrivateNotes,
            ]),
            max_agent_steps: 12,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlindFrontierRequest {
    pub case_id: String,
    pub question: String,
    pub original_records: Vec<OriginalRecord>,
    pub resources: AgentResources,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BenchmarkError {
    EmptyCase,
    DuplicatedSourceId,
    WeakBaselineConfiguration,
    InvalidScoringRubric,
    NotIndependentlyAdjudicated,
}

/// Only accepts RawCase and independent resources. There is no parameter for
/// Niccolò-generated hypotheses or derived provenance: deliberately different
/// types/assemblies for baseline and Niccolò runs.
/// This is a type-level contract, not a replacement for an isolated agent runner.
pub fn prepare_blind_frontier_request(
    case: &RawCase,
    resources: AgentResources,
) -> Result<BlindFrontierRequest, BenchmarkError> {
    if case.case_id.trim().is_empty()
        || case.question.trim().is_empty()
        || case.original_records.is_empty()
    {
        return Err(BenchmarkError::EmptyCase);
    }
    let mut ids = BTreeSet::new();
    for record in &case.original_records {
        if record.source_id.trim().is_empty() || !ids.insert(&record.source_id) {
            return Err(BenchmarkError::DuplicatedSourceId);
        }
    }
    let required = AgentResources::competent_frontier_baseline();
    if resources.max_agent_steps < 4 || !required.allowed_tools.is_subset(&resources.allowed_tools)
    {
        return Err(BenchmarkError::WeakBaselineConfiguration);
    }
    Ok(BlindFrontierRequest {
        case_id: case.case_id.clone(),
        question: case.question.clone(),
        original_records: case.original_records.clone(),
        resources,
    })
}

/// One unit is one millionth of a US dollar. Track actual charges separately;
/// in a real concurrent service, reservations MUST be transactional in DB.
#[derive(Clone, Debug)]
pub struct EvaluationCostPolicy {
    pub background_evaluations_enabled: bool,
    pub extra_model_probes_allowed: bool,
    pub permitted_providers: BTreeSet<String>,
    pub prohibited_models: BTreeSet<String>,
    pub monthly_cap_micro_usd: Option<u64>,
    pub monthly_spent_micro_usd: u64,
    pub monthly_reserved_micro_usd: u64,
    /// User-configured upper bound on total utilization after the probe.
    /// 300 means background probes require total projected usage <= 30% of cap.
    pub maximum_background_utilization_per_mille: u16,
}

pub struct ProposedFrontierProbe<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub original_data_egress_authorized: bool,
    pub estimated_incremental_micro_usd: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeDenied {
    BackgroundEvaluationDisabled,
    ExtraModelNotAuthorized,
    ProviderNotPermitted,
    ModelProhibited,
    DataTransferNotAuthorized,
    NoConfiguredBudget,
    InvalidUtilizationThreshold,
    MonthlyBudgetWouldBeExceeded,
    NotWellBelowUserBudget,
}

/// The model need not be selected as the user's preferred model. That is not
/// permission to use a new provider or export records to an unapproved party.
pub fn authorize_opportunistic_frontier_probe(
    policy: &EvaluationCostPolicy,
    probe: &ProposedFrontierProbe<'_>,
) -> Result<(), ProbeDenied> {
    if !policy.background_evaluations_enabled {
        return Err(ProbeDenied::BackgroundEvaluationDisabled);
    }
    if !policy.extra_model_probes_allowed {
        return Err(ProbeDenied::ExtraModelNotAuthorized);
    }
    if !policy.permitted_providers.contains(probe.provider) {
        return Err(ProbeDenied::ProviderNotPermitted);
    }
    if policy.prohibited_models.contains(probe.model) {
        return Err(ProbeDenied::ModelProhibited);
    }
    if !probe.original_data_egress_authorized {
        return Err(ProbeDenied::DataTransferNotAuthorized);
    }
    let cap = policy
        .monthly_cap_micro_usd
        .filter(|value| *value > 0)
        .ok_or(ProbeDenied::NoConfiguredBudget)?;
    if policy.maximum_background_utilization_per_mille == 0
        || policy.maximum_background_utilization_per_mille > 1000
    {
        return Err(ProbeDenied::InvalidUtilizationThreshold);
    }
    let prospective = (policy.monthly_spent_micro_usd as u128)
        + (policy.monthly_reserved_micro_usd as u128)
        + (probe.estimated_incremental_micro_usd as u128);
    if prospective > (cap as u128) {
        return Err(ProbeDenied::MonthlyBudgetWouldBeExceeded);
    }
    if prospective * 1000
        > (cap as u128) * (policy.maximum_background_utilization_per_mille as u128)
    {
        return Err(ProbeDenied::NotWellBelowUserBudget);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Criterion {
    FactualCorrectness,
    NestedBeliefAttribution,
    EpistemicDiscipline,
    SourceTraceability,
    UsefulUnexpectedQuestions,
    CalibratedUncertainty,
}

#[derive(Clone, Debug)]
pub struct ScoredCriterion {
    pub criterion: Criterion,
    pub weight: u8,
    pub niccolo_score: u8,
    pub frontier_agent_score: u8,
    /// Traceable independent assessment, not the contestant's self-rating.
    pub reference_or_reviewer: String,
    pub supporting_original_source_ids: Vec<String>,
    pub explanation: String,
}

#[derive(Clone, Debug)]
pub struct AdjudicatedComparison {
    pub case_id: String,
    pub independent_review: bool,
    pub dimensions: Vec<ScoredCriterion>,
    pub niccolo_cost_micro_usd: u64,
    pub frontier_cost_micro_usd: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparisonResult {
    /// Each score is 0..10000 basis points, i.e. 10000 == 100.00 / 100.
    pub niccolo_quality_basis_points: u32,
    pub frontier_quality_basis_points: u32,
    pub failing_dimensions: Vec<Criterion>,
}

pub fn compare_adjudicated_results(
    comparison: &AdjudicatedComparison,
) -> Result<ComparisonResult, BenchmarkError> {
    if !comparison.independent_review {
        return Err(BenchmarkError::NotIndependentlyAdjudicated);
    }
    let mut weight = 0u16;
    let mut criteria = BTreeSet::new();
    let mut niccolo_sum = 0u32;
    let mut frontier_sum = 0u32;
    let mut failing_dimensions = Vec::new();
    for metric in &comparison.dimensions {
        if metric.weight == 0
            || metric.niccolo_score > 100
            || metric.frontier_agent_score > 100
            || metric.reference_or_reviewer.trim().is_empty()
            || metric.explanation.trim().is_empty()
        {
            return Err(BenchmarkError::InvalidScoringRubric);
        }
        // Prevent multiple weight assignments to the same dimension.
        if !criteria.insert(format!("{:?}", metric.criterion)) {
            return Err(BenchmarkError::InvalidScoringRubric);
        }
        weight += metric.weight as u16;
        niccolo_sum += (metric.weight as u32) * (metric.niccolo_score as u32);
        frontier_sum += (metric.weight as u32) * (metric.frontier_agent_score as u32);
        if metric.frontier_agent_score > metric.niccolo_score {
            failing_dimensions.push(metric.criterion.clone());
        }
    }
    if weight != 100 {
        return Err(BenchmarkError::InvalidScoringRubric);
    }
    Ok(ComparisonResult {
        niccolo_quality_basis_points: niccolo_sum,
        frontier_quality_basis_points: frontier_sum,
        failing_dimensions,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImprovementExperiment {
    pub case_id: String,
    pub observed_gap: Vec<Criterion>,
    pub reproducer: String,
    pub proposed_change: String,
    pub passing_condition: String,
    pub evidence_required: String,
    pub release_status: String,
}

/// Candidate proposal only: a regression is not evidence that a particular fix
/// works. The fix must be separately implemented, run and validated on held-out
/// data before any release.
pub fn prepare_improvement_experiment(
    comparison: &AdjudicatedComparison,
    result: &ComparisonResult,
) -> Option<ImprovementExperiment> {
    if result.failing_dimensions.is_empty() {
        return None;
    }
    Some(ImprovementExperiment {
        case_id: comparison.case_id.clone(),
        observed_gap: result.failing_dimensions.clone(),
        reproducer: format!(
            "Replay case {} against a frontier agent using only authorized original records; record model/version, prompts, tools, sampling, source IDs and costs.",
            comparison.case_id
        ),
        proposed_change: "Form a falsifiable algorithmic hypothesis targeting the observed gaps; do not copy the frontier answer into Niccolò's canonical facts.".to_owned(),
        passing_condition: "Re-run on held-out and adversarial cases under comparable budgets; independently adjudicate epistemic correctness and question quality.".to_owned(),
        evidence_required: "Actual archived competing outputs, tool traces, independent judgments, signed-off data permissions and reproducible code/tests.".to_owned(),
        release_status: "PROPOSAL_ONLY_NOT_AUTOMATICALLY_DEPLOYED".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> RawCase {
        RawCase {
            case_id: "borgia-demo".into(),
            question: "What explains the private meetings?".into(),
            original_records: vec![OriginalRecord {
                source_id: "original-note-1".into(),
                text: "Three courtiers repeated the clerk's phrase".into(),
                original_headers: vec![("date".into(), "1503-02-01".into())],
            }],
        }
    }
    fn policy() -> EvaluationCostPolicy {
        EvaluationCostPolicy {
            background_evaluations_enabled: true,
            extra_model_probes_allowed: true,
            permitted_providers: BTreeSet::from(["authorized-provider".into()]),
            prohibited_models: BTreeSet::new(),
            monthly_cap_micro_usd: Some(10_000_000),
            monthly_spent_micro_usd: 1_000_000,
            monthly_reserved_micro_usd: 500_000,
            maximum_background_utilization_per_mille: 300,
        }
    }
    fn probe<'a>() -> ProposedFrontierProbe<'a> {
        ProposedFrontierProbe {
            provider: "authorized-provider",
            model: "frontier-model",
            original_data_egress_authorized: true,
            estimated_incremental_micro_usd: 100_000,
        }
    }
    #[test]
    fn unselected_frontier_may_be_sampled_if_permitted() {
        assert_eq!(
            authorize_opportunistic_frontier_probe(&policy(), &probe()),
            Ok(())
        );
    }
    #[test]
    fn unauthorized_provider_is_blocked() {
        let p = ProposedFrontierProbe {
            provider: "other-provider",
            ..probe()
        };
        assert_eq!(
            authorize_opportunistic_frontier_probe(&policy(), &p),
            Err(ProbeDenied::ProviderNotPermitted)
        );
    }
    #[test]
    fn forbidden_model_is_blocked() {
        let mut p = policy();
        p.prohibited_models.insert("frontier-model".into());
        assert_eq!(
            authorize_opportunistic_frontier_probe(&p, &probe()),
            Err(ProbeDenied::ModelProhibited)
        );
    }
    #[test]
    fn external_raw_evidence_transfer_needs_authorization() {
        let req = ProposedFrontierProbe {
            original_data_egress_authorized: false,
            ..probe()
        };
        assert_eq!(
            authorize_opportunistic_frontier_probe(&policy(), &req),
            Err(ProbeDenied::DataTransferNotAuthorized)
        );
    }
    #[test]
    fn no_configured_monthly_budget_means_no_autonomous_spending() {
        let mut p = policy();
        p.monthly_cap_micro_usd = None;
        assert_eq!(
            authorize_opportunistic_frontier_probe(&p, &probe()),
            Err(ProbeDenied::NoConfiguredBudget)
        );
    }
    #[test]
    fn background_evaluation_opt_out_is_respected() {
        let mut p = policy();
        p.background_evaluations_enabled = false;
        assert_eq!(
            authorize_opportunistic_frontier_probe(&p, &probe()),
            Err(ProbeDenied::BackgroundEvaluationDisabled)
        );
    }
    #[test]
    fn extra_model_opt_out_is_respected() {
        let mut p = policy();
        p.extra_model_probes_allowed = false;
        assert_eq!(
            authorize_opportunistic_frontier_probe(&p, &probe()),
            Err(ProbeDenied::ExtraModelNotAuthorized)
        );
    }
    #[test]
    fn opportunistic_probes_must_stay_well_below_configured_cap() {
        let p = ProposedFrontierProbe {
            estimated_incremental_micro_usd: 1_600_000,
            ..probe()
        };
        assert_eq!(
            authorize_opportunistic_frontier_probe(&policy(), &p),
            Err(ProbeDenied::NotWellBelowUserBudget)
        );
    }
    #[test]
    fn absolute_monthly_cap_is_enforced_even_under_overflow_attempts() {
        let p = ProposedFrontierProbe {
            estimated_incremental_micro_usd: u64::MAX,
            ..probe()
        };
        assert_eq!(
            authorize_opportunistic_frontier_probe(&policy(), &p),
            Err(ProbeDenied::MonthlyBudgetWouldBeExceeded)
        );
    }
    #[test]
    fn corrupted_threshold_rejected() {
        let mut p = policy();
        p.maximum_background_utilization_per_mille = 1001;
        assert_eq!(
            authorize_opportunistic_frontier_probe(&p, &probe()),
            Err(ProbeDenied::InvalidUtilizationThreshold)
        );
    }
    #[test]
    fn hidden_niccolo_insights_do_not_enter_blind_request() {
        let secret = "NICCOLO_PRIVATE_HYPOTHESIS_DO_NOT_LEAK";
        let internal = NiccoloPrivateAnalysis {
            source_lineage: vec![secret.into()],
            suspect_hypotheses: vec![secret.into()],
            insights: vec![secret.into()],
            proactive_questions: vec![secret.into()],
        };
        let blind =
            prepare_blind_frontier_request(&case(), AgentResources::competent_frontier_baseline())
                .unwrap();
        assert!(!format!("{blind:?}").contains(secret));
        assert!(format!("{internal:?}").contains(secret));
        assert_eq!(blind.original_records[0].original_headers[0].0, "date");
    }
    #[test]
    fn weak_baseline_is_not_accepted_as_fair() {
        let mut tools = AgentResources::competent_frontier_baseline();
        tools.max_agent_steps = 1;
        assert_eq!(
            prepare_blind_frontier_request(&case(), tools),
            Err(BenchmarkError::WeakBaselineConfiguration)
        );
    }
    #[test]
    fn duplicate_source_ids_are_rejected() {
        let mut c = case();
        c.original_records.push(c.original_records[0].clone());
        assert_eq!(
            prepare_blind_frontier_request(&c, AgentResources::competent_frontier_baseline()),
            Err(BenchmarkError::DuplicatedSourceId)
        );
    }
    fn evaluation() -> AdjudicatedComparison {
        AdjudicatedComparison {
            case_id: "synthetic-benchmark".into(),
            independent_review: true,
            dimensions: vec![
                ScoredCriterion {
                    criterion: Criterion::EpistemicDiscipline,
                    weight: 60,
                    niccolo_score: 65,
                    frontier_agent_score: 90,
                    reference_or_reviewer: "independent reviewer 1".into(),
                    supporting_original_source_ids: vec!["original-note-1".into()],
                    explanation: "Synthetic sample: frontier avoids overclaiming".into(),
                },
                ScoredCriterion {
                    criterion: Criterion::UsefulUnexpectedQuestions,
                    weight: 40,
                    niccolo_score: 80,
                    frontier_agent_score: 70,
                    reference_or_reviewer: "independent reviewer 1".into(),
                    supporting_original_source_ids: vec!["original-note-1".into()],
                    explanation: "Synthetic sample: Niccolò asks a more useful question".into(),
                },
            ],
            niccolo_cost_micro_usd: 3000,
            frontier_cost_micro_usd: 21000,
        }
    }
    #[test]
    fn independent_scoring_finds_real_dimension_level_gaps() {
        let result = compare_adjudicated_results(&evaluation()).unwrap();
        assert_eq!(result.niccolo_quality_basis_points, 7100);
        assert_eq!(result.frontier_quality_basis_points, 8200);
        assert_eq!(
            result.failing_dimensions,
            vec![Criterion::EpistemicDiscipline]
        );
    }
    #[test]
    fn self_judgment_cannot_declare_niccolo_winner() {
        let mut e = evaluation();
        e.independent_review = false;
        assert_eq!(
            compare_adjudicated_results(&e),
            Err(BenchmarkError::NotIndependentlyAdjudicated)
        );
    }
    #[test]
    fn broken_or_duplicate_weights_are_rejected() {
        let mut e = evaluation();
        e.dimensions[1].weight = 39;
        assert_eq!(
            compare_adjudicated_results(&e),
            Err(BenchmarkError::InvalidScoringRubric)
        );
    }
    #[test]
    fn improvement_ticket_is_reproducer_not_false_success_claim() {
        let eval = evaluation();
        let result = compare_adjudicated_results(&eval).unwrap();
        let proposal = prepare_improvement_experiment(&eval, &result).unwrap();
        assert_eq!(proposal.observed_gap, vec![Criterion::EpistemicDiscipline]);
        assert!(proposal.reproducer.contains("synthetic-benchmark"));
        assert!(
            proposal
                .evidence_required
                .contains("Actual archived competing outputs")
        );
        assert_eq!(
            proposal.release_status,
            "PROPOSAL_ONLY_NOT_AUTOMATICALLY_DEPLOYED"
        );
    }
}
