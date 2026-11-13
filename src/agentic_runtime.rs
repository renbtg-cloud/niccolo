//! v0.291-v0.300: bounded execution loop over the durable agentic DAG.
//! The runtime may execute reasoning steps, but human-required steps always hand control back.
use crate::bff_operations::OperationScope;
use crate::{
    adversarial_challenge, agentic_controller, agentic_policy, analysis_coverage, analysis_quality,
    bff_operations, model_adapter, model_artifacts, observation_query_ledger,
    observation_retrieval, operation_budget, operation_checkpoint, reasoning_handoff,
    reasoning_packet, reasoning_updates, resource_policy, runtime_resources,
};
use model_adapter::{InvocationScope, ModelRequest, ReasoningModel};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("controller unavailable: {0}")]
    Controller(#[from] agentic_controller::ControllerError),
    #[error("operation unavailable: {0}")]
    Operation(#[from] bff_operations::BffOperationError),
    #[error("model unavailable: {0}")]
    Model(#[from] model_adapter::ModelError),
    #[error("policy unavailable: {0}")]
    Policy(#[from] agentic_policy::PolicyError),
    #[error("reasoning packet unavailable: {0}")]
    Packet(#[from] reasoning_packet::PacketError),
    #[error("model artifact invalid: {0}")]
    Artifact(#[from] model_artifacts::ArtifactError),
    #[error("reasoning update invalid: {0}")]
    Update(#[from] reasoning_updates::UpdateError),
    #[error("analysis quality unavailable: {0}")]
    Quality(#[from] analysis_quality::QualityError),
    #[error("adversarial challenge invalid: {0}")]
    Challenge(#[from] adversarial_challenge::ChallengeError),
    #[error("operation budget unavailable: {0}")]
    Budget(#[from] operation_budget::BudgetError),
    #[error("operation checkpoint unavailable: {0}")]
    Checkpoint(#[from] operation_checkpoint::CheckpointError),
    #[error("analysis coverage unavailable: {0}")]
    Coverage(#[from] analysis_coverage::AnalysisCoverageError),
    #[error("reasoning handoff unavailable: {0}")]
    Handoff(#[from] reasoning_handoff::HandoffError),
    #[error("observation search unavailable: {0}")]
    ObservationSearch(#[from] observation_retrieval::ObservationSearchError),
    #[error("observation query audit unavailable: {0}")]
    ObservationLedger(#[from] observation_query_ledger::QueryLedgerError),
    #[error("agentic operation requires an analysis")]
    MissingAnalysis,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutcome {
    pub state: String,
    pub completed_steps: usize,
    pub waiting_for_human: bool,
    pub terminal: bool,
}

async fn record_step_checkpoint(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    step: &agentic_controller::StepView,
) -> Result<(), RuntimeError> {
    let op = bff_operations::get(pool, scope).await?;
    let analysis = op.target_analysis_id.ok_or(RuntimeError::MissingAnalysis)?;
    let controller = agentic_controller::view(pool, scope).await?;
    let budget = operation_budget::view(pool, scope).await?;
    let quality = analysis_quality::report(
        pool,
        crate::analysis_structure::AnalysisScope {
            workspace: scope.workspace,
            situation: scope.situation,
            analysis,
        },
    )
    .await?;
    let succeeded_steps = controller
        .steps
        .iter()
        .filter(|candidate| candidate.state == "SUCCEEDED")
        .count();
    let failed_steps = controller
        .steps
        .iter()
        .filter(|candidate| candidate.state == "FAILED")
        .count();
    let summary = json!({
        "operation_state": op.state,
        "analysis_id": analysis,
        "plan_revision": controller.plan_revision,
        "plan_sha256": controller.plan_sha256,
        "completed_step_id": step.step_id,
        "completed_step_kind": step.kind,
        "completed_step_label": step.label,
        "controller_complete": controller.complete,
        "controller_waiting_for_human": controller.waiting_for_human,
        "controller_blocked": controller.blocked,
        "succeeded_step_count": succeeded_steps,
        "failed_step_count": failed_steps,
        "total_step_count": controller.steps.len(),
        "operation_budget": {
            "max_invocations": budget.max_invocations,
            "used_invocations": budget.used_invocations,
            "remaining_invocations": budget.remaining_invocations,
            "max_total_tokens": budget.max_total_tokens,
            "used_total_tokens": budget.used_total_tokens,
            "remaining_total_tokens": budget.remaining_total_tokens,
            "unreported_token_usage_invocations": budget.unreported_token_usage_invocations,
            "token_usage_complete": budget.token_usage_complete,
            "remaining_total_tokens_is_upper_bound": budget.remaining_total_tokens_is_upper_bound,
            "stop_reasons": budget.stop_reasons,
            "can_start_model_call": budget.can_start_model_call,
            "budget_is_quality_score": false
        },
        "quality": quality,
        "checkpoint_is_provisional": true,
        "checkpoint_establishes_truth": false,
        "checkpoint_proves_completeness": false
    });
    let kind = if controller.waiting_for_human {
        "HUMAN_BOUNDARY"
    } else {
        "STEP_COMPLETED"
    };
    operation_checkpoint::record(
        pool,
        scope,
        &operation_checkpoint::NewCheckpoint {
            checkpoint_id: step.step_id,
            analysis_id: Some(analysis),
            checkpoint_kind: kind.into(),
            step_id: Some(step.step_id),
            summary,
            provisional: true,
            terminal: false,
            recorded_by: actor,
        },
    )
    .await?;
    Ok(())
}

fn step_requires_model(step: &agentic_controller::StepView) -> bool {
    !step.requires_human
        && !matches!(
            step.kind.as_str(),
            "REQUEST_HUMAN_DECISION" | "CHECK_STOP_CONDITIONS" | "PREPARE_HANDOFF"
        )
}

fn budget_stop_reason(error: &operation_budget::BudgetError) -> Option<&'static str> {
    match error {
        operation_budget::BudgetError::Deadline => Some("DEADLINE_EXPIRED"),
        operation_budget::BudgetError::FinalizationReserve => Some("FINALIZATION_RESERVE"),
        operation_budget::BudgetError::Invocations => Some("INVOCATION_LIMIT"),
        operation_budget::BudgetError::Tokens => Some("TOKEN_LIMIT"),
        _ => None,
    }
}

async fn finish_budget_boundary(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    step: &agentic_controller::StepView,
    completed_steps: usize,
    reason: &str,
) -> Result<RunOutcome, RuntimeError> {
    let op = bff_operations::get(pool, scope).await?;
    let analysis = op.target_analysis_id.ok_or(RuntimeError::MissingAnalysis)?;
    let controller = agentic_controller::view(pool, scope).await?;
    let budget = operation_budget::view(pool, scope).await?;
    let quality = analysis_quality::report(
        pool,
        crate::analysis_structure::AnalysisScope {
            workspace: scope.workspace,
            situation: scope.situation,
            analysis,
        },
    )
    .await?;
    let handoff = reasoning_handoff::bundle(
        pool,
        crate::analysis_structure::AnalysisScope {
            workspace: scope.workspace,
            situation: scope.situation,
            analysis,
        },
    )
    .await?;
    let handoff_sha256 = reasoning_handoff::sha256(&handoff)?;
    let result = json!({
        "agentic": true,
        "completion_status": "PARTIAL_BUDGET_BOUNDARY",
        "provisional": true,
        "stop_reason": reason,
        "next_unexecuted_step_id": step.step_id,
        "next_unexecuted_step_kind": step.kind,
        "completed_steps_this_run": completed_steps,
        "plan_revision": controller.plan_revision,
        "plan_sha256": controller.plan_sha256,
        "quality": quality,
        "budget": budget,
        "handoff": handoff,
        "handoff_sha256": handoff_sha256,
        "published_analysis_may_be_absent_or_older": true,
        "partial_result_establishes_truth": false,
        "partial_result_proves_completeness": false,
        "human_owns_consequential_decisions": true
    });
    let final_view = if op.state == "RUNNING" {
        bff_operations::succeed(pool, scope, op.state_version, actor, &result).await?
    } else {
        op
    };
    let checkpoint_summary = json!({
        "completion_status": "PARTIAL_BUDGET_BOUNDARY",
        "stop_reason": reason,
        "analysis_id": analysis,
        "plan_revision": controller.plan_revision,
        "plan_sha256": controller.plan_sha256,
        "next_unexecuted_step_id": step.step_id,
        "next_unexecuted_step_kind": step.kind,
        "completed_steps_this_run": completed_steps,
        "used_invocations": budget.used_invocations,
        "max_invocations": budget.max_invocations,
        "used_total_tokens": budget.used_total_tokens,
        "max_total_tokens": budget.max_total_tokens,
        "token_usage_complete": budget.token_usage_complete,
        "handoff_sha256": handoff_sha256,
        "provisional": true,
        "establishes_truth": false,
        "proves_completeness": false
    });
    operation_checkpoint::record(
        pool,
        scope,
        &operation_checkpoint::NewCheckpoint {
            checkpoint_id: step.step_id,
            analysis_id: Some(analysis),
            checkpoint_kind: "BUDGET_BOUNDARY".into(),
            step_id: Some(step.step_id),
            summary: checkpoint_summary,
            provisional: true,
            terminal: true,
            recorded_by: actor,
        },
    )
    .await?;
    Ok(RunOutcome {
        state: final_view.state,
        completed_steps,
        waiting_for_human: false,
        terminal: final_view.terminal,
    })
}

pub async fn ensure_default_plan(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
) -> Result<agentic_controller::ControllerView, RuntimeError> {
    match agentic_controller::view(pool, scope).await {
        Ok(v) => Ok(v),
        Err(agentic_controller::ControllerError::Invisible) => {
            let op = bff_operations::get(pool, scope).await?;
            let analysis = op.target_analysis_id.ok_or(RuntimeError::MissingAnalysis)?;
            let input = bff_operations::input(pool, scope).await?;
            let intent = input
                .get("human_intent")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok());
            let steps = agentic_policy::plan_for_intent(
                pool,
                scope.workspace,
                scope.situation,
                analysis,
                intent,
            )
            .await?;
            agentic_controller::create_plan(pool, scope, actor, &steps).await?;
            Ok(agentic_controller::view(pool, scope).await?)
        }
        Err(e) => Err(e.into()),
    }
}

async fn execute_step<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &M,
    step: &agentic_controller::StepView,
    plan_revision: i64,
) -> Result<(), RuntimeError> {
    // Once STARTED is persisted, every error path must leave a terminal step.
    // Model/artifact failures are handled here alongside pre-invocation errors.
    agentic_controller::start_step(pool, scope, step.step_id, actor).await?;
    let result: Result<(), RuntimeError> = async {
        if step.requires_human || step.kind == "REQUEST_HUMAN_DECISION" {
            agentic_controller::finish_step(
                pool,
                scope,
                step.step_id,
                actor,
                "WAITING_FOR_HUMAN",
                "explicit human decision required",
            )
            .await?;
            return Ok(());
        }
        let op = bff_operations::get(pool, scope).await?;
        let analysis = op.target_analysis_id.ok_or(RuntimeError::MissingAnalysis)?;
        let analysis_scope = crate::analysis_structure::AnalysisScope {
            workspace: scope.workspace,
            situation: scope.situation,
            analysis,
        };
        if step.kind == "CHECK_STOP_CONDITIONS" {
            let signals =
                agentic_policy::signals(pool, scope.workspace, scope.situation, analysis).await?;
            agentic_controller::finish_step(
                pool,
                scope,
                step.step_id,
                actor,
                "SUCCEEDED",
                &format!(
                    "deterministic stop-condition check: {} triggered",
                    signals.triggered_stop_conditions
                ),
            )
            .await?;
            return Ok(());
        }
        if step.kind == "PREPARE_HANDOFF" {
            let handoff = reasoning_handoff::bundle(pool, analysis_scope).await?;
            let handoff_sha = reasoning_handoff::sha256(&handoff)?;
            agentic_controller::finish_step(
                pool,
                scope,
                step.step_id,
                actor,
                "SUCCEEDED",
                &format!("deterministic handoff prepared: {handoff_sha}"),
            )
            .await?;
            return Ok(());
        }
        let packet = reasoning_packet::build(pool, analysis_scope).await?;
        let packet_sha = reasoning_packet::sha256(&packet)?;
        let human_steering = agentic_controller::human_steering(pool, scope, 32).await?;
        let packet_evidence_ids = packet
            .frozen_input
            .source_fragments
            .iter()
            .map(|fragment| fragment.evidence_id)
            .collect::<Vec<_>>();
        let request = ModelRequest {
            purpose: step.kind.clone(),
            input: json!({"operation_id":scope.operation,"analysis_id":analysis,"label":step.label,"plan_revision":plan_revision,"reasoning_packet_sha256":packet_sha,"reasoning_packet":packet,"human_steering":human_steering}),
            output_contract: if step.kind == "SYNTHESIZE" {
                json!({"type":"object","required":["purpose","status","niccolo_artifact"],"niccolo_artifact":{"required":["answer_text","coverage_note","claims"],"optional":["source_links","context_links","assumptions","alternatives","evidence_gaps","clarifications"]},"niccolo_rules":{"separate_evidence_from_inference":true,"surface_competing_interpretations":true,"do_not_execute_human_actions":true,"human_steering_is_not_evidence":true,"human_steering_does_not_rewrite_frozen_input":true}})
            } else if matches!(
                step.kind.as_str(),
                "ASSESS_CONTEXT" | "CHECK_ASSUMPTIONS" | "PROPOSE_CLARIFICATION" | "CHECK_ALTERNATIVES"
            ) {
                json!({"type":"object","required":["purpose","status","niccolo_update"],"niccolo_update":{"optional":["used_contribution_ids","candidate_interpretations","assumptions","evidence_gaps","clarifications","source_queries"]},"hypothesis_fields":{"predictions":"bounded expected observations with SUPPORT, COUNTEREVIDENCE or DISCRIMINATE direction","evidence_links":"frozen-input evidence links with SUPPORTS, CONTRADICTS or CONTEXT relation","relations":"links to other candidate interpretation UUIDs using COMPETES_WITH, CAN_COEXIST, REFINES or OVERLAPS"},"clarification_value_fields":{"discriminates_interpretation_ids":"candidate interpretation UUIDs affected by plausible answers","possible_outcomes":"bounded answer/observation classes with effect notes","decision_relevance":"0-100 scheduling heuristic","acquisition_cost":"0-100 effort/cost heuristic","cheaper_alternative":"optional cheaper way to answer the same discriminator"},"niccolo_rules":{"human_context_is_not_evidence":true,"source_queries_are_retrieval_intents_not_facts":true,"question_value_scores_are_scheduling_heuristics_not_truth_probabilities":true,"candidate_interpretations_are_hypotheses_not_facts":true,"multiple_interpretations_may_coexist":true,"temporal_assertions_keep_their_qualification":true,"later_information_must_not_be_projected_backward_without_support":true,"formal_authority_is_not_practical_influence":true,"inferred_awareness_is_not_observed_knowledge":true,"source_scope_statuses_are_not_evidence_of_misconduct":true,"uncollected_or_unknown_source_families_forbid_claiming_no_evidence_exists":true,"collection_roles_do_not_establish_original_authorship_or_neutrality":true,"summaries_and_transforms_remain_derived_material":true,"byte_or_hash_continuity_does_not_establish_pre_collection_authenticity_or_completeness":true,"decision_readiness_may_coexist_with_unresolved_unknowns":true,"material_reversal_conditions_should_be_exposed_before_stopping":true,"human_owns_consequential_decisions":true,"actor_kind_is_revisable_metadata_not_an_investigation_gate":true,"software_human_mixed_or_unknown_subjects_can_all_be_investigated_from_observable_behavior":true,"actor_kind_does_not_establish_mind_or_consciousness":true,"political_affiliation_does_not_change_misconduct_or_credibility_inference_without_case_specific_connecting_observations":true,"prefer_predictions_that_could_support_or_challenge_live_interpretations":true,"prefer_questions_whose_possible_answers_separate_live_interpretations_or_change_a_real_decision":true,"separate_evidence_from_inference":true,"do_not_execute_human_actions":true,"human_steering_is_not_evidence":true,"human_steering_does_not_rewrite_frozen_input":true}})
            } else if step.kind == "CHALLENGE_SYNTHESIS" {
                json!({"type":"object","required":["purpose","status","niccolo_challenge"],"niccolo_challenge":{"required":["challenges"]},"niccolo_rules":{"seek_counterevidence":true,"seek_competing_interpretations":true,"challenge_is_not_truth":true,"do_not_execute_human_actions":true,"human_steering_is_not_evidence":true,"human_steering_does_not_rewrite_frozen_input":true}})
            } else {
                json!({"type":"object","required":["purpose","status"],"niccolo_rules":{"separate_evidence_from_inference":true,"surface_competing_interpretations":true,"do_not_execute_human_actions":true,"human_steering_is_not_evidence":true,"human_steering_does_not_rewrite_frozen_input":true}})
            },
        };
        // Re-read the live operation budget immediately before the provider
        // call. Packet construction and deterministic checks may have consumed
        // wall-clock time since the outer admission check.
        let live_budget = operation_budget::require_available_for_attempts(
            pool,
            scope,
            model.provider_attempt_ceiling(),
        )
        .await?;
        let timeout_ms = live_budget
            .remaining_deadline_ms
            .map(|remaining| {
                remaining
                    .saturating_sub(live_budget.finalization_reserve_ms)
                    .clamp(1, 30_000) as u64
            })
            .unwrap_or(30_000);
        let invocation = model_adapter::invoke_recorded_with_policy(
            pool,
            InvocationScope {
                operation: scope,
                plan_revision,
                step_id: step.step_id,
            },
            actor,
            model,
            &request,
            model_adapter::InvocationPolicy {
                timeout_ms,
                max_attempts: 1,
                max_total_tokens: Some(live_budget.remaining_total_tokens),
            },
        )
        .await;
        match invocation {
            Ok(v) => {
                // Transport success does not establish that the provider returned
                // a valid Niccolò artifact. Keep failed post-processing from
                // stranding a durable step in STARTED indefinitely.
                let applied: Result<(), RuntimeError> = async {
                    let response = v
                        .response
                        .as_ref()
                        .expect("successful invocation has response");
                    if step.kind == "INSPECT_SOURCES" {
                        analysis_coverage::record_many(
                            pool,
                            scope.workspace,
                            actor,
                            scope.situation,
                            analysis,
                            &packet_evidence_ids,
                        )
                        .await?;
                    }
                    if step.kind == "SYNTHESIZE" {
                        let artifact = model_artifacts::parse(response)?
                            .ok_or(model_artifacts::ArtifactError::Invalid)?;
                        model_artifacts::apply(
                            pool,
                            crate::analysis_structure::AnalysisScope {
                                workspace: scope.workspace,
                                situation: scope.situation,
                                analysis,
                            },
                            actor,
                            &artifact,
                        )
                        .await?;
                    } else if matches!(
                        step.kind.as_str(),
                        "ASSESS_CONTEXT"
                            | "CHECK_ASSUMPTIONS"
                            | "PROPOSE_CLARIFICATION"
                            | "CHECK_ALTERNATIVES"
                    ) {
                        let update = reasoning_updates::parse(response)?
                            .ok_or(reasoning_updates::UpdateError::Invalid)?;
                        if step.kind != "CHECK_ALTERNATIVES" && !update.source_queries.is_empty() {
                            return Err(reasoning_updates::UpdateError::Invalid.into());
                        }
                        reasoning_updates::apply(pool, analysis_scope, actor, &update).await?;
                        if step.kind == "CHECK_ALTERNATIVES" {
                            // Model intents stay bounded by this operation's
                            // frozen analysis and the workspace's RLS boundary.
                            for query in &update.source_queries {
                                let found =
                                    observation_retrieval::search(pool, analysis_scope, &query.query, 5)
                                        .await?;
                                observation_query_ledger::record_hits(
                                    pool,
                                    analysis_scope,
                                    query.query_id,
                                    actor,
                                    &found.results,
                                )
                                .await?;
                            }
                        }
                    } else if step.kind == "CHALLENGE_SYNTHESIS" {
                        let artifact = adversarial_challenge::parse(response)?
                            .ok_or(adversarial_challenge::ChallengeError::Invalid)?;
                        adversarial_challenge::apply(
                            pool,
                            crate::analysis_structure::AnalysisScope {
                                workspace: scope.workspace,
                                situation: scope.situation,
                                analysis,
                            },
                            actor,
                            &artifact,
                        )
                        .await?;
                    }
                    Ok(())
                }
                .await;
                match applied {
                    Ok(()) => {
                        agentic_controller::finish_step(
                            pool,
                            scope,
                            step.step_id,
                            actor,
                            "SUCCEEDED",
                            &format!("model invocation {} succeeded", v.invocation_id),
                        )
                        .await?;
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Err(e) => Err(e.into()),
        }
    }
    .await;
    if let Err(error) = &result {
        let reason = match error {
            RuntimeError::Model(_) => "model invocation failed",
            RuntimeError::Artifact(_) | RuntimeError::Update(_) | RuntimeError::Challenge(_) => {
                "model artifact failed validation or application"
            }
            _ => "agentic step could not complete",
        };
        // A SQL failure may prevent this update too, but never suppress the
        // original error. The enclosing BFF independently fails its operation.
        let _ = agentic_controller::finish_step(pool, scope, step.step_id, actor, "FAILED", reason)
            .await;
    }
    result
}

fn resource_degradable_step(step: &agentic_controller::StepView) -> bool {
    step.kind == "PROPOSE_CLARIFICATION" && !step.requires_human
}

async fn maybe_apply_resource_degradation<O>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    step: &agentic_controller::StepView,
    policy: &resource_policy::ResourcePolicy,
    observe_resources: &O,
) -> Result<bool, RuntimeError>
where
    O: Fn() -> Result<runtime_resources::ResourceSnapshot, runtime_resources::ResourceError>,
{
    if !resource_degradable_step(step) {
        return Ok(false);
    }
    let Ok(resources) = observe_resources() else {
        // Missing telemetry must never invent pressure or silently degrade work.
        return Ok(false);
    };
    let Ok(advice) = resource_policy::advise(&resources, policy) else {
        // Invalid/delegated policy is an operator configuration concern, not a
        // license to silently skip reasoning.
        return Ok(false);
    };
    if !advice.apply_concurrency_automatically || !advice.defer_nonessential_new_work {
        return Ok(false);
    }

    agentic_controller::start_step(pool, scope, step.step_id, actor).await?;
    agentic_controller::finish_step(
        pool,
        scope,
        step.step_id,
        actor,
        "SKIPPED",
        "critical resource pressure deferred the bounded clarification-proposal pass; synthesis, adversarial challenge and human control remain required",
    )
    .await?;

    let op = bff_operations::get(pool, scope).await?;
    let analysis = op.target_analysis_id.ok_or(RuntimeError::MissingAnalysis)?;
    let checkpoint = operation_checkpoint::NewCheckpoint {
        checkpoint_id: step.step_id,
        analysis_id: Some(analysis),
        checkpoint_kind: "RESOURCE_DEGRADATION".into(),
        step_id: Some(step.step_id),
        summary: json!({
            "completion_status": "RESOURCE_DEGRADATION",
            "reason": "CRITICAL_RESOURCE_PRESSURE",
            "skipped_step_id": step.step_id,
            "skipped_step_kind": step.kind,
            "skipped_step_label": step.label,
            "resource_snapshot": resources,
            "resource_advice": advice,
            "synthesis_still_required": true,
            "adversarial_challenge_still_required": true,
            "human_control_still_required": true,
            "provisional": true,
            "establishes_truth": false,
            "proves_completeness": false,
            "resource_pressure_is_not_epistemic_quality": true
        }),
        provisional: true,
        terminal: false,
        recorded_by: actor,
    };
    operation_checkpoint::record(pool, scope, &checkpoint).await?;
    Ok(true)
}

async fn run_until_boundary_with_resource_observer<
    M: ReasoningModel + ?Sized,
    O: Fn() -> Result<runtime_resources::ResourceSnapshot, runtime_resources::ResourceError>,
>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &M,
    resource_policy: Option<&resource_policy::ResourcePolicy>,
    observe_resources: &O,
) -> Result<RunOutcome, RuntimeError> {
    ensure_default_plan(pool, scope, actor).await?;
    let _ = agentic_controller::recover_stale_started(pool, scope, actor, 120).await?;
    let mut completed = 0usize;
    loop {
        let view = agentic_controller::view(pool, scope).await?;
        if view.waiting_for_human {
            return Ok(RunOutcome {
                state: "WAITING_FOR_HUMAN".into(),
                completed_steps: completed,
                waiting_for_human: true,
                terminal: false,
            });
        }
        if view.complete {
            let op = bff_operations::get(pool, scope).await?;
            let analysis = op.target_analysis_id.ok_or(RuntimeError::MissingAnalysis)?;
            let quality = analysis_quality::report(
                pool,
                crate::analysis_structure::AnalysisScope {
                    workspace: scope.workspace,
                    situation: scope.situation,
                    analysis,
                },
            )
            .await?;
            let already_had_human_review =
                agentic_controller::human_review_occurred(pool, scope).await?;
            if quality.human_attention_recommended && !already_had_human_review {
                let reason = format!(
                    "{} open evidence gap(s), {} clarification(s), {} material adversarial challenge(s), {} frozen source fragment(s) not examined",
                    quality.open_evidence_gap_count,
                    quality.clarification_count,
                    quality.open_material_challenge_count,
                    quality.unexamined_source_fragment_count
                );
                let followup = agentic_policy::human_attention_plan(&reason)?;
                agentic_controller::create_plan(pool, scope, actor, &followup).await?;
                continue;
            }
            let handoff = reasoning_handoff::bundle(
                pool,
                crate::analysis_structure::AnalysisScope {
                    workspace: scope.workspace,
                    situation: scope.situation,
                    analysis,
                },
            )
            .await?;
            let handoff_sha256 = reasoning_handoff::sha256(&handoff)?;
            let resource_degraded_steps = view
                .steps
                .iter()
                .filter(|step| step.state == "SKIPPED" && resource_degradable_step(step))
                .map(|step| json!({"step_id":step.step_id,"kind":step.kind,"label":step.label}))
                .collect::<Vec<_>>();
            let resource_degraded = !resource_degraded_steps.is_empty();
            let result = json!({
                "agentic": true,
                "completion_status": if resource_degraded { "DEGRADED_RESOURCE_PRESSURE" } else { "COMPLETE" },
                "provisional": resource_degraded,
                "plan_revision": view.plan_revision,
                "plan_sha256": view.plan_sha256,
                "budget_used": view.budget_used,
                "steps": view.steps.len(),
                "quality": quality,
                "explicit_human_review_occurred": already_had_human_review,
                "resource_degraded_steps": resource_degraded_steps,
                "resource_pressure_is_not_epistemic_quality": true,
                "handoff": handoff,
                "handoff_sha256": handoff_sha256
            });
            let final_view = if op.state == "RUNNING" {
                bff_operations::succeed(pool, scope, op.state_version, actor, &result).await?
            } else {
                op
            };
            return Ok(RunOutcome {
                state: final_view.state,
                completed_steps: completed,
                waiting_for_human: false,
                terminal: final_view.terminal,
            });
        }
        if view.blocked {
            return Ok(RunOutcome {
                state: "BLOCKED".into(),
                completed_steps: completed,
                waiting_for_human: false,
                terminal: false,
            });
        }
        let Some(id) = view.ready_step_ids.first().copied() else {
            return Ok(RunOutcome {
                state: "IDLE".into(),
                completed_steps: completed,
                waiting_for_human: false,
                terminal: false,
            });
        };
        let step = view
            .steps
            .iter()
            .find(|s| s.step_id == id)
            .expect("ready step exists")
            .clone();
        if let Some(policy) = resource_policy
            && maybe_apply_resource_degradation(
                pool,
                scope,
                actor,
                &step,
                policy,
                observe_resources,
            )
            .await?
        {
            continue;
        }
        if step_requires_model(&step) {
            match operation_budget::require_available_for_attempts(
                pool,
                scope,
                model.provider_attempt_ceiling(),
            )
            .await
            {
                Ok(_) => {}
                Err(error) => {
                    if let Some(reason) = budget_stop_reason(&error) {
                        return finish_budget_boundary(
                            pool, scope, actor, &step, completed, reason,
                        )
                        .await;
                    }
                    return Err(error.into());
                }
            }
        }
        match execute_step(pool, scope, actor, model, &step, view.plan_revision).await {
            Ok(()) => {}
            Err(RuntimeError::Budget(error)) => {
                if let Some(reason) = budget_stop_reason(&error) {
                    return finish_budget_boundary(
                        pool, scope, actor, &step, completed, reason,
                    )
                    .await;
                }
                return Err(error.into());
            }
            Err(RuntimeError::Model(model_adapter::ModelError::Budget)) => {
                return finish_budget_boundary(
                    pool,
                    scope,
                    actor,
                    &step,
                    completed,
                    "TOKEN_LIMIT",
                )
                .await;
            }
            Err(RuntimeError::Model(model_adapter::ModelError::Timeout)) => {
                // A provider timeout is ordinarily an execution failure. If the
                // timeout was clamped to the operation's remaining model-call
                // window, however, transition to the same provisional deadline
                // boundary used when that window is exhausted before a call.
                match operation_budget::require_available_for_attempts(
                    pool,
                    scope,
                    model.provider_attempt_ceiling(),
                )
                .await
                {
                    Err(error) if budget_stop_reason(&error).is_some() => {
                        return finish_budget_boundary(
                            pool,
                            scope,
                            actor,
                            &step,
                            completed,
                            budget_stop_reason(&error).expect("checked above"),
                        )
                        .await;
                    }
                    _ => return Err(model_adapter::ModelError::Timeout.into()),
                }
            }
            Err(error) => return Err(error),
        }
        completed += 1;
        record_step_checkpoint(pool, scope, actor, &step).await?;
    }
}

/// Resource-aware execution using live runtime telemetry. Observation failures
/// remain fail-open for epistemic work: missing telemetry never licenses a skip.
pub async fn run_until_boundary_with_resource_policy<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &M,
    resource_policy: Option<&resource_policy::ResourcePolicy>,
) -> Result<RunOutcome, RuntimeError> {
    run_until_boundary_with_resource_observer(
        pool,
        scope,
        actor,
        model,
        resource_policy,
        &runtime_resources::observe,
    )
    .await
}

/// Deterministic validation/embedder seam: pin one already-observed resource
/// envelope for the duration of this run. First-party production execution uses
/// the live-observation function above instead.
#[doc(hidden)]
pub async fn run_until_boundary_with_resource_snapshot<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &M,
    resource_policy: &resource_policy::ResourcePolicy,
    snapshot: &runtime_resources::ResourceSnapshot,
) -> Result<RunOutcome, RuntimeError> {
    let snapshot = snapshot.clone();
    run_until_boundary_with_resource_observer(
        pool,
        scope,
        actor,
        model,
        Some(resource_policy),
        &move || Ok(snapshot.clone()),
    )
    .await
}

/// Backwards-compatible deterministic execution path. Explicitly non-adaptive
/// callers retain the previous behavior and never skip a reasoning step due to
/// ambient telemetry.
pub async fn run_until_boundary<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &M,
) -> Result<RunOutcome, RuntimeError> {
    run_until_boundary_with_resource_policy(pool, scope, actor, model, None).await
}
