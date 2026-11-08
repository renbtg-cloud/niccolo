#![cfg(feature = "pg-integration")]

use niccolo_evidence::{
    analysis_structure::AnalysisScope,
    authz, hypothesis_portfolio, investigation_policy, reasoning_packet, reasoning_updates,
    temporal_state,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    principal: Uuid,
}

async fn pools() -> (PgPool, PgPool) {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").expect("admin DB"))
        .await
        .unwrap();
    let app = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").expect("service DB"))
        .await
        .unwrap();
    (admin, app)
}

async fn seed(admin: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        analysis: Uuid::new_v4(),
        principal: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'law-firm-capabilities')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'Investigate competing explanations',$3,1)")
        .bind(s.workspace)
        .bind(s.situation)
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'Investigate competing explanations',$3,$4,'CREATED')")
        .bind(s.workspace)
        .bind(s.situation)
        .bind("b".repeat(64))
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'separate live explanations and identify decision-relevant observations',$5)")
        .bind(s.workspace)
        .bind(s.situation)
        .bind(s.analysis)
        .bind("b".repeat(64))
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    s
}

#[tokio::test]
async fn discriminating_question_value_can_outrank_expensive_generic_question() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let h1 = Uuid::new_v4();
    let h2 = Uuid::new_v4();
    let cheap = Uuid::new_v4();
    let expensive = Uuid::new_v4();

    let update = reasoning_updates::ReasoningUpdate {
        used_contribution_ids: vec![],
        candidate_interpretations: vec![
            reasoning_updates::UpdateInterpretation {
                interpretation_id: h1,
                text: "The timing was driven by the new incentive rule.".into(),
                rationale: "The observed sequence is compatible with incentive timing.".into(),
                predictions: vec![],
                evidence_links: vec![],
                relations: vec![],
            },
            reasoning_updates::UpdateInterpretation {
                interpretation_id: h2,
                text: "The timing was ordinary and unrelated to the new rule.".into(),
                rationale: "Ordinary operational timing remains plausible.".into(),
                predictions: vec![],
                evidence_links: vec![],
                relations: vec![],
            },
        ],
        assumptions: vec![],
        evidence_gaps: vec![],
        clarifications: vec![
            reasoning_updates::UpdateClarification {
                question_id: cheap,
                text: "What was the effective date of the rule?".into(),
                reason: "The answer separates the two live timing explanations.".into(),
                discriminates_interpretation_ids: vec![h1, h2],
                possible_outcomes: vec![
                    reasoning_updates::UpdateClarificationOutcome {
                        outcome_id: Uuid::new_v4(),
                        label: "Before the disputed event".into(),
                        effect_note: "Keeps incentive timing live.".into(),
                    },
                    reasoning_updates::UpdateClarificationOutcome {
                        outcome_id: Uuid::new_v4(),
                        label: "After the disputed event".into(),
                        effect_note: "Materially weakens incentive timing.".into(),
                    },
                ],
                decision_relevance: Some(100),
                acquisition_cost: Some(5),
                cheaper_alternative: None,
            },
            reasoning_updates::UpdateClarification {
                question_id: expensive,
                text: "Reconstruct five years of browser history.".into(),
                reason: "Potentially interesting background.".into(),
                discriminates_interpretation_ids: vec![],
                possible_outcomes: vec![reasoning_updates::UpdateClarificationOutcome {
                    outcome_id: Uuid::new_v4(),
                    label: "Large heterogeneous dataset".into(),
                    effect_note: "No specific decision-changing branch identified.".into(),
                }],
                decision_relevance: Some(10),
                acquisition_cost: Some(95),
                cheaper_alternative: Some("Check the dated policy publication first.".into()),
            },
        ],
        source_queries: vec![],
    };

    authz::with_auth_digest("law-firm-test".into(), async {
        reasoning_updates::apply(&app, scope, s.principal, &update)
            .await
            .unwrap();
        let queue = investigation_policy::queue(&app, scope, 32).await.unwrap();
        let cheap_item = queue.items.iter().find(|x| x.id == cheap).unwrap();
        let expensive_item = queue.items.iter().find(|x| x.id == expensive).unwrap();
        assert!(cheap_item.information_value_score > expensive_item.information_value_score);
        assert!(cheap_item.information_value_score > 86);
        assert!(expensive_item.information_value_score < 60);
        assert!(!queue.ranking_establishes_truth);
    })
    .await;
}

#[tokio::test]
async fn hypothesis_portfolio_keeps_predictions_relations_and_human_steering() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let h1 = Uuid::new_v4();
    let h2 = Uuid::new_v4();
    let prediction = Uuid::new_v4();
    let relation = Uuid::new_v4();

    let update = reasoning_updates::ReasoningUpdate {
        used_contribution_ids: vec![],
        candidate_interpretations: vec![
            reasoning_updates::UpdateInterpretation {
                interpretation_id: h1,
                text: "The old system was configuration-biased.".into(),
                rationale: "Observed outputs favor one actor.".into(),
                predictions: vec![reasoning_updates::UpdateHypothesisPrediction {
                    prediction_id: prediction,
                    expected_observation: "Changing configuration changes the output pattern.".into(),
                    direction: "SUPPORT".into(),
                    rationale: "A configuration-sensitive shift supports this explanation.".into(),
                }],
                evidence_links: vec![],
                relations: vec![reasoning_updates::UpdateHypothesisRelation {
                    relation_id: relation,
                    other_interpretation_id: h2,
                    relation: "CAN_COEXIST".into(),
                    rationale: "Configuration bias and selection bias can both operate.".into(),
                }],
            },
            reasoning_updates::UpdateInterpretation {
                interpretation_id: h2,
                text: "The collected corpus was selection-biased.".into(),
                rationale: "Collection initially prioritized bonus-related terms.".into(),
                predictions: vec![],
                evidence_links: vec![],
                relations: vec![],
            },
        ],
        assumptions: vec![],
        evidence_gaps: vec![],
        clarifications: vec![],
        source_queries: vec![],
    };

    authz::with_auth_digest("law-firm-test".into(), async {
        reasoning_updates::apply(&app, scope, s.principal, &update)
            .await
            .unwrap();
        let before = hypothesis_portfolio::portfolio(&app, scope).await.unwrap();
        let first = before
            .hypotheses
            .iter()
            .find(|x| x.interpretation_id == h1)
            .unwrap();
        assert_eq!(first.latest_status, "LIVE");
        assert_eq!(first.predictions[0].prediction_id, prediction);
        assert!(first.relations.iter().any(|x| {
            x.other_interpretation_id == h2 && x.relation == "CAN_COEXIST"
        }));
        assert!(!before.forced_single_winner);
        assert!(!before.portfolio_establishes_truth);

        hypothesis_portfolio::append_event(
            &app,
            scope,
            s.principal,
            h1,
            Uuid::new_v4(),
            "WEAKENED",
            "A newly recovered date weakens but does not delete the hypothesis.",
        )
        .await
        .unwrap();
        let after = hypothesis_portfolio::portfolio(&app, scope).await.unwrap();
        assert_eq!(
            after
                .hypotheses
                .iter()
                .find(|x| x.interpretation_id == h1)
                .unwrap()
                .latest_status,
            "WEAKENED"
        );
    })
    .await;
}

#[tokio::test]
async fn temporal_state_keeps_formal_authority_separate_from_practical_influence() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };

    authz::with_auth_digest("law-firm-test".into(), async {
        temporal_state::record(
            &app,
            scope,
            s.principal,
            &temporal_state::TemporalAssertionInput {
                assertion_id: Uuid::new_v4(),
                subject_label: "Camille".into(),
                assertion_type: "FORMAL_AUTHORITY".into(),
                object_text: "Could approve settlement band X".into(),
                valid_from: Some("2026-01-01T00:00:00Z".into()),
                valid_to: Some("2026-03-31T23:59:59Z".into()),
                epistemic_kind: "HUMAN_ASSERTION".into(),
                qualification: "ATTRIBUTED".into(),
                evidence_id: None,
                contribution_id: None,
                rationale: "Operator-prepared authority table supplied for review.".into(),
            },
        )
        .await
        .unwrap();
        temporal_state::record(
            &app,
            scope,
            s.principal,
            &temporal_state::TemporalAssertionInput {
                assertion_id: Uuid::new_v4(),
                subject_label: "Julian".into(),
                assertion_type: "PRACTICAL_INFLUENCE".into(),
                object_text: "Appears to have shaped the ranges ultimately adopted".into(),
                valid_from: Some("2026-01-01T00:00:00Z".into()),
                valid_to: Some("2026-05-31T23:59:59Z".into()),
                epistemic_kind: "MACHINE_HYPOTHESIS".into(),
                qualification: "INFERRED".into(),
                evidence_id: None,
                contribution_id: None,
                rationale: "Multiple threads show proposals later adopted; influence remains inferential.".into(),
            },
        )
        .await
        .unwrap();

        let state = temporal_state::list(&app, scope).await.unwrap();
        assert!(state.iter().any(|x| x.assertion_type == "FORMAL_AUTHORITY"));
        assert!(state.iter().any(|x| x.assertion_type == "PRACTICAL_INFLUENCE"));
        assert!(state.iter().all(|x| !x.assertion_establishes_truth));

        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(packet.temporal_assertions.len(), 2);
        assert!(packet
            .temporal_assertions
            .iter()
            .any(|x| x.qualification == "INFERRED"));
    })
    .await;
}
