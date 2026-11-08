//! v0.291-v0.300: bounded execution loop over the durable agentic DAG.
//! The runtime may execute reasoning steps, but human-required steps always hand control back.
use crate::bff_operations::OperationScope;
use crate::{
    adversarial_challenge, agentic_controller, agentic_policy, analysis_coverage, analysis_quality,
    bff_operations, model_adapter, model_artifacts, operation_budget, reasoning_handoff,
    reasoning_packet, reasoning_updates, observation_retrieval, observation_query_ledger,
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
        let _operation_budget = operation_budget::require_available(pool, scope).await?;
        let packet = reasoning_packet::build(pool, analysis_scope).await?;
        let packet_sha = reasoning_packet::sha256(&packet)?;
        let packet_evidence_ids = packet
            .frozen_input
            .source_fragments
            .iter()
            .map(|fragment| fragment.evidence_id)
            .collect::<Vec<_>>();
        let request = ModelRequest {
            purpose: step.kind.clone(),
            input: json!({"operation_id":scope.operation,"analysis_id":analysis,"label":step.label,"plan_revision":plan_revision,"reasoning_packet_sha256":packet_sha,"reasoning_packet":packet}),
            output_contract: if step.kind == "SYNTHESIZE" {
                json!({"type":"object","required":["purpose","status","niccolo_artifact"],"niccolo_artifact":{"required":["answer_text","coverage_note","claims"],"optional":["source_links","context_links","assumptions","alternatives","evidence_gaps","clarifications"]},"niccolo_rules":{"separate_evidence_from_inference":true,"surface_competing_interpretations":true,"do_not_execute_human_actions":true}})
            } else if matches!(
                step.kind.as_str(),
                "ASSESS_CONTEXT" | "CHECK_ASSUMPTIONS" | "PROPOSE_CLARIFICATION" | "CHECK_ALTERNATIVES"
            ) {
                json!({"type":"object","required":["purpose","status","niccolo_update"],"niccolo_update":{"optional":["used_contribution_ids","candidate_interpretations","assumptions","evidence_gaps","clarifications","source_queries"]},"hypothesis_fields":{"predictions":"bounded expected observations with SUPPORT, COUNTEREVIDENCE or DISCRIMINATE direction","evidence_links":"frozen-input evidence links with SUPPORTS, CONTRADICTS or CONTEXT relation","relations":"links to other candidate interpretation UUIDs using COMPETES_WITH, CAN_COEXIST, REFINES or OVERLAPS"},"clarification_value_fields":{"discriminates_interpretation_ids":"candidate interpretation UUIDs affected by plausible answers","possible_outcomes":"bounded answer/observation classes with effect notes","decision_relevance":"0-100 scheduling heuristic","acquisition_cost":"0-100 effort/cost heuristic","cheaper_alternative":"optional cheaper way to answer the same discriminator"},"niccolo_rules":{"human_context_is_not_evidence":true,"source_queries_are_retrieval_intents_not_facts":true,"question_value_scores_are_scheduling_heuristics_not_truth_probabilities":true,"candidate_interpretations_are_hypotheses_not_facts":true,"multiple_interpretations_may_coexist":true,"temporal_assertions_keep_their_qualification":true,"later_information_must_not_be_projected_backward_without_support":true,"formal_authority_is_not_practical_influence":true,"inferred_awareness_is_not_observed_knowledge":true,"source_scope_statuses_are_not_evidence_of_misconduct":true,"uncollected_or_unknown_source_families_forbid_claiming_no_evidence_exists":true,"collection_roles_do_not_establish_original_authorship_or_neutrality":true,"summaries_and_transforms_remain_derived_material":true,"byte_or_hash_continuity_does_not_establish_pre_collection_authenticity_or_completeness":true,"decision_readiness_may_coexist_with_unresolved_unknowns":true,"material_reversal_conditions_should_be_exposed_before_stopping":true,"human_owns_consequential_decisions":true,"political_affiliation_does_not_change_misconduct_or_credibility_inference_without_case_specific_connecting_observations":true,"prefer_predictions_that_could_support_or_challenge_live_interpretations":true,"prefer_questions_whose_possible_answers_separate_live_interpretations_or_change_a_real_decision":true,"separate_evidence_from_inference":true,"do_not_execute_human_actions":true}})
            } else if step.kind == "CHALLENGE_SYNTHESIS" {
                json!({"type":"object","required":["purpose","status","niccolo_challenge"],"niccolo_challenge":{"required":["challenges"]},"niccolo_rules":{"seek_counterevidence":true,"seek_competing_interpretations":true,"challenge_is_not_truth":true,"do_not_execute_human_actions":true}})
            } else {
                json!({"type":"object","required":["purpose","status"],"niccolo_rules":{"separate_evidence_from_inference":true,"surface_competing_interpretations":true,"do_not_execute_human_actions":true}})
            },
        };
        let invocation = model_adapter::invoke_recorded(
            pool,
            InvocationScope {
                operation: scope,
                plan_revision,
                step_id: step.step_id,
            },
            actor,
            model,
            &request,
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
        let _ = agentic_controller::finish_step(
            pool,
            scope,
            step.step_id,
            actor,
            "FAILED",
            reason,
        )
        .await;
    }
    result
}

pub async fn run_until_boundary<M: ReasoningModel + ?Sized>(
    pool: &PgPool,
    scope: OperationScope,
    actor: Uuid,
    model: &M,
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
            let already_had_human_review = view
                .steps
                .iter()
                .any(|s| s.kind == "REQUEST_HUMAN_DECISION");
            if quality.human_attention_recommended && !already_had_human_review {
                let reason = format!(
                    "{} open evidence gap(s), {} clarification(s), {} material adversarial challenge(s)",
                    quality.open_evidence_gap_count,
                    quality.clarification_count,
                    quality.open_material_challenge_count
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
            let result = json!({"agentic":true,"plan_revision":view.plan_revision,"plan_sha256":view.plan_sha256,"budget_used":view.budget_used,"steps":view.steps.len(),"quality":quality,"explicit_human_review_occurred":already_had_human_review,"handoff":handoff,"handoff_sha256":handoff_sha256});
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
        execute_step(pool, scope, actor, model, &step, view.plan_revision).await?;
        completed += 1;
    }
}
