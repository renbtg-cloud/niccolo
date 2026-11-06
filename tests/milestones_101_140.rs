use niccolo_next::analysis_assumptions::{
    self, AlternativeInput, AssumptionEvent, AssumptionInput,
};
use niccolo_next::analysis_claims::{self, NewClaim};
use niccolo_next::analysis_structure::AnalysisScope;
use niccolo_next::decision_points::{self, DecisionOption, SelectionEvent};
use niccolo_next::human_agency::{
    self, DecisionRationaleInput, EvidenceGapEvent, EvidenceGapInput, GoalEvent, GoalInput,
    NextStepEvent, NextStepInput,
};
use niccolo_next::human_briefing;
use niccolo_next::review_checklist::{self, ChecklistEvent};
use niccolo_next::review_workflow::{self, ReviewSessionScope};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    other: Uuid,
    analysis: Uuid,
}

async fn pools() -> (PgPool, PgPool) {
    let admin = std::env::var("NICCOLO_NEXT_ADMIN_URL")
        .unwrap_or_else(|_| "postgres://oai@127.0.0.1:55432/niccolo_next_test".into());
    let app = std::env::var("NICCOLO_NEXT_APP_URL")
        .unwrap_or_else(|_| "postgres://niccolo_next_app@127.0.0.1:55432/niccolo_next_test".into());
    (
        PgPoolOptions::new()
            .max_connections(8)
            .connect(&admin)
            .await
            .unwrap(),
        PgPoolOptions::new()
            .max_connections(32)
            .connect(&app)
            .await
            .unwrap(),
    )
}

async fn seed(admin: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        principal: Uuid::new_v4(),
        other: Uuid::new_v4(),
        analysis: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id,display_name) VALUES($1,'human'),($2,'other')").bind(s.principal).bind(s.other).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,name) VALUES($1,'w')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'current focus',$3,2)").bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id) VALUES($1,$2,1,'old focus',$3,$5),($1,$2,2,'current focus',$4,$5)").bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind("b".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'answer','bounded',$4)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.principal).execute(admin).await.unwrap();
    s
}
fn scope(s: Seed) -> AnalysisScope {
    AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    }
}
async fn claim(app: &PgPool, s: Seed) -> Uuid {
    let id = Uuid::new_v4();
    analysis_claims::record_claim(
        app,
        s.workspace,
        s.situation,
        s.analysis,
        s.principal,
        NewClaim {
            claim_id: id,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "provisional interpretation".into(),
            qualification: Some("bounded".into()),
        },
    )
    .await
    .unwrap();
    id
}
async fn session(app: &PgPool, s: Seed) -> ReviewSessionScope {
    let id = Uuid::new_v4();
    review_workflow::start_session(
        app,
        s.workspace,
        s.situation,
        id,
        Some(s.analysis),
        "review",
        s.principal,
    )
    .await
    .unwrap();
    ReviewSessionScope {
        workspace: s.workspace,
        situation: s.situation,
        session: id,
    }
}

#[tokio::test]
async fn v101_v105_checklist_is_human_owned_append_only_and_hashable() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let sess = session(&app, s).await;
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    review_checklist::create_item(&app, sess, a, "inspect assumptions", s.principal)
        .await
        .unwrap();
    review_checklist::create_item(&app, sess, b, "check alternatives", s.principal)
        .await
        .unwrap();
    let open = review_checklist::summary(&app, sess).await.unwrap();
    assert_eq!(open.open_count, 2);
    assert!(!open.complete_for_session);
    review_checklist::append_event(
        &app,
        sess,
        a,
        &ChecklistEvent {
            event_id: Uuid::new_v4(),
            kind: "DONE".into(),
            note: "done by human".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    review_checklist::append_event(
        &app,
        sess,
        b,
        &ChecklistEvent {
            event_id: Uuid::new_v4(),
            kind: "WAIVED".into(),
            note: "human waived for this review".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let closed = review_checklist::summary(&app, sess).await.unwrap();
    assert!(closed.complete_for_session);
    assert!(!closed.complete_means_analysis_approved);
    assert_eq!(
        review_checklist::sha256(&closed).unwrap(),
        review_checklist::sha256(&closed).unwrap()
    );
    review_checklist::append_event(
        &app,
        sess,
        a,
        &ChecklistEvent {
            event_id: Uuid::new_v4(),
            kind: "REOPENED".into(),
            note: "look again".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert!(
        !review_checklist::summary(&app, sess)
            .await
            .unwrap()
            .complete_for_session
    );
}

#[tokio::test]
async fn v101_checklist_duplicate_creation_is_idempotent() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let sess = session(&app, s).await;
    let id = Uuid::new_v4();
    let mut joins = Vec::new();
    for _ in 0..24 {
        let p = app.clone();
        joins.push(tokio::spawn(async move {
            review_checklist::create_item(&p, sess, id, "same item", s.principal).await
        }));
    }
    for j in joins {
        j.await.unwrap().unwrap();
    }
    let x = review_checklist::summary(&app, sess).await.unwrap();
    assert_eq!(x.items.len(), 1);
}

#[tokio::test]
async fn v106_v111_decisions_are_ordered_human_choices_not_execution() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let d = Uuid::new_v4();
    decision_points::create(
        &app,
        scope(s),
        d,
        "Which path?",
        Some("human decides"),
        s.principal,
    )
    .await
    .unwrap();
    let o2 = DecisionOption {
        option_id: Uuid::new_v4(),
        ordinal: 2,
        text: "Path B".into(),
        tradeoff_note: Some("cost B".into()),
    };
    let o1 = DecisionOption {
        option_id: Uuid::new_v4(),
        ordinal: 1,
        text: "Path A".into(),
        tradeoff_note: Some("cost A".into()),
    };
    decision_points::add_option(&app, scope(s), d, &o2, s.principal)
        .await
        .unwrap();
    decision_points::add_option(&app, scope(s), d, &o1, s.principal)
        .await
        .unwrap();
    decision_points::append_selection(
        &app,
        scope(s),
        d,
        &SelectionEvent {
            event_id: Uuid::new_v4(),
            kind: "SELECTED".into(),
            option_id: Some(o2.option_id),
            note: "human chose B".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let p = decision_points::package(&app, scope(s)).await.unwrap();
    let v = &p.decisions[0];
    assert_eq!(
        v.options.iter().map(|x| x.ordinal).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(v.selected_option_id, Some(o2.option_id));
    assert!(!v.options_are_automatically_ranked);
    assert!(!v.selection_means_executed);
    assert!(!v.selection_means_system_recommendation);
    assert!(p.human_owns_decision);
    assert!(!p.package_executes_actions);
    assert_eq!(
        decision_points::sha256(&p).unwrap(),
        decision_points::sha256(&p).unwrap()
    );
}

#[tokio::test]
async fn v108_v110_selection_can_be_cleared_and_reopened_without_action() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let d = Uuid::new_v4();
    decision_points::create(&app, scope(s), d, "Choose?", None, s.principal)
        .await
        .unwrap();
    let o = DecisionOption {
        option_id: Uuid::new_v4(),
        ordinal: 1,
        text: "A".into(),
        tradeoff_note: None,
    };
    decision_points::add_option(&app, scope(s), d, &o, s.principal)
        .await
        .unwrap();
    for (k, opt) in [
        ("SELECTED", Some(o.option_id)),
        ("CLEARED", None),
        ("REOPENED", None),
    ] {
        decision_points::append_selection(
            &app,
            scope(s),
            d,
            &SelectionEvent {
                event_id: Uuid::new_v4(),
                kind: k.into(),
                option_id: opt,
                note: "human event".into(),
                recorded_by: s.principal,
            },
        )
        .await
        .unwrap();
    }
    let p = decision_points::package(&app, scope(s)).await.unwrap();
    assert_eq!(p.decisions[0].current_state, "REOPENED");
    assert_eq!(p.decisions[0].selected_option_id, None);
    assert!(!p.package_executes_actions);
}

#[tokio::test]
async fn v107_selection_rejects_option_from_another_decision() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let d1 = Uuid::new_v4();
    let d2 = Uuid::new_v4();
    decision_points::create(&app, scope(s), d1, "D1?", None, s.principal)
        .await
        .unwrap();
    decision_points::create(&app, scope(s), d2, "D2?", None, s.principal)
        .await
        .unwrap();
    let o = DecisionOption {
        option_id: Uuid::new_v4(),
        ordinal: 1,
        text: "foreign".into(),
        tradeoff_note: None,
    };
    decision_points::add_option(&app, scope(s), d2, &o, s.principal)
        .await
        .unwrap();
    let r = decision_points::append_selection(
        &app,
        scope(s),
        d1,
        &SelectionEvent {
            event_id: Uuid::new_v4(),
            kind: "SELECTED".into(),
            option_id: Some(o.option_id),
            note: "wrong decision".into(),
            recorded_by: s.principal,
        },
    )
    .await;
    assert!(r.is_err());
}

#[tokio::test]
async fn v112_v117_assumptions_and_alternatives_are_explicit_not_truth_or_ranking() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    let aid = Uuid::new_v4();
    analysis_assumptions::record(
        &app,
        scope(s),
        &AssumptionInput {
            assumption_id: aid,
            claim_id: Some(c),
            statement: "Assume timing reported is approximate".into(),
            basis_note: "user report is imprecise".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    analysis_assumptions::append_event(
        &app,
        scope(s),
        aid,
        &AssumptionEvent {
            event_id: Uuid::new_v4(),
            kind: "CHALLENGED".into(),
            note: "human challenges timing".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    analysis_assumptions::append_event(
        &app,
        scope(s),
        aid,
        &AssumptionEvent {
            event_id: Uuid::new_v4(),
            kind: "RETAINED_FOR_ANALYSIS".into(),
            note: "retain provisionally".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    analysis_assumptions::add_alternative(
        &app,
        scope(s),
        &AlternativeInput {
            alternative_id: Uuid::new_v4(),
            claim_id: c,
            text: "Alternative interpretation".into(),
            rationale: "credible competing reading".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let lens = analysis_assumptions::lens(&app, scope(s)).await.unwrap();
    assert_eq!(lens.assumptions[0].latest_status, "RETAINED_FOR_ANALYSIS");
    assert!(!lens.assumptions_are_facts);
    assert_eq!(lens.alternatives.len(), 1);
    assert!(!lens.alternatives[0].automatically_ranked);
    assert!(!lens.alternatives_are_automatically_ranked);
    assert_eq!(
        analysis_assumptions::sha256(&lens).unwrap(),
        analysis_assumptions::sha256(&lens).unwrap()
    );
}

#[tokio::test]
async fn v112_assumption_cannot_reference_claim_from_another_analysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let foreign = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'foreign',$5)").bind(s.workspace).bind(s.situation).bind(foreign).bind("a".repeat(64)).bind(s.principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'foreign answer','bounded',$4)").bind(s.workspace).bind(s.situation).bind(foreign).bind(s.principal).execute(&admin).await.unwrap();
    let foreign_claim = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        foreign,
        s.principal,
        NewClaim {
            claim_id: foreign_claim,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "foreign".into(),
            qualification: None,
        },
    )
    .await
    .unwrap();
    let r = analysis_assumptions::record(
        &app,
        scope(s),
        &AssumptionInput {
            assumption_id: Uuid::new_v4(),
            claim_id: Some(foreign_claim),
            statement: "wrong scope".into(),
            basis_note: "test".into(),
            recorded_by: s.principal,
        },
    )
    .await;
    assert!(r.is_err());
}

#[tokio::test]
async fn v118_v120_briefing_reports_staleness_and_never_decides_or_executes() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    analysis_assumptions::record(
        &app,
        scope(s),
        &AssumptionInput {
            assumption_id: Uuid::new_v4(),
            claim_id: Some(c),
            statement: "assumption".into(),
            basis_note: "basis".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let d = Uuid::new_v4();
    decision_points::create(&app, scope(s), d, "choose?", None, s.principal)
        .await
        .unwrap();
    let b = human_briefing::build(&app, scope(s)).await.unwrap();
    assert_eq!((b.input_revision, b.current_revision), (1, 2));
    assert!(!b.input_is_current);
    assert_eq!(b.assumption_count, 1);
    assert_eq!(b.decision_count, 1);
    assert!(!b.automatically_ranked);
    assert!(!b.briefing_decides_for_human);
    assert!(!b.briefing_starts_analysis);
    assert!(!b.briefing_executes_actions);
    assert_eq!(
        human_briefing::sha256(&b).unwrap(),
        human_briefing::sha256(&b).unwrap()
    );
}

#[tokio::test]
async fn v121_v124_decision_rationale_is_attributed_but_does_not_decide() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let d = Uuid::new_v4();
    decision_points::create(&app, scope(s), d, "choose?", None, s.principal)
        .await
        .unwrap();
    let o = DecisionOption {
        option_id: Uuid::new_v4(),
        ordinal: 1,
        text: "A".into(),
        tradeoff_note: Some("tradeoff".into()),
    };
    decision_points::add_option(&app, scope(s), d, &o, s.principal)
        .await
        .unwrap();
    human_agency::record_decision_rationale(
        &app,
        scope(s),
        d,
        &DecisionRationaleInput {
            rationale_id: Uuid::new_v4(),
            option_id: Some(o.option_id),
            rationale: "my reason".into(),
            tradeoff_ack: Some("I understand cost".into()),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let xs = human_agency::decision_rationales(&app, scope(s), d)
        .await
        .unwrap();
    assert_eq!(xs.len(), 1);
    assert_eq!(xs[0].recorded_by, s.principal);
    assert!(!xs[0].rationale_decides_for_human);
}

#[tokio::test]
async fn v125_v129_evidence_gap_satisfaction_does_not_make_claim_true_and_can_reopen() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    let gap = Uuid::new_v4();
    human_agency::record_evidence_gap(
        &app,
        scope(s),
        &EvidenceGapInput {
            gap_id: gap,
            claim_id: Some(c),
            description: "need original source".into(),
            requested_source_kind: Some("document".into()),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let before = human_agency::evidence_gaps(&app, scope(s)).await.unwrap();
    assert_eq!(before[0].state, "OPEN");
    assert!(!before[0].automatically_ranked);
    let ev = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,evidence_id,document_id,source_start,source_end) VALUES($1,$2,$3,0,10)").bind(s.workspace).bind(ev).bind(Uuid::new_v4()).execute(&admin).await.unwrap();
    human_agency::append_gap_event(
        &app,
        scope(s),
        gap,
        &EvidenceGapEvent {
            event_id: Uuid::new_v4(),
            kind: "SATISFIED".into(),
            evidence_id: Some(ev),
            note: "source found".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let sat = human_agency::evidence_gaps(&app, scope(s)).await.unwrap();
    assert_eq!(sat[0].state, "SATISFIED");
    assert!(!sat[0].satisfied_means_claim_true);
    human_agency::append_gap_event(
        &app,
        scope(s),
        gap,
        &EvidenceGapEvent {
            event_id: Uuid::new_v4(),
            kind: "REOPENED".into(),
            evidence_id: None,
            note: "human wants more".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        human_agency::evidence_gaps(&app, scope(s)).await.unwrap()[0].state,
        "REOPENED"
    );
}

#[tokio::test]
async fn v130_v134_goals_are_human_owned_with_explicit_success_criteria() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let g = Uuid::new_v4();
    human_agency::create_goal(
        &app,
        s.workspace,
        s.situation,
        &GoalInput {
            goal_id: g,
            goal: "understand situation".into(),
            success_criterion: "human can explain tradeoffs".into(),
            owner: s.principal,
            created_by: s.principal,
        },
    )
    .await
    .unwrap();
    let a = human_agency::goals(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(a[0].state, "ACTIVE");
    assert_eq!(a[0].owner, s.principal);
    assert!(!a[0].system_owns_goal);
    for k in ["ACHIEVED", "REOPENED", "ABANDONED"] {
        human_agency::append_goal_event(
            &app,
            s.workspace,
            s.situation,
            g,
            &GoalEvent {
                event_id: Uuid::new_v4(),
                kind: k.into(),
                note: "human state change".into(),
                recorded_by: s.principal,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            human_agency::goals(&app, s.workspace, s.situation)
                .await
                .unwrap()[0]
                .state,
            k
        );
    }
}

#[tokio::test]
async fn v135_v140_next_steps_are_unranked_and_acceptance_is_not_execution() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let step = Uuid::new_v4();
    human_agency::propose_next_step(
        &app,
        scope(s),
        &NextStepInput {
            step_id: step,
            kind: "ASK_HUMAN".into(),
            text: "Ask whether timing matters".into(),
            rationale: "high information value".into(),
            proposed_by: s.principal,
        },
    )
    .await
    .unwrap();
    let p = human_agency::next_steps(&app, scope(s)).await.unwrap();
    assert_eq!(p.steps[0].state, "PROPOSED");
    assert!(!p.steps[0].automatically_ranked);
    assert!(!p.steps[0].accepted_means_executed);
    assert!(!p.steps[0].system_may_execute_without_human);
    assert!(p.human_owns_next_action);
    assert!(!p.package_executes_actions);
    human_agency::append_next_step_event(
        &app,
        scope(s),
        step,
        &NextStepEvent {
            event_id: Uuid::new_v4(),
            kind: "ACCEPTED".into(),
            note: "human accepts suggestion".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let a = human_agency::next_steps(&app, scope(s)).await.unwrap();
    assert_eq!(a.steps[0].state, "ACCEPTED");
    assert!(!a.steps[0].accepted_means_executed);
    assert_eq!(
        human_agency::next_steps_sha256(&a).unwrap(),
        human_agency::next_steps_sha256(&a).unwrap()
    );
}

#[tokio::test]
async fn v135_invalid_next_step_kind_is_rejected() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let r = human_agency::propose_next_step(
        &app,
        scope(s),
        &NextStepInput {
            step_id: Uuid::new_v4(),
            kind: "AUTO_EXECUTE".into(),
            text: "do it".into(),
            rationale: "bad".into(),
            proposed_by: s.principal,
        },
    )
    .await;
    assert!(r.is_err());
}

use niccolo_next::human_control::{
    self, ConsequenceInput, ConstraintEvent, ConstraintInput, PriorityInput, StopConditionEvent,
    StopConditionInput,
};

#[tokio::test]
async fn v141_v145_priorities_are_human_declared_and_never_auto_reordered() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    human_control::declare_priority(
        &app,
        s.workspace,
        s.situation,
        &PriorityInput {
            priority_id: Uuid::new_v4(),
            ordinal: 2,
            text: "preserve provenance".into(),
            rationale: "auditability".into(),
            declared_by: s.principal,
        },
    )
    .await
    .unwrap();
    human_control::declare_priority(
        &app,
        s.workspace,
        s.situation,
        &PriorityInput {
            priority_id: Uuid::new_v4(),
            ordinal: 1,
            text: "human control".into(),
            rationale: "project belongs to user".into(),
            declared_by: s.principal,
        },
    )
    .await
    .unwrap();
    let p = human_control::priorities(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(p.iter().map(|x| x.ordinal).collect::<Vec<_>>(), vec![1, 2]);
    assert!(p.iter().all(|x| !x.automatically_reordered));
}

#[tokio::test]
async fn v146_v150_constraints_require_human_waiver_and_reinstatement() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let id = Uuid::new_v4();
    human_control::declare_constraint(
        &app,
        s.workspace,
        s.situation,
        &ConstraintInput {
            constraint_id: id,
            kind: "MUST_NOT".into(),
            text: "do not execute external actions automatically".into(),
            declared_by: s.principal,
        },
    )
    .await
    .unwrap();
    let c = human_control::constraints(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(c[0].state, "ACTIVE");
    assert!(!c[0].system_may_waive);
    human_control::append_constraint_event(
        &app,
        s.workspace,
        s.situation,
        id,
        &ConstraintEvent {
            event_id: Uuid::new_v4(),
            kind: "WAIVED".into(),
            note: "human explicitly waives for this situation".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        human_control::constraints(&app, s.workspace, s.situation)
            .await
            .unwrap()[0]
            .state,
        "WAIVED"
    );
    human_control::append_constraint_event(
        &app,
        s.workspace,
        s.situation,
        id,
        &ConstraintEvent {
            event_id: Uuid::new_v4(),
            kind: "REINSTATED".into(),
            note: "restore boundary".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        human_control::constraints(&app, s.workspace, s.situation)
            .await
            .unwrap()[0]
            .state,
        "REINSTATED"
    );
}

#[tokio::test]
async fn v151_v155_projected_consequences_are_not_facts_or_recommendations() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let d = Uuid::new_v4();
    decision_points::create(&app, scope(s), d, "Ship now?", None, s.principal)
        .await
        .unwrap();
    let o = DecisionOption {
        option_id: Uuid::new_v4(),
        ordinal: 1,
        text: "ship".into(),
        tradeoff_note: None,
    };
    decision_points::add_option(&app, scope(s), d, &o, s.principal)
        .await
        .unwrap();
    human_control::add_consequence(
        &app,
        scope(s),
        d,
        o.option_id,
        &ConsequenceInput {
            consequence_id: Uuid::new_v4(),
            text: "could shorten feedback cycle".into(),
            uncertainty_note: "depends on adoption".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let c = human_control::consequences(&app, scope(s), d, o.option_id)
        .await
        .unwrap();
    assert_eq!(c.len(), 1);
    assert!(!c[0].projection_is_fact);
    assert!(!c[0].automatically_recommends_option);
}

#[tokio::test]
async fn v156_v160_triggered_stop_condition_blocks_automation_and_requires_human() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let id = Uuid::new_v4();
    human_control::add_stop_condition(
        &app,
        scope(s),
        &StopConditionInput {
            condition_id: id,
            text: "source identity becomes uncertain".into(),
            escalation_note: "ask human before proceeding".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let before = human_control::stop_conditions(&app, scope(s))
        .await
        .unwrap();
    assert!(before[0].automation_allowed);
    assert!(!before[0].requires_human_handling);
    human_control::append_stop_event(
        &app,
        scope(s),
        id,
        &StopConditionEvent {
            event_id: Uuid::new_v4(),
            kind: "TRIGGERED".into(),
            note: "identity conflict detected".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let triggered = human_control::stop_conditions(&app, scope(s))
        .await
        .unwrap();
    assert!(!triggered[0].automation_allowed);
    assert!(triggered[0].requires_human_handling);
    human_control::append_stop_event(
        &app,
        scope(s),
        id,
        &StopConditionEvent {
            event_id: Uuid::new_v4(),
            kind: "CLEARED".into(),
            note: "human resolved identity".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert!(
        human_control::stop_conditions(&app, scope(s))
            .await
            .unwrap()[0]
            .automation_allowed
    );
}

use niccolo_next::human_handoff::{
    self, ClaimAssessmentInput, ConflictInput, SourceRequestEvent, SourceRequestInput,
};

#[tokio::test]
async fn v161_v165_source_request_acceptance_never_fetches_automatically() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let id = Uuid::new_v4();
    human_handoff::propose_source_request(
        &app,
        scope(s),
        &SourceRequestInput {
            request_id: id,
            text: "obtain original contract".into(),
            source_kind: "primary document".into(),
            rationale: "resolve provenance gap".into(),
            proposed_by: s.principal,
        },
    )
    .await
    .unwrap();
    human_handoff::append_source_request_event(
        &app,
        scope(s),
        id,
        &SourceRequestEvent {
            event_id: Uuid::new_v4(),
            kind: "ACCEPTED".into(),
            note: "human approves request".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let r = human_handoff::source_requests(&app, scope(s))
        .await
        .unwrap();
    assert_eq!(r[0].state, "ACCEPTED");
    assert!(!r[0].accepted_means_fetched);
}

#[tokio::test]
async fn v166_v170_claim_assessment_is_purpose_bound_and_not_truth() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    human_handoff::assess_claim(
        &app,
        scope(s),
        c,
        &ClaimAssessmentInput {
            assessment_id: Uuid::new_v4(),
            purpose: "decide whether to ask another question".into(),
            assessment: "SUFFICIENT_FOR_PURPOSE".into(),
            basis_note: "enough for this narrow decision".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let a = human_handoff::claim_assessments(&app, scope(s), c)
        .await
        .unwrap();
    assert_eq!(a.len(), 1);
    assert!(!a[0].establishes_truth);
    assert!(!a[0].reusable_for_other_purposes);
}

#[tokio::test]
async fn v171_v175_conflict_set_stays_explicit_and_unresolved() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let a = claim(&app, s).await;
    let b = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis,
        s.principal,
        NewClaim {
            claim_id: b,
            ordinal: 2,
            kind: "INFERENCE".into(),
            text: "competing claim".into(),
            qualification: Some("provisional".into()),
        },
    )
    .await
    .unwrap();
    let id = Uuid::new_v4();
    human_handoff::record_conflict(
        &app,
        scope(s),
        &ConflictInput {
            conflict_id: id,
            description: "claims cannot both hold under current framing".into(),
            claim_ids: vec![a, b],
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let c = human_handoff::conflicts(&app, scope(s)).await.unwrap();
    assert_eq!(c[0].claim_ids.len(), 2);
    assert!(!c[0].automatically_resolved);
}

#[tokio::test]
async fn v176_v180_handback_is_deterministic_and_returns_control_to_human() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    human_agency::create_goal(
        &app,
        s.workspace,
        s.situation,
        &GoalInput {
            goal_id: Uuid::new_v4(),
            goal: "reach a human decision".into(),
            success_criterion: "I can explain why I chose it".into(),
            owner: s.principal,
            created_by: s.principal,
        },
    )
    .await
    .unwrap();
    human_control::declare_priority(
        &app,
        s.workspace,
        s.situation,
        &PriorityInput {
            priority_id: Uuid::new_v4(),
            ordinal: 1,
            text: "preserve agency".into(),
            rationale: "user owns project".into(),
            declared_by: s.principal,
        },
    )
    .await
    .unwrap();
    human_agency::propose_next_step(
        &app,
        scope(s),
        &NextStepInput {
            step_id: Uuid::new_v4(),
            kind: "ASK_HUMAN".into(),
            text: "choose whether to continue".into(),
            rationale: "decision belongs to human".into(),
            proposed_by: s.principal,
        },
    )
    .await
    .unwrap();
    let h = human_handoff::handback(&app, scope(s)).await.unwrap();
    assert_eq!(h.goal_count, 1);
    assert_eq!(h.priority_count, 1);
    assert_eq!(h.proposed_or_open_next_step_count, 1);
    assert!(h.human_owns_project);
    assert!(!h.system_may_execute);
    assert!(!h.system_may_rewrite_goals);
    assert_eq!(
        human_handoff::handback_sha256(&h).unwrap(),
        human_handoff::handback_sha256(&h).unwrap()
    );
}
