#![cfg(feature = "pg-integration")]

use niccolo_evidence::{
    analysis_structure::AnalysisScope,
    actors, adversarial_dossier, analysis_snapshot, authz, collection_provenance, decision_points,
    decision_readiness, hypothesis_portfolio, integrity_scope, investigation_policy,
    reasoning_packet, reasoning_updates, source_scope, temporal_state,
};
use sha2::{Digest, Sha256};
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


async fn attach_source_with_speaker(
    admin: &PgPool,
    s: Seed,
    locator: &str,
    content: &str,
    speaker_label: &str,
) -> (Uuid, Uuid) {
    let document = Uuid::new_v4();
    let evidence = Uuid::new_v4();
    let bytes = content.as_bytes();
    let digest = format!("{:x}", Sha256::digest(bytes));
    sqlx::query(
        "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length)
         VALUES($1,$2,'WHATSAPP_EXPORT',$3,$4,$5)",
    )
    .bind(s.workspace)
    .bind(document)
    .bind(locator)
    .bind(&digest)
    .bind(i64::try_from(bytes.len()).unwrap())
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)",
    )
    .bind(s.workspace)
    .bind(document)
    .bind(bytes)
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class)
         VALUES($1,$2,$3,$4,'REPORTED_SPEECH')",
    )
    .bind(s.workspace)
    .bind(evidence)
    .bind(locator)
    .bind(content)
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label)
         VALUES($1,$2,0,$3,0,$4,$5)",
    )
    .bind(s.workspace)
    .bind(document)
    .bind(evidence)
    .bind(i32::try_from(bytes.len()).unwrap())
    .bind(speaker_label)
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_sources(workspace_id,situation_id,document_id,added_by_principal_id)
         VALUES($1,$2,$3,$4)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(document)
    .bind(s.principal)
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id)
         VALUES($1,$2,1,$3)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(document)
    .execute(admin)
    .await
    .unwrap();
    (document, evidence)
}

async fn attach_source(admin: &PgPool, s: Seed, locator: &str, content: &str) -> (Uuid, Uuid) {
    attach_source_with_speaker(admin, s, locator, content, "External operator").await
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
        let forbidden_awareness = temporal_state::record(
            &app,
            scope,
            s.principal,
            &temporal_state::TemporalAssertionInput {
                assertion_id: Uuid::new_v4(),
                subject_label: "Julian".into(),
                assertion_type: "INFERRED_AWARENESS".into(),
                object_text: "Knew the downstream bonus effect".into(),
                valid_from: Some("2026-03-01T00:00:00Z".into()),
                valid_to: Some("2026-03-31T23:59:59Z".into()),
                epistemic_kind: "MACHINE_HYPOTHESIS".into(),
                qualification: "OBSERVED".into(),
                evidence_id: None,
                contribution_id: None,
                rationale: "Even strong communication evidence cannot make internal awareness directly observed.".into(),
            },
        )
        .await;
        assert!(matches!(
            forbidden_awareness,
            Err(temporal_state::TemporalStateError::Invalid)
        ));

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

#[tokio::test]
async fn source_scope_keeps_uncollected_unavailable_and_unknown_distinct() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };

    authz::with_auth_digest("law-firm-test".into(), async {
        source_scope::record(
            &app,
            scope,
            s.principal,
            &source_scope::SourceScopeInput {
                scope_id: Uuid::new_v4(),
                source_family: "Personal mobile messages".into(),
                collection_status: "EXPECTED_NOT_REQUESTED".into(),
                scope_description: "Messages potentially relevant to the review.".into(),
                reason: "Collector declared that personal mobile messages were never collected.".into(),
                collector_label: Some("Elias".into()),
                date_scope: Some("2026-01-01..2026-05-31".into()),
                assertion_kind: "HUMAN_ASSERTION".into(),
                document_id: None,
                supersedes_scope_id: None,
            },
        )
        .await
        .unwrap();
        source_scope::record(
            &app,
            scope,
            s.principal,
            &source_scope::SourceScopeInput {
                scope_id: Uuid::new_v4(),
                source_family: "Retention-deleted mailbox folder".into(),
                collection_status: "KNOWN_UNAVAILABLE".into(),
                scope_description: "One mailbox folder was deleted before collection.".into(),
                reason: "Availability statement supplied by the collector.".into(),
                collector_label: Some("Elias".into()),
                date_scope: None,
                assertion_kind: "HUMAN_ASSERTION".into(),
                document_id: None,
                supersedes_scope_id: None,
            },
        )
        .await
        .unwrap();
        source_scope::record(
            &app,
            scope,
            s.principal,
            &source_scope::SourceScopeInput {
                scope_id: Uuid::new_v4(),
                source_family: "Handwritten March notes".into(),
                collection_status: "UNKNOWN_EXISTENCE".into(),
                scope_description: "Possible handwritten meeting notes.".into(),
                reason: "No one currently knows whether these notes exist.".into(),
                collector_label: None,
                date_scope: Some("2026-03".into()),
                assertion_kind: "HUMAN_ASSERTION".into(),
                document_id: None,
                supersedes_scope_id: None,
            },
        )
        .await
        .unwrap();

        let states = source_scope::list(&app, scope).await.unwrap();
        let statuses = states
            .iter()
            .map(|x| x.collection_status.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(statuses.contains("EXPECTED_NOT_REQUESTED"));
        assert!(statuses.contains("KNOWN_UNAVAILABLE"));
        assert!(statuses.contains("UNKNOWN_EXISTENCE"));
        assert!(states.iter().all(|x| !x.missingness_establishes_misconduct));

        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(packet.source_scope.len(), 3);
    })
    .await;
}

#[tokio::test]
async fn collection_provenance_preserves_roles_without_creating_trust() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let (document, evidence) = attach_source(
        &admin,
        s,
        "operator-transcript.txt",
        "01/01/2026, 00:00 - External operator: Elias summarized the interview.",
    )
    .await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };

    authz::with_auth_digest("law-firm-test".into(), async {
        collection_provenance::record(
            &app,
            scope,
            s.principal,
            &collection_provenance::CollectionEventInput {
                event_id: Uuid::new_v4(),
                actor_label: "Elias".into(),
                actor_role: "SUMMARIZER".into(),
                operation: "Prepared interview summary".into(),
                input_document_id: Some(document),
                input_evidence_id: None,
                output_document_id: Some(document),
                output_evidence_id: Some(evidence),
                occurred_at: Some("2026-05-20T12:00:00Z".into()),
                method: "Human summary of interview material".into(),
                assertion_kind: "HUMAN_ASSERTION".into(),
                qualification: "ATTRIBUTED".into(),
                note: "This event records the asserted transformation role only.".into(),
            },
        )
        .await
        .unwrap();

        let events = collection_provenance::list(&app, scope).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].actor_role, "SUMMARIZER");
        assert!(!events[0].actor_role_establishes_original_authorship);
        assert!(!events[0].actor_role_establishes_neutrality);

        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(packet.collection_events.len(), 1);
        assert_eq!(packet.collection_events[0].output_evidence_id, Some(evidence));
    })
    .await;
}

#[tokio::test]
async fn decision_readiness_can_stop_bounded_investigation_without_omniscience() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let decision = Uuid::new_v4();

    authz::with_auth_digest("law-firm-test".into(), async {
        decision_points::create(
            &app,
            scope,
            decision,
            "What employment consequence is justified for Kessler now?",
            Some("Human decision after bounded investigation"),
            s.principal,
        )
        .await
        .unwrap();
        decision_points::add_option(
            &app,
            scope,
            decision,
            &decision_points::DecisionOption {
                option_id: Uuid::new_v4(),
                ordinal: 1,
                text: "Take no adverse action now".into(),
                tradeoff_note: Some("Preserves status quo while uncertainty remains.".into()),
            },
            s.principal,
        )
        .await
        .unwrap();
        decision_points::add_option(
            &app,
            scope,
            decision,
            &decision_points::DecisionOption {
                option_id: Uuid::new_v4(),
                ordinal: 2,
                text: "Apply the bounded consequence supported by the current record".into(),
                tradeoff_note: Some("Must be revisited if material counterevidence appears.".into()),
            },
            s.principal,
        )
        .await
        .unwrap();

        let assessment = Uuid::new_v4();
        let reversal = Uuid::new_v4();
        let view = decision_readiness::record(
            &app,
            scope,
            s.principal,
            &decision_readiness::DecisionReadinessInput {
                assessment_id: assessment,
                decision_id: decision,
                subject_label: "Kessler".into(),
                readiness: "READY".into(),
                rationale: "The current record is sufficient for this bounded human decision.".into(),
                unresolved_unknowns: "Personal mobile messages remain uncollected.".into(),
                reversal_conditions: vec![decision_readiness::ReversalConditionInput {
                    condition_id: reversal,
                    condition_text: "A contemporaneous message showing contrary authorization.".into(),
                    impact: "MAY_REVERSE".into(),
                    acquisition_note: Some("Reanalyse before acting if recovered.".into()),
                }],
            },
        )
        .await
        .unwrap();
        assert_eq!(view.readiness, "READY");
        assert!(!view.readiness_claims_omniscience);
        assert!(view.human_owns_decision);
        assert_eq!(view.reversal_conditions[0].condition_id, reversal);

        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(packet.decisions.decisions.len(), 1);
        assert_eq!(packet.decision_readiness.len(), 1);
        assert_eq!(packet.decision_readiness[0].readiness, "READY");
        assert!(!packet.decision_readiness[0].readiness_claims_omniscience);
    })
    .await;
}

#[tokio::test]
async fn verified_stored_bytes_do_not_upgrade_authenticity_or_completeness() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let (document, _) = attach_source(
        &admin,
        s,
        "integrity-fixture.txt",
        "01/01/2026, 00:00 - External operator: retained bytes with unknown upstream authenticity.",
    )
    .await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };

    authz::with_auth_digest("law-firm-test".into(), async {
        let assessment = integrity_scope::record(
            &app,
            scope,
            s.principal,
            &integrity_scope::IntegrityAssessmentInput {
                assessment_id: Uuid::new_v4(),
                document_id: document,
                integrity_scope: "STORED_ORIGINAL_BYTES".into(),
                authenticity: "UNKNOWN".into(),
                completeness: "UNKNOWN".into(),
                rationale: "Only post-ingest byte continuity is directly verifiable.".into(),
                assertion_kind: "DOCUMENTARY_OBSERVATION".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(assessment.byte_continuity, "VERIFIED");
        assert_eq!(assessment.authenticity, "UNKNOWN");
        assert_eq!(assessment.completeness, "UNKNOWN");
        assert!(!assessment.byte_continuity_establishes_authenticity);
        assert!(!assessment.byte_continuity_establishes_completeness);

        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        assert_eq!(packet.integrity_assessments.len(), 1);
        assert_eq!(packet.integrity_assessments[0].byte_continuity, "VERIFIED");
        assert_eq!(packet.integrity_assessments[0].authenticity, "UNKNOWN");
    })
    .await;
}

#[tokio::test]
async fn old_niccolo_can_be_a_frozen_observable_actor_without_introspection() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let (document, _) = attach_source_with_speaker(
        &admin,
        s,
        "old-niccolo-transcript.txt",
        "01/01/2026, 00:00 - Historical Niccolo: Kessler appears commercially aggressive but not improper.",
        "Historical Niccolo",
    )
    .await;
    let old_niccolo = Uuid::new_v4();

    authz::with_auth_digest("law-firm-test".into(), async {
        actors::create(
            &app,
            s.workspace,
            s.principal,
            old_niccolo,
            "Historical Niccolo",
        )
        .await
        .unwrap();
        actors::record_kind(
            &app,
            s.workspace,
            s.principal,
            old_niccolo,
            Uuid::new_v4(),
            Some("SOFTWARE"),
            "The observed participant is a historical software system; this is metadata, not an investigation gate.",
        )
        .await
        .unwrap();
        actors::bind_source_speaker(
            &app,
            s.workspace,
            s.principal,
            document,
            "Historical Niccolo",
            Uuid::new_v4(),
            Some(old_niccolo),
            "Bind the transcript speaker label to the observed historical system.",
        )
        .await
        .unwrap();

        let analysis = Uuid::new_v4();
        let snapshot = analysis_snapshot::begin(
            &app,
            s.workspace,
            s.principal,
            s.situation,
            analysis,
            "Investigate observable behavior without requiring internal reasoning access",
            None,
        )
        .await
        .unwrap();
        assert!(snapshot.actor_kind_event_no_max > 0);
        assert!(snapshot.actor_binding_event_no_max > 0);

        let scope = AnalysisScope {
            workspace: s.workspace,
            situation: s.situation,
            analysis,
        };
        let packet = reasoning_packet::build(&app, scope).await.unwrap();
        let binding = packet
            .frozen_input
            .actor_bindings
            .iter()
            .find(|x| x.actor_id == Some(old_niccolo))
            .unwrap();
        assert_eq!(binding.actor_kind.as_deref(), Some("SOFTWARE"));
        assert!(!binding.actor_kind_establishes_mind);
        assert!(!packet.frozen_input.actor_kind_is_investigation_gate);
    })
    .await;
}


#[tokio::test]
async fn adversarial_dossier_packages_review_state_without_claiming_verdict() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let scope = AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    };
    let decision = Uuid::new_v4();

    authz::with_auth_digest("law-firm-test".into(), async {
        source_scope::record(
            &app,
            scope,
            s.principal,
            &source_scope::SourceScopeInput {
                scope_id: Uuid::new_v4(),
                source_family: "Personal mobile messages".into(),
                collection_status: "EXPECTED_NOT_REQUESTED".into(),
                scope_description: "Potentially relevant messages were outside the collected set.".into(),
                reason: "The operator reports that this family was not requested.".into(),
                collector_label: Some("Elias".into()),
                date_scope: Some("2026-01-01..2026-05-31".into()),
                assertion_kind: "HUMAN_ASSERTION".into(),
                document_id: None,
                supersedes_scope_id: None,
            },
        )
        .await
        .unwrap();

        decision_points::create(
            &app,
            scope,
            decision,
            "What consequence, if any, is justified for Kessler now?",
            Some("Human decision after bounded adversarial review"),
            s.principal,
        )
        .await
        .unwrap();
        decision_points::add_option(
            &app,
            scope,
            decision,
            &decision_points::DecisionOption {
                option_id: Uuid::new_v4(),
                ordinal: 1,
                text: "Take no adverse action now".into(),
                tradeoff_note: Some("Preserves the status quo while material uncertainty remains.".into()),
            },
            s.principal,
        )
        .await
        .unwrap();
        decision_readiness::record(
            &app,
            scope,
            s.principal,
            &decision_readiness::DecisionReadinessInput {
                assessment_id: Uuid::new_v4(),
                decision_id: decision,
                subject_label: "Kessler".into(),
                readiness: "READY".into(),
                rationale: "The present record supports a bounded human decision, not omniscience.".into(),
                unresolved_unknowns: "Personal mobile messages remain uncollected.".into(),
                reversal_conditions: vec![decision_readiness::ReversalConditionInput {
                    condition_id: Uuid::new_v4(),
                    condition_text: "A contemporaneous authorization message is recovered.".into(),
                    impact: "MAY_REVERSE".into(),
                    acquisition_note: Some("Reanalyse before acting if recovered.".into()),
                }],
            },
        )
        .await
        .unwrap();

        let dossier = adversarial_dossier::build(&app, scope).await.unwrap();
        assert_eq!(dossier.format, "niccolo.adversarial-dossier");
        assert_eq!(dossier.version, 1);
        assert!(dossier.intended_for_adversarial_review);
        assert!(!dossier.dossier_asserts_verdict);
        assert!(!dossier.dossier_asserts_source_authenticity);
        assert!(!dossier.dossier_asserts_collection_completeness);
        assert!(!dossier.dossier_grants_authority);
        assert!(!dossier.dossier_executes_actions);
        assert_eq!(dossier.reasoning_packet.source_scope.len(), 1);
        assert_eq!(dossier.human_handoff.source_scope.len(), 1);
        assert_eq!(dossier.human_handoff.decision_readiness.len(), 1);
        assert_eq!(dossier.human_handoff.decision_readiness[0].readiness, "READY");

        let first = adversarial_dossier::sha256(&dossier).unwrap();
        let second = adversarial_dossier::sha256(&dossier).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    })
    .await;
}
