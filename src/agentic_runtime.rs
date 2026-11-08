//! v0.291-v0.300: bounded execution loop over the durable agentic DAG.
//! The runtime may execute reasoning steps, but human-required steps always hand control back.
use crate::bff_operations::OperationScope;
use crate::{
    adversarial_challenge, agentic_controller, agentic_policy, analysis_coverage, analysis_quality,
    bff_operations, model_adapter, model_artifacts, operation_budget, reasoning_handoff,
    reasoning_packet, reasoning_updates,
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
    let started = agentic_controller::start_step(pool, scope, step.step_id, actor).await?;
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
            json!({"type":"object","required":["purpose","status","niccolo_update"],"niccolo_update":{"optional":["used_contribution_ids","candidate_interpretations","assumptions","evidence_gaps","clarifications"]},"niccolo_rules":{"human_context_is_not_evidence":true,"separate_evidence_from_inference":true,"do_not_execute_human_actions":true}})
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
                    reasoning_updates::apply(
                        pool,
                        crate::analysis_structure::AnalysisScope {
                            workspace: scope.workspace,
                            situation: scope.situation,
                            analysis,
                        },
                        actor,
                        &update,
                    )
                    .await?;
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
                    let _ = started;
                    Ok(())
                }
                Err(error) => {
                    agentic_controller::finish_step(
                        pool,
                        scope,
                        step.step_id,
                        actor,
                        "FAILED",
                        "model artifact failed validation or application",
                    )
                    .await?;
                    Err(error)
                }
            }
        }
        Err(e) => {
            agentic_controller::finish_step(
                pool,
                scope,
                step.step_id,
                actor,
                "FAILED",
                "model invocation failed",
            )
            .await?;
            Err(e.into())
        }
    }
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
