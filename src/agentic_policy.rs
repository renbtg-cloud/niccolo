//! v0.261-v0.350: deterministic planner from recorded analysis state and explicit human-agency signals.
//! It chooses investigation structure, not the user's decisions or external actions.
use crate::{agentic_controller::StepInput, analysis_review, bff_intent::HumanIntent};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("analysis review unavailable: {0}")]
    Review(#[from] analysis_review::AnalysisReviewError),
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicySignals {
    pub open_evidence_gaps: i64,
    pub conflict_sets: i64,
    pub triggered_stop_conditions: i64,
    pub source_requests: i64,
}
fn id() -> Uuid {
    Uuid::new_v4()
}
async fn scoped(pool: &PgPool, workspace: Uuid) -> Result<Transaction<'_, Postgres>, PolicyError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
pub async fn signals(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<PolicySignals, PolicyError> {
    let mut tx = scoped(pool, workspace).await?;
    let open_evidence_gaps:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_evidence_gaps g WHERE g.workspace_id=$1 AND g.situation_id=$2 AND g.analysis_id=$3 AND coalesce((SELECT e.event_kind FROM niccolo.situation_analysis_evidence_gap_events e WHERE e.workspace_id=g.workspace_id AND e.situation_id=g.situation_id AND e.analysis_id=g.analysis_id AND e.gap_id=g.gap_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),'OPEN') NOT IN ('SATISFIED','WAIVED')")
      .bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let conflict_sets:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_conflict_sets WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let triggered_stop_conditions:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_stop_conditions c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 AND (SELECT e.event_kind FROM niccolo.situation_analysis_stop_condition_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id AND e.condition_id=c.condition_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1)='TRIGGERED'")
      .bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let source_requests:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_source_requests r WHERE r.workspace_id=$1 AND r.situation_id=$2 AND r.analysis_id=$3 AND coalesce((SELECT e.event_kind FROM niccolo.situation_analysis_source_request_events e WHERE e.workspace_id=r.workspace_id AND e.situation_id=r.situation_id AND e.analysis_id=r.analysis_id AND e.request_id=r.request_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1),'OPEN') NOT IN ('DECLINED')")
      .bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(PolicySignals {
        open_evidence_gaps,
        conflict_sets,
        triggered_stop_conditions,
        source_requests,
    })
}

pub async fn default_plan(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<Vec<StepInput>, PolicyError> {
    plan_for_intent(pool, workspace, situation, analysis, None).await
}

pub async fn plan_for_intent(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    intent: Option<HumanIntent>,
) -> Result<Vec<StepInput>, PolicyError> {
    let r = analysis_review::get(pool, workspace, situation, analysis).await?;
    let sig = signals(pool, workspace, situation, analysis).await?;
    let inspect = id();
    let context = id();
    let stop = id();
    let assumptions = id();
    let alternatives = id();
    let clarify = id();
    let synth = id();
    let challenge = id();
    let human = id();
    let handoff = id();
    let mut steps = vec![
        StepInput {
            step_id: inspect,
            ordinal: 1,
            kind: "INSPECT_SOURCES".into(),
            label: format!(
                "Inspect recorded source scope: {} catalogued / {} examined; {} open evidence gap(s)",
                r.catalogued_fragment_count, r.examined_fragment_count, sig.open_evidence_gaps
            ),
            requires_human: false,
            depends_on: vec![],
        },
        StepInput {
            step_id: context,
            ordinal: 2,
            kind: "ASSESS_CONTEXT".into(),
            label: format!(
                "Assess human context: {} present / {} explicitly used",
                r.input_context_count, r.used_context_count
            ),
            requires_human: false,
            depends_on: vec![inspect],
        },
        StepInput {
            step_id: stop,
            ordinal: 3,
            kind: "CHECK_STOP_CONDITIONS".into(),
            label: format!(
                "Check explicit stop/escalation gates: {} currently triggered",
                sig.triggered_stop_conditions
            ),
            requires_human: false,
            depends_on: vec![context],
        },
        StepInput {
            step_id: assumptions,
            ordinal: 4,
            kind: "CHECK_ASSUMPTIONS".into(),
            label: "Enumerate and challenge material assumptions".into(),
            requires_human: false,
            depends_on: vec![stop],
        },
        StepInput {
            step_id: alternatives,
            ordinal: 5,
            kind: "CHECK_ALTERNATIVES".into(),
            label: format!(
                "Generate credible competing interpretations; {} unresolved conflict set(s)",
                sig.conflict_sets
            ),
            requires_human: false,
            depends_on: vec![assumptions],
        },
    ];
    let clarification_pressure = r.clarification_question_count - r.answered_question_count
        + sig.open_evidence_gaps
        + sig.conflict_sets
        + sig.source_requests;
    let mut synth_deps = vec![alternatives];
    if clarification_pressure > 0 {
        steps.push(StepInput{step_id:clarify,ordinal:6,kind:"PROPOSE_CLARIFICATION".into(),label:format!("Address {} recorded clarification/evidence/conflict/source-request pressure point(s)",clarification_pressure),requires_human:false,depends_on:vec![alternatives]});
        synth_deps = vec![clarify];
    }
    let synthesis_label = match intent {
        Some(HumanIntent::AboutToActOrSay) => {
            "Synthesize decision support before the human acts: consequences, uncertainty, alternatives, and explicit non-execution"
        }
        Some(HumanIntent::Understand) => {
            "Synthesize provisional understanding with evidence/inference separation and competing interpretations"
        }
        Some(HumanIntent::FigureOutWhatMatters) => {
            "Synthesize what appears material, why it matters, uncertainty, and attention-worthy unresolved questions"
        }
        None => "Synthesize provisional answer with evidence/inference separation",
    };
    steps.push(StepInput {
        step_id: synth,
        ordinal: steps.len() as i32 + 1,
        kind: "SYNTHESIZE".into(),
        label: synthesis_label.into(),
        requires_human: false,
        depends_on: synth_deps,
    });
    steps.push(StepInput{step_id:challenge,ordinal:steps.len() as i32+1,kind:"CHALLENGE_SYNTHESIS".into(),label:"Adversarially challenge the synthesis: seek counterevidence, assumption risk, competing explanations and coverage gaps".into(),requires_human:false,depends_on:vec![synth]});
    let needs_human = matches!(intent, Some(HumanIntent::AboutToActOrSay))
        || clarification_pressure > 0
        || sig.triggered_stop_conditions > 0
        || r.added_source_count > 0
        || r.added_context_count > 0
        || r.focus_changed;
    if needs_human {
        steps.push(StepInput{step_id:human,ordinal:steps.len() as i32+1,kind:"REQUEST_HUMAN_DECISION".into(),label:"Return unresolved material choice, stop gate, or source-acquisition decision to the human".into(),requires_human:true,depends_on:vec![challenge]});
        steps.push(StepInput {
            step_id: handoff,
            ordinal: steps.len() as i32 + 1,
            kind: "PREPARE_HANDOFF".into(),
            label: "Prepare deterministic handback with provenance".into(),
            requires_human: false,
            depends_on: vec![human],
        });
    } else {
        steps.push(StepInput {
            step_id: handoff,
            ordinal: steps.len() as i32 + 1,
            kind: "PREPARE_HANDOFF".into(),
            label: "Prepare deterministic handback with provenance".into(),
            requires_human: false,
            depends_on: vec![challenge],
        });
    }
    Ok(steps)
}

/// v1.301-v1.400: follow-up handback created only when reasoning itself discovers
/// material unresolved state after the original plan was constructed.
pub fn human_attention_plan(reason: &str) -> Result<Vec<StepInput>, PolicyError> {
    if reason.trim().is_empty() {
        return Err(PolicyError::Database(sqlx::Error::Protocol(
            "empty attention reason".into(),
        )));
    }
    let human = id();
    let handoff = id();
    Ok(vec![
        StepInput {
            step_id: human,
            ordinal: 1,
            kind: "REQUEST_HUMAN_DECISION".into(),
            label: format!("Human review required after reasoning: {}", reason.trim()),
            requires_human: true,
            depends_on: vec![],
        },
        StepInput {
            step_id: handoff,
            ordinal: 2,
            kind: "PREPARE_HANDOFF".into(),
            label: "Prepare handback after explicit human review".into(),
            requires_human: false,
            depends_on: vec![human],
        },
    ])
}
