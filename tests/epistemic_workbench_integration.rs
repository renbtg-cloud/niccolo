#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    authz,
    epistemic_workbench::{
        self, ArgumentMode, ExpertNodeKind, GroundingAdequacy, GroundingEffect,
        GroundingMemberRole, NewGroundingMember, ReassessmentDisposition, RelationFamily,
        RelationKind, WorkbenchError,
    },
    postgres_ingest::ingest_source,
    situation_store,
    source_import::SourceFormat,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

struct Fixture {
    service: PgPool,
    workspace: Uuid,
    principal: Uuid,
}

async fn fixture() -> Fixture {
    let admin = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .max_connections(12)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'expert-workbench-test')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let _token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    Fixture {
        service,
        workspace,
        principal,
    }
}

async fn situation(f: &Fixture, focus: &str) -> Uuid {
    let situation = Uuid::new_v4();
    situation_store::create(&f.service, f.workspace, f.principal, situation, focus)
        .await
        .unwrap();
    situation
}

async fn node(f: &Fixture, situation: Uuid, kind: ExpertNodeKind, text: &str) -> Uuid {
    let id = Uuid::new_v4();
    epistemic_workbench::create_node(
        &f.service,
        f.workspace,
        situation,
        f.principal,
        id,
        kind,
        text,
        "Explicitly authored by the trained investigator.",
    )
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn investigative_links_expose_presuppositions_without_asserting_them_or_resolving_gaps() {
    let f = fixture().await;
    let sit = situation(&f, "Examine a loaded investigative question").await;
    let other = situation(&f, "Another investigation").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Why did Juan lie?").await;
    let unlinked = node(&f, sit, ExpertNodeKind::Question, "What happened next?").await;
    let _other_question = node(&f, other, ExpertNodeKind::Question, "Unrelated inquiry?").await;
    let g1 = node(
        &f,
        sit,
        ExpertNodeKind::InformationGap,
        "Whether the statement was false",
    )
    .await;
    let g2 = node(
        &f,
        sit,
        ExpertNodeKind::InformationGap,
        "Whether the speaker knew it was false",
    )
    .await;
    let h1 = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The speaker intended to mislead",
    )
    .await;
    let h2 = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The speaker was mistaken",
    )
    .await;
    let p = node(
        &f,
        sit,
        ExpertNodeKind::Proposition,
        "The speaker made the statement",
    )
    .await;
    let foreign = node(&f, other, ExpertNodeKind::InformationGap, "A gap elsewhere").await;
    let link = |id, kind, left, right| {
        epistemic_workbench::create_relation(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            RelationFamily::Contextual,
            kind,
            left,
            right,
            "Investigator-authored relationship; no truth assertion",
        )
    };
    for (kind, target) in [
        (RelationKind::AddressesGap, g1),
        (RelationKind::AddressesGap, g2),
        (RelationKind::TestsHypothesis, h1),
        (RelationKind::TestsHypothesis, h2),
        (RelationKind::Presupposes, p),
        (RelationKind::Presupposes, h1),
    ] {
        let id = Uuid::new_v4();
        assert!(!link(id, kind, q, target).await.unwrap().replayed);
        assert!(link(id, kind, q, target).await.unwrap().replayed);
    }
    assert!(matches!(
        link(Uuid::new_v4(), RelationKind::AddressesGap, g1, q).await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        link(Uuid::new_v4(), RelationKind::TestsHypothesis, q, g1).await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        link(Uuid::new_v4(), RelationKind::Presupposes, q, g1).await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        link(Uuid::new_v4(), RelationKind::AddressesGap, q, foreign).await,
        Err(WorkbenchError::Invisible)
    ));
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(f.workspace.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let invalid = sqlx::query("INSERT INTO niccolo.epistemic_relations(workspace_id,situation_id,relation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id) VALUES($1,$2,$3,'CONTEXTUAL','ADDRESSES_GAP',$4,$5,'Wrong Situation',$6)")
        .bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(q).bind(foreign).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        invalid.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    tx.rollback().await.unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.relations.len(), 6);
    assert!(snapshot.arguments.is_empty());
    assert!(snapshot.assessments.is_empty());
    assert!(snapshot.reassessment_impacts.is_empty());
    assert!(snapshot.reasoning_diagnostics.is_empty());
    assert_eq!(snapshot.question_inquiries.len(), 2);
    let inquiry = snapshot
        .question_inquiries
        .iter()
        .find(|i| i.question_object_id == q)
        .unwrap();
    assert_eq!(inquiry.addressed_gaps.len(), 2);
    assert_eq!(inquiry.tested_hypotheses.len(), 2);
    assert_eq!(inquiry.presuppositions.len(), 2);
    assert!(inquiry.current_discriminatory_power_events.is_empty());
    for link in inquiry
        .addressed_gaps
        .iter()
        .chain(&inquiry.tested_hypotheses)
        .chain(&inquiry.presuppositions)
    {
        assert!(snapshot.relations.iter().any(
            |r| r.relation_id == link.relation_id && r.right_object_id == link.target_object_id
        ));
    }
    let blank = snapshot
        .question_inquiries
        .iter()
        .find(|i| i.question_object_id == unlinked)
        .unwrap();
    assert!(
        blank.addressed_gaps.is_empty()
            && blank.tested_hypotheses.is_empty()
            && blank.presuppositions.is_empty()
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|n| n.object_id == q && n.text == "Why did Juan lie?")
    );
    use epistemic_workbench::assessment::*;
    let input = AssessmentInput {
        target: AssessmentTarget::Object { object_id: q },
        dimension: AssessmentDimension::DiscriminatoryPower,
        result: AssessmentResult::HighPower,
        confidence: AssessmentConfidence::Low,
        criterion: "Distinguishing the candidate explanations".into(),
        context: "Current inquiry".into(),
        rationale: "Investigator judgment; not an inferred score".into(),
        grounds: vec![],
        revises_event_id: None,
    };
    let record = |id, input| {
        epistemic_workbench::assessment::record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            input,
        )
    };
    let initial = Uuid::new_v4();
    let revised = Uuid::new_v4();
    let alternative = Uuid::new_v4();
    let relevance = Uuid::new_v4();
    record(initial, input.clone()).await.unwrap();
    record(
        revised,
        AssessmentInput {
            result: AssessmentResult::LimitedPower,
            revises_event_id: Some(initial),
            ..input.clone()
        },
    )
    .await
    .unwrap();
    record(
        alternative,
        AssessmentInput {
            result: AssessmentResult::NoPower,
            ..input.clone()
        },
    )
    .await
    .unwrap();
    record(
        relevance,
        AssessmentInput {
            dimension: AssessmentDimension::Relevance,
            result: AssessmentResult::Relevant,
            ..input.clone()
        },
    )
    .await
    .unwrap();
    record(
        Uuid::new_v4(),
        AssessmentInput {
            target: AssessmentTarget::Object { object_id: h1 },
            ..input
        },
    )
    .await
    .unwrap();
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let inquiry = after
        .question_inquiries
        .iter()
        .find(|i| i.question_object_id == q)
        .unwrap();
    assert_eq!(inquiry.current_discriminatory_power_events.len(), 2);
    assert!(
        inquiry
            .current_discriminatory_power_events
            .contains(&revised)
    );
    assert!(
        inquiry
            .current_discriminatory_power_events
            .contains(&alternative)
    );
    assert_eq!(inquiry.current_relevance_events, vec![relevance]);
    assert_eq!(after.assessments.len(), 5);
}

#[tokio::test]
async fn pinned_interpretation_changes_require_only_explicit_route_and_downstream_attention() {
    use epistemic_workbench::{
        inquiry_response::*,
        interpretation_dependency::{self, InterpretationDependencyInput},
    };
    use niccolo_evidence::epistemic_identity::MeaningConfirmation;
    let f = fixture().await;
    let sit = situation(&f, "Interpretation dependence").await;
    let other = situation(&f, "Other inquiry").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Who approved it?").await;
    let utterance = Uuid::new_v4();
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        q,
        Uuid::new_v4(),
        ResponseInput {
            utterance_id: utterance,
            utterance_text: "I don't know.".into(),
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
            rationale: "Interview".into(),
        },
    )
    .await
    .unwrap();
    let interpretation = Uuid::new_v4();
    let revision = Uuid::new_v4();
    let meaning = InterpretationInput {
        expected_current_revision_id: None,
        interpretation_text: "The actor claims ignorance.".into(),
        rationale: "Claimed, not actual ignorance".into(),
    };
    record_interpretation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        utterance,
        interpretation,
        revision,
        meaning.clone(),
    )
    .await
    .unwrap();
    let p = node(
        &f,
        sit,
        ExpertNodeKind::Proposition,
        "The person responded to the interview",
    )
    .await;
    let alt = node(&f, sit, ExpertNodeKind::Proposition, "Independent evidence").await;
    let h = node(&f, sit, ExpertNodeKind::Hypothesis, "Candidate explanation").await;
    let downstream = node(&f, sit, ExpertNodeKind::Finding, "Subsequent conclusion").await;
    let mut routes = vec![];
    for (ground, result) in [(p, h), (alt, h), (h, downstream)] {
        let a = Uuid::new_v4();
        let g = Uuid::new_v4();
        epistemic_workbench::create_argument(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            a,
            result,
            ArgumentMode::Abductive,
            "Explicit route",
        )
        .await
        .unwrap();
        epistemic_workbench::create_grounding_set(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            a,
            g,
            GroundingEffect::Supports,
            GroundingAdequacy::Partial,
            "Explicit basis",
            &[NewGroundingMember {
                member_id: Uuid::new_v4(),
                role: GroundingMemberRole::Premise,
                ground_object_id: Some(ground),
                evidence_id: None,
                rationale: "Ground".into(),
            }],
        )
        .await
        .unwrap();
        routes.push((a, g));
    }
    let input = InterpretationDependencyInput {
        argument_id: routes[0].0,
        grounding_set_id: routes[0].1,
        interpretation_id: interpretation,
        revision_id: revision,
        confirmation_event_id: None,
        rationale: "This route provisionally relies on the reported meaning".into(),
    };
    let id = Uuid::new_v4();
    let pin = |scope, id, input| {
        interpretation_dependency::record(&f.service, f.workspace, scope, f.principal, id, input)
    };
    assert!(matches!(
        pin(other, id, input.clone()).await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(matches!(
        pin(
            sit,
            id,
            InterpretationDependencyInput {
                confirmation_event_id: Some(Uuid::new_v4()),
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(!pin(sit, id, input.clone()).await.unwrap().replayed);
    assert!(pin(sit, id, input.clone()).await.unwrap().replayed);
    assert!(
        !epistemic_workbench::snapshot(&f.service, f.workspace, sit)
            .await
            .unwrap()
            .interpretation_dependencies[0]
            .requires_review
    );
    let event = Uuid::new_v4();
    confirm_interpretation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        utterance,
        interpretation,
        revision,
        event,
        ConfirmationInput {
            state: MeaningConfirmation::Confirmed,
            note: "Meaning only".into(),
        },
    )
    .await
    .unwrap();
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let view = &after.interpretation_dependencies[0];
    assert!(
        view.requires_review && view.meaning_confirmation_changed && !view.interpretation_revised
    );
    assert_eq!(view.current_confirmation_event_id, Some(event));
    assert_eq!(view.potential_downstream_routes.len(), 1);
    assert_eq!(view.potential_downstream_routes[0].argument_id, routes[2].0);
    for (index, (a, g)) in routes.iter().enumerate() {
        let diag = after
            .reasoning_diagnostics
            .iter()
            .flat_map(|c| &c.routes)
            .find(|r| r.argument_id == *a && r.grounding_set_id == *g)
            .unwrap();
        assert_eq!(
            diag.interpretation_reassessment.directly_relying.unreviewed,
            usize::from(index == 0)
        );
        assert_eq!(
            diag.interpretation_reassessment.downstream.unreviewed,
            usize::from(index == 2)
        );
        assert_eq!(diag.assessment_reassessment.directly_relying.impacted, 0);
    }
    let second = Uuid::new_v4();
    record_interpretation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        utterance,
        interpretation,
        second,
        InterpretationInput {
            expected_current_revision_id: Some(revision),
            interpretation_text: "The actor claims not to know the approver's identity.".into(),
            ..meaning
        },
    )
    .await
    .unwrap();
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(
        after.interpretation_dependencies[0].current_revision_id,
        second
    );
    assert!(after.interpretation_dependencies[0].interpretation_revised);
    assert_eq!(after.interpretation_dependencies[0].dependency.input, input);
    assert_eq!(after.grounding_sets.len(), 3);
    assert!(after.assessments.is_empty());
}

#[tokio::test]
async fn response_meaning_confirmation_is_revision_specific_and_never_truth() {
    use epistemic_workbench::inquiry_response::*;
    use niccolo_evidence::epistemic_identity::{InterpretationProducer, MeaningConfirmation};
    let f = fixture().await;
    let sit = situation(&f, "Meaning versus truth").await;
    let other = situation(&f, "Unrelated Situation").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Who approved it?").await;
    let q2 = node(
        &f,
        sit,
        ExpertNodeKind::Question,
        "What did the speaker say?",
    )
    .await;
    let utterance = Uuid::new_v4();
    let input = ResponseInput {
        utterance_id: utterance,
        utterance_text: "I don't know.".into(),
        speaker_actor_id: None,
        source_evidence_id: None,
        source_contribution_id: None,
        rationale: "Interview".into(),
    };
    for question in [q, q2] {
        record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            question,
            Uuid::new_v4(),
            input.clone(),
        )
        .await
        .unwrap();
    }
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert!(snapshot.response_interpretations.is_empty());
    let interpretation = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let draft = InterpretationInput {
        expected_current_revision_id: None,
        interpretation_text: "The speaker claims ignorance.".into(),
        rationale: "Do not infer actual ignorance".into(),
    };
    assert!(matches!(
        record_interpretation(
            &f.service,
            f.workspace,
            other,
            f.principal,
            utterance,
            interpretation,
            first,
            draft.clone()
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    let add = |revision, input| {
        record_interpretation(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            utterance,
            interpretation,
            revision,
            input,
        )
    };
    let initial = add(first, draft.clone()).await.unwrap();
    assert!(initial.meaning_is_provisional && !initial.confirmation_establishes_truth);
    assert_eq!(initial.producer, InterpretationProducer::Human);
    assert!(add(first, draft.clone()).await.unwrap().replayed);
    let event = Uuid::new_v4();
    let confirmation = ConfirmationInput {
        state: MeaningConfirmation::Confirmed,
        note: "Accurate description of wording only".into(),
    };
    let confirm = |revision, event, input| {
        confirm_interpretation(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            utterance,
            interpretation,
            revision,
            event,
            input,
        )
    };
    assert!(matches!(
        confirm(Uuid::new_v4(), event, confirmation.clone()).await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(
        !confirm(first, event, confirmation.clone())
            .await
            .unwrap()
            .meaning_is_provisional
    );
    assert!(
        confirm(first, event, confirmation.clone())
            .await
            .unwrap()
            .replayed
    );
    let revised = add(
        second,
        InterpretationInput {
            expected_current_revision_id: Some(first),
            interpretation_text: "The speaker claims not to know the identity of the approver."
                .into(),
            rationale: "Narrow the scope of the statement".into(),
        },
    )
    .await
    .unwrap();
    assert!(revised.meaning_is_provisional);
    let historical = confirm(first, Uuid::new_v4(), confirmation).await.unwrap();
    assert_eq!(historical.revision_id, second);
    assert!(historical.meaning_is_provisional && !historical.confirmation_establishes_truth);
    assert!(matches!(
        add(
            Uuid::new_v4(),
            InterpretationInput {
                expected_current_revision_id: Some(first),
                ..draft.clone()
            }
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    let final_snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(final_snapshot.response_interpretations.len(), 1);
    let view = &final_snapshot.response_interpretations[0];
    assert_eq!(view.current_revision_id, second);
    assert_eq!(view.revisions.len(), 2);
    assert_eq!(
        view.revisions[0].meaning_confirmation,
        MeaningConfirmation::Confirmed
    );
    assert_eq!(
        view.revisions[1].meaning_confirmation,
        MeaningConfirmation::Unconfirmed
    );
    assert_eq!(view.confirmations.len(), 2);
    assert!(!view.confirmation_establishes_truth);
    assert_eq!(
        final_snapshot.question_responses[0]
            .utterance
            .utterance_text,
        "I don't know."
    );
    assert!(final_snapshot.assessments.is_empty() && final_snapshot.arguments.is_empty());
    assert!(
        epistemic_workbench::snapshot(&f.service, f.workspace, other)
            .await
            .unwrap()
            .response_interpretations
            .is_empty()
    );
}

#[tokio::test]
async fn response_occurrences_preserve_original_text_and_do_not_resolve_gaps() {
    use epistemic_workbench::inquiry_response::*;
    let f = fixture().await;
    let sit = situation(&f, "Response is not truth").await;
    let other = situation(&f, "Other inquiry").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Who approved payment?").await;
    let g = node(&f, sit, ExpertNodeKind::InformationGap, "Approval identity").await;
    let foreign = node(&f, other, ExpertNodeKind::Question, "Who signed elsewhere?").await;
    let q2 = node(
        &f,
        sit,
        ExpertNodeKind::Question,
        "What does the witness claim to know?",
    )
    .await;
    let response = Uuid::new_v4();
    let input = ResponseInput {
        utterance_id: Uuid::new_v4(),
        utterance_text: "I do not know.".into(),
        speaker_actor_id: None,
        source_evidence_id: None,
        source_contribution_id: None,
        rationale: "Answer received during interview".into(),
    };
    let record = |question, id, input| {
        epistemic_workbench::inquiry_response::record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            question,
            id,
            input,
        )
    };
    assert!(matches!(
        record(g, response, input.clone()).await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        record(foreign, response, input.clone()).await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(matches!(
        record(
            q,
            response,
            ResponseInput {
                source_evidence_id: Some(Uuid::new_v4()),
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    let authored = record(q, response, input.clone()).await.unwrap();
    assert!(!authored.replayed && !authored.utterance.replayed);
    assert_eq!(authored.utterance.utterance_text, "I do not know.");
    assert!(record(q, response, input.clone()).await.unwrap().replayed);
    assert!(matches!(
        record(
            q,
            response,
            ResponseInput {
                utterance_text: "I know.".into(),
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    assert!(matches!(
        record(q, Uuid::new_v4(), input.clone()).await,
        Err(WorkbenchError::Conflict)
    ));
    let second = record(q2, Uuid::new_v4(), input.clone()).await.unwrap();
    assert!(!second.replayed && second.utterance.replayed);
    let independent = record(
        q,
        Uuid::new_v4(),
        ResponseInput {
            utterance_id: Uuid::new_v4(),
            ..input.clone()
        },
    )
    .await
    .unwrap();
    assert_ne!(
        independent.utterance.utterance_id,
        authored.utterance.utterance_id
    );
    let rejected_utterance = Uuid::new_v4();
    assert!(matches!(
        record(
            q,
            response,
            ResponseInput {
                utterance_id: rejected_utterance,
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.question_responses.len(), 3);
    assert_eq!(snapshot.nodes.len(), 3);
    assert!(snapshot.assessments.is_empty() && snapshot.arguments.is_empty());
    assert!(
        snapshot
            .question_inquiries
            .iter()
            .all(|q| q.current_discriminatory_power_events.is_empty())
    );
    assert!(
        epistemic_workbench::snapshot(&f.service, f.workspace, other)
            .await
            .unwrap()
            .question_responses
            .is_empty()
    );
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(f.workspace.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let exists:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_human_utterances WHERE workspace_id=$1 AND utterance_id=$2)").bind(f.workspace).bind(rejected_utterance).fetch_one(&mut *tx).await.unwrap();
    assert!(
        !exists,
        "conflicting response must roll back the newly inserted utterance"
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn gap_resolution_is_attributed_revision_not_an_answer_or_question_link() {
    use epistemic_workbench::assessment::*;
    let f = fixture().await;
    let sit = situation(&f, "Resolve only explicitly supported aspects").await;
    let other = situation(&f, "Other case").await;
    let gap = node(
        &f,
        sit,
        ExpertNodeKind::InformationGap,
        "Whether approval preceded payment",
    )
    .await;
    let question = node(
        &f,
        sit,
        ExpertNodeKind::Question,
        "When was the payment approved?",
    )
    .await;
    let proposition = node(
        &f,
        sit,
        ExpertNodeKind::Proposition,
        "The approval log records 09:00",
    )
    .await;
    let foreign = node(&f, other, ExpertNodeKind::InformationGap, "Other gap").await;
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        Uuid::new_v4(),
        RelationFamily::Contextual,
        RelationKind::AddressesGap,
        question,
        gap,
        "Investigate timing",
    )
    .await
    .unwrap();
    assert!(
        epistemic_workbench::snapshot(&f.service, f.workspace, sit)
            .await
            .unwrap()
            .assessments
            .is_empty()
    );
    let input = AssessmentInput {
        target: AssessmentTarget::Object { object_id: gap },
        dimension: AssessmentDimension::GapResolution,
        result: AssessmentResult::Open,
        confidence: AssessmentConfidence::Moderate,
        criterion: "Timing of approval and payment".into(),
        context: "Current investigation".into(),
        rationale: "The sequence is not established".into(),
        grounds: vec![],
        revises_event_id: None,
    };
    let record = |id, value| {
        epistemic_workbench::assessment::record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            value,
        )
    };
    assert!(matches!(
        record(
            Uuid::new_v4(),
            AssessmentInput {
                target: AssessmentTarget::Object {
                    object_id: question
                },
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        record(
            Uuid::new_v4(),
            AssessmentInput {
                target: AssessmentTarget::Object { object_id: foreign },
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    for grounds in [
        vec![],
        vec![AssessmentGround::Object {
            object_id: question,
        }],
        vec![AssessmentGround::Object { object_id: gap }],
    ] {
        assert!(matches!(
            record(
                Uuid::new_v4(),
                AssessmentInput {
                    result: AssessmentResult::Supported,
                    grounds,
                    ..input.clone()
                }
            )
            .await,
            Err(WorkbenchError::Invalid)
        ));
    }
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let third = Uuid::new_v4();
    record(first, input.clone()).await.unwrap();
    let partial = AssessmentInput {
        result: AssessmentResult::PartiallyAddressed,
        rationale: "Approval time supported, payment time missing".into(),
        grounds: vec![AssessmentGround::Object {
            object_id: proposition,
        }],
        revises_event_id: Some(first),
        ..input.clone()
    };
    assert!(!record(second, partial.clone()).await.unwrap().replayed);
    assert!(record(second, partial).await.unwrap().replayed);
    record(
        third,
        AssessmentInput {
            result: AssessmentResult::Contested,
            rationale: "Another judgment questions the timestamp".into(),
            ..input.clone()
        },
    )
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.assessments.len(), 3);
    assert_eq!(snapshot.assessments[1].input.revises_event_id, Some(first));
    assert_eq!(
        snapshot.assessments[2].input.result,
        AssessmentResult::Contested
    );
    assert!(
        snapshot.question_inquiries[0]
            .current_discriminatory_power_events
            .is_empty()
    );
    assert!(snapshot.arguments.is_empty());
    for (target, result, deferred) in [
        (question, "OPEN", false),
        (foreign, "OPEN", false),
        (gap, "SUPPORTED", true),
    ] {
        let mut tx = f.service.begin().await.unwrap();
        sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)")
            .bind(f.workspace.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        let insert = sqlx::query("INSERT INTO niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id,target_object_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,'GAP_RESOLUTION',$5,'LOW','criterion','context','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(target).bind(result).bind(f.principal).execute(&mut *tx).await;
        let err = if deferred {
            insert.unwrap();
            tx.commit().await.unwrap_err()
        } else {
            let err = insert.unwrap_err();
            tx.rollback().await.unwrap();
            err
        };
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
}

#[tokio::test]
async fn investigative_question_is_distinct_from_gap_and_never_a_conclusion_or_premise() {
    let f = fixture().await;
    let sit = situation(&f, "Investigate an unverified allegation").await;
    let other = situation(&f, "Separate investigation").await;
    let question = node(
        &f,
        sit,
        ExpertNodeKind::Question,
        "Why did the manager lie?",
    )
    .await;
    let gap = node(
        &f,
        sit,
        ExpertNodeKind::InformationGap,
        "Whether the manager knew the statement was false",
    )
    .await;
    let hypothesis = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The manager knowingly misrepresented the record",
    )
    .await;
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(
        snapshot
            .nodes
            .iter()
            .find(|n| n.object_id == question)
            .unwrap()
            .kind,
        ExpertNodeKind::Question
    );
    assert!(snapshot.relations.is_empty());
    assert!(snapshot.arguments.is_empty());
    let link = |s, family, kind, left, right| {
        epistemic_workbench::create_relation(
            &f.service,
            f.workspace,
            s,
            f.principal,
            Uuid::new_v4(),
            family,
            kind,
            left,
            right,
            "Investigative relevance only",
        )
    };
    assert!(
        link(
            sit,
            RelationFamily::Contextual,
            RelationKind::RelevantTo,
            question,
            gap
        )
        .await
        .is_ok()
    );
    assert!(matches!(
        link(
            sit,
            RelationFamily::Semantic,
            RelationKind::Entails,
            question,
            hypothesis
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        link(
            other,
            RelationFamily::Contextual,
            RelationKind::RelevantTo,
            question,
            gap
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(matches!(
        epistemic_workbench::create_argument(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            Uuid::new_v4(),
            question,
            ArgumentMode::Abductive,
            "Not a proposition"
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    let argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument,
        hypothesis,
        ArgumentMode::Abductive,
        "Hypothesis remains open",
    )
    .await
    .unwrap();
    let member = |role| NewGroundingMember {
        member_id: Uuid::new_v4(),
        role,
        ground_object_id: Some(question),
        evidence_id: None,
        rationale: "Question frames this inquiry".into(),
    };
    let create = async |members: Vec<NewGroundingMember>| {
        epistemic_workbench::create_grounding_set(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            argument,
            Uuid::new_v4(),
            GroundingEffect::Supports,
            GroundingAdequacy::Partial,
            "A question is not evidence",
            &members,
        )
        .await
    };
    assert!(matches!(
        create(vec![member(GroundingMemberRole::Premise)]).await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(
        create(vec![member(GroundingMemberRole::Context)])
            .await
            .is_ok()
    );
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert!(!snapshot.reasoning_diagnostics[0].has_supporting_routes);
    assert_eq!(
        snapshot.reasoning_diagnostics[0].routes[0].non_context_member_count,
        0
    );
    assert!(
        epistemic_workbench::snapshot(&f.service, f.workspace, other)
            .await
            .unwrap()
            .nodes
            .is_empty()
    );
}

#[tokio::test]
async fn question_authoring_respects_authenticated_http_and_workspace_boundaries() {
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;
    let f = fixture().await;
    let sit = situation(&f, "Question authoring").await;
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let writer = authz::provision_token(&admin, f.principal, f.workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1)
        .await
        .unwrap();
    let foreign = fixture().await;
    let outsider =
        authz::provision_token(&admin, foreign.principal, foreign.workspace, true, true, 1)
            .await
            .unwrap();
    let id = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/objects/{id}",
        f.workspace, sit
    );
    let router = niccolo_evidence::api::router(secure);
    let send = |token: &str, question: &str| {
        Request::builder().method("POST").uri(&path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE,"application/json")
        .body(Body::from(serde_json::json!({"kind":"QUESTION","text":question,"rationale":"Investigate without presupposing truth"}).to_string())).unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(send(&reader, "What happened?"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(send(&outsider, "What happened?"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(send(&writer, "What happened?"))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        router
            .clone()
            .oneshot(send(&writer, "What happened?"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .clone()
            .oneshot(send(&writer, "Different question?"))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let response_path = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/questions/{id}/responses/{}",
        f.workspace,
        sit,
        Uuid::new_v4()
    );
    let payload = serde_json::json!({"utterance_id":Uuid::new_v4(),"utterance_text":"I do not know.","rationale":"A recorded reply"});
    let send_response = |token: &str| {
        Request::builder()
            .method("POST")
            .uri(&response_path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap()
    };
    for (token, status) in [
        (&reader, StatusCode::FORBIDDEN),
        (&outsider, StatusCode::FORBIDDEN),
        (&writer, StatusCode::CREATED),
        (&writer, StatusCode::OK),
    ] {
        assert_eq!(
            router
                .clone()
                .oneshot(send_response(token))
                .await
                .unwrap()
                .status(),
            status
        );
    }
    let interpretation_path = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/utterances/{}/interpretations/{}/revisions/{}",
        f.workspace,
        sit,
        payload["utterance_id"].as_str().unwrap(),
        Uuid::new_v4(),
        Uuid::new_v4()
    );
    let confirmation_path = format!("{interpretation_path}/confirmations/{}", Uuid::new_v4());
    for (uri, payload) in [
        (
            interpretation_path,
            serde_json::json!({"interpretation_text":"The speaker claims ignorance","rationale":"Interpret the utterance only"}),
        ),
        (
            confirmation_path,
            serde_json::json!({"state":"Confirmed","note":"Meaning only"}),
        ),
    ] {
        for (token, status) in [
            (&reader, StatusCode::FORBIDDEN),
            (&outsider, StatusCode::FORBIDDEN),
            (&writer, StatusCode::CREATED),
            (&writer, StatusCode::OK),
        ] {
            let request = Request::builder()
                .method("POST")
                .uri(&uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            assert_eq!(
                router.clone().oneshot(request).await.unwrap().status(),
                status
            );
        }
    }
}

#[tokio::test]
async fn assessments_can_target_exact_prior_assessments_without_mutating_them() {
    use epistemic_workbench::assessment::*;
    let f = fixture().await;
    let sit = situation(&f, "Review an assessment's reliability").await;
    let other = situation(&f, "Separate investigation").await;
    let object = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "An initial explanatory proposition",
    )
    .await;
    let original = Uuid::new_v4();
    let base = AssessmentInput {
        target: AssessmentTarget::Object { object_id: object },
        dimension: AssessmentDimension::Credibility,
        result: AssessmentResult::Credible,
        confidence: AssessmentConfidence::Moderate,
        criterion: "Consistency".into(),
        context: "Initial examination".into(),
        rationale: "Consistent with reported sequence".into(),
        grounds: vec![],
        revises_event_id: None,
    };
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        original,
        base.clone(),
    )
    .await
    .unwrap();
    let meta_id = Uuid::new_v4();
    let meta = AssessmentInput {
        target: AssessmentTarget::Assessment {
            assessment_event_id: original,
        },
        dimension: AssessmentDimension::Reliability,
        result: AssessmentResult::LimitedReliability,
        confidence: AssessmentConfidence::High,
        criterion: "Method used".into(),
        context: "Assess investigator judgment".into(),
        rationale: "Independent confirmation is absent".into(),
        grounds: vec![],
        revises_event_id: None,
    };
    assert!(
        !record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            meta_id,
            meta.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(
        record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            meta_id,
            meta.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(matches!(
        record(
            &f.service,
            f.workspace,
            other,
            f.principal,
            Uuid::new_v4(),
            meta.clone()
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    let self_id = Uuid::new_v4();
    assert!(matches!(
        record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            self_id,
            AssessmentInput {
                target: AssessmentTarget::Assessment {
                    assessment_event_id: self_id
                },
                ..meta.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    let revision = Uuid::new_v4();
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        revision,
        AssessmentInput {
            result: AssessmentResult::Questionable,
            revises_event_id: Some(original),
            ..base.clone()
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            Uuid::new_v4(),
            AssessmentInput {
                target: AssessmentTarget::Assessment {
                    assessment_event_id: revision
                },
                revises_event_id: Some(meta_id),
                ..meta.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.assessments.len(), 3);
    assert_eq!(snapshot.assessments[0].input, base);
    assert_eq!(snapshot.assessments[1].input, meta);
    assert_eq!(
        snapshot.assessments[2].input.revises_event_id,
        Some(original)
    );
    assert!(snapshot.assessment_dependencies.is_empty());
}

#[tokio::test]
async fn multidimensional_assessments_preserve_revision_and_disagreement() {
    use epistemic_workbench::assessment::*;
    let f = fixture().await;
    let sit = situation(&f, "Assess competing explanations").await;
    let other = situation(&f, "Separate investigation").await;
    let target = node(&f, sit, ExpertNodeKind::Hypothesis, "Explanation A").await;
    let ground = node(&f, sit, ExpertNodeKind::Proposition, "Observed sequence").await;
    let foreign = node(
        &f,
        other,
        ExpertNodeKind::Proposition,
        "Unrelated observation",
    )
    .await;
    let input = AssessmentInput {
        target: AssessmentTarget::Object { object_id: target },
        dimension: AssessmentDimension::Credibility,
        result: AssessmentResult::Questionable,
        confidence: AssessmentConfidence::Moderate,
        criterion: "Consistency with observed events".into(),
        context: "Current investigation".into(),
        rationale: "The order of events is not conclusive".into(),
        grounds: vec![AssessmentGround::Object { object_id: ground }],
        revises_event_id: None,
    };
    let id = Uuid::new_v4();
    let record = |id, input| {
        epistemic_workbench::assessment::record(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            input,
        )
    };
    assert!(!record(id, input.clone()).await.unwrap().replayed);
    assert!(record(id, input.clone()).await.unwrap().replayed);
    let mut invalid = input.clone();
    invalid.result = AssessmentResult::Reliable;
    assert!(matches!(
        record(Uuid::new_v4(), invalid).await,
        Err(WorkbenchError::Invalid)
    ));
    let mut invisible = input.clone();
    invisible.grounds = vec![AssessmentGround::Object { object_id: foreign }];
    assert!(matches!(
        record(Uuid::new_v4(), invisible).await,
        Err(WorkbenchError::Invisible)
    ));
    let mut revision = input.clone();
    revision.revises_event_id = Some(id);
    revision.result = AssessmentResult::Credible;
    let revision_id = Uuid::new_v4();
    assert!(matches!(
        record(
            id,
            AssessmentInput {
                rationale: "Changed".into(),
                ..input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    assert!(
        !record(revision_id, revision.clone())
            .await
            .unwrap()
            .replayed
    );
    assert!(matches!(
        record(Uuid::new_v4(), revision).await,
        Err(WorkbenchError::Conflict)
    ));
    let divergent_id = Uuid::new_v4();
    record(
        divergent_id,
        AssessmentInput {
            result: AssessmentResult::NotCredible,
            ..input.clone()
        },
    )
    .await
    .unwrap();
    record(
        Uuid::new_v4(),
        AssessmentInput {
            dimension: AssessmentDimension::Reliability,
            result: AssessmentResult::Unknown,
            ..input.clone()
        },
    )
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.assessments.len(), 4);
    assert_eq!(snapshot.assessments[0].input, input);
    assert_eq!(snapshot.assessments[1].input.revises_event_id, Some(id));
    assert_eq!(snapshot.assessments[2].event_id, divergent_id);
    assert!(
        epistemic_workbench::snapshot(&f.service, f.workspace, other)
            .await
            .unwrap()
            .assessments
            .is_empty()
    );
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let writer = authz::provision_token(&admin, f.principal, f.workspace, true, true, 1)
        .await
        .unwrap();
    let other_actor = Uuid::new_v4();
    let other_writer = authz::provision_token(&admin, other_actor, f.workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1)
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let router = niccolo_evidence::api::router(secure);
    let http_id = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/assessments/{}",
        f.workspace, sit, http_id
    );
    let request = |token: &str, body: &AssessmentInput| {
        Request::builder()
            .method("POST")
            .uri(&path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap()
    };
    let http_input = AssessmentInput {
        dimension: AssessmentDimension::Independence,
        result: AssessmentResult::DependenceIndicated,
        ..input.clone()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(request(&reader, &http_input))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(&writer, &http_input))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        router
            .oneshot(request(&writer, &http_input))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert!(matches!(
        epistemic_workbench::assessment::record(
            &f.service,
            f.workspace,
            sit,
            other_actor,
            Uuid::new_v4(),
            AssessmentInput {
                revises_event_id: Some(http_id),
                ..http_input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    let divergent = epistemic_workbench::assessment::record(
        &f.service,
        f.workspace,
        sit,
        other_actor,
        Uuid::new_v4(),
        AssessmentInput {
            result: AssessmentResult::IndependenceSupported,
            ..http_input
        },
    )
    .await
    .unwrap();
    assert_eq!(divergent.recorded_by, other_actor);
    drop(other_writer);
}

#[tokio::test]
async fn pinned_assessment_revisions_require_selective_review_without_rewriting_routes() {
    use epistemic_workbench::assessment::*;
    let f = fixture().await;
    let sit = situation(&f, "Investigate reliance on an assessed premise").await;
    let other = situation(&f, "Other investigation").await;
    let premise = node(&f, sit, ExpertNodeKind::Proposition, "Observed premise").await;
    let alternate = node(&f, sit, ExpertNodeKind::Proposition, "Independent premise").await;
    let conclusion = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "Intermediate explanation",
    )
    .await;
    let final_node = node(&f, sit, ExpertNodeKind::Finding, "Consequence").await;
    let mut routes = vec![];
    for (ground, result) in [
        (premise, conclusion),
        (alternate, conclusion),
        (conclusion, final_node),
        (final_node, conclusion),
    ] {
        let argument_id = Uuid::new_v4();
        let grounding_set_id = Uuid::new_v4();
        epistemic_workbench::create_argument(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            argument_id,
            result,
            ArgumentMode::Abductive,
            "An explicitly authored route",
        )
        .await
        .unwrap();
        epistemic_workbench::create_grounding_set(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            argument_id,
            grounding_set_id,
            GroundingEffect::Supports,
            GroundingAdequacy::Partial,
            "Explicit ground",
            &[NewGroundingMember {
                member_id: Uuid::new_v4(),
                role: GroundingMemberRole::Premise,
                ground_object_id: Some(ground),
                evidence_id: None,
                rationale: "Ground for this route".into(),
            }],
        )
        .await
        .unwrap();
        routes.push((argument_id, grounding_set_id));
    }
    let original = Uuid::new_v4();
    let assessment = AssessmentInput {
        target: AssessmentTarget::Object { object_id: premise },
        dimension: AssessmentDimension::Reliability,
        result: AssessmentResult::Reliable,
        confidence: AssessmentConfidence::Moderate,
        criterion: "Record continuity".into(),
        context: "Current question".into(),
        rationale: "No known break in continuity".into(),
        grounds: vec![],
        revises_event_id: None,
    };
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        original,
        assessment.clone(),
    )
    .await
    .unwrap();
    let id = Uuid::new_v4();
    let dependency = DependencyInput {
        argument_id: routes[0].0,
        grounding_set_id: routes[0].1,
        assessment_event_id: original,
        rationale: "Continuity matters for this route".into(),
    };
    assert!(
        !record_dependency(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            dependency.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(
        record_dependency(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            dependency.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(matches!(
        record_dependency(
            &f.service,
            f.workspace,
            other,
            f.principal,
            Uuid::new_v4(),
            dependency.clone()
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(matches!(
        record_dependency(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            DependencyInput {
                rationale: "Different".into(),
                ..dependency.clone()
            }
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert!(!snapshot.assessment_dependencies[0].requires_review);
    // A divergent, independently authored judgment never revises the pinned one.
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        Uuid::new_v4(),
        AssessmentInput {
            result: AssessmentResult::Unreliable,
            ..assessment.clone()
        },
    )
    .await
    .unwrap();
    assert!(
        !epistemic_workbench::snapshot(&f.service, f.workspace, sit)
            .await
            .unwrap()
            .assessment_dependencies[0]
            .requires_review
    );
    let revision = Uuid::new_v4();
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        revision,
        AssessmentInput {
            result: AssessmentResult::LimitedReliability,
            revises_event_id: Some(original),
            ..assessment.clone()
        },
    )
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let view = &snapshot.assessment_dependencies[0];
    assert!(view.requires_review);
    assert_eq!(view.current_assessment_event_id, revision);
    assert_eq!(view.potential_downstream_routes.len(), 2);
    assert!(
        view.potential_downstream_routes
            .iter()
            .any(|r| r.argument_id == routes[2].0)
    );
    assert!(
        !view
            .potential_downstream_routes
            .iter()
            .any(|r| r.argument_id == routes[1].0)
    );
    assert_eq!(snapshot.grounding_sets.len(), 4);
    for (index, (argument_id, grounding_set_id)) in routes.iter().enumerate() {
        let diagnostic = snapshot
            .reasoning_diagnostics
            .iter()
            .flat_map(|d| &d.routes)
            .find(|r| r.argument_id == *argument_id && r.grounding_set_id == *grounding_set_id)
            .unwrap();
        assert_eq!(
            diagnostic
                .assessment_reassessment
                .directly_relying
                .unreviewed,
            usize::from(index == 0)
        );
        assert_eq!(
            diagnostic.assessment_reassessment.downstream.unreviewed,
            usize::from(index >= 2)
        );
        assert_eq!(diagnostic.reassessment.impacted, 0);
    }
    let review_id = Uuid::new_v4();
    let review = DependencyReviewInput {
        reviewed_assessment_event_id: revision,
        disposition: ReassessmentDisposition::Retained,
        note: "Other aspects of the same route still suffice for partial support".into(),
    };
    assert!(
        !record_dependency_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            review_id,
            review.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    let view = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap()
        .assessment_dependencies
        .remove(0);
    assert!(!view.requires_review);
    assert_eq!(view.review_history.len(), 1);
    assert_eq!(view.downstream_reviews.len(), 2);
    assert!(view.downstream_reviews.iter().all(|r| r.requires_review));
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let route = snapshot
        .reasoning_diagnostics
        .iter()
        .flat_map(|d| &d.routes)
        .find(|r| r.argument_id == routes[0].0)
        .unwrap();
    assert_eq!(route.assessment_reassessment.directly_relying.retained, 1);
    assert_eq!(route.assessment_reassessment.directly_relying.unreviewed, 0);
    let downstream_id = Uuid::new_v4();
    let downstream = DownstreamReviewInput {
        reviewed_assessment_event_id: revision,
        argument_id: routes[2].0,
        grounding_set_id: routes[2].1,
        disposition: ReassessmentDisposition::Retained,
        note: "This route remains supportable on its own terms".into(),
    };
    assert!(
        !record_downstream_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            downstream_id,
            downstream.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(
        record_downstream_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            downstream_id,
            downstream.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(matches!(
        record_downstream_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            Uuid::new_v4(),
            DownstreamReviewInput {
                argument_id: routes[1].0,
                grounding_set_id: routes[1].1,
                ..downstream.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    let view = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap()
        .assessment_dependencies
        .remove(0);
    assert_eq!(
        view.downstream_reviews
            .iter()
            .filter(|r| r.requires_review)
            .count(),
        1
    );
    let second = Uuid::new_v4();
    record(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        second,
        AssessmentInput {
            result: AssessmentResult::Unknown,
            revises_event_id: Some(revision),
            ..assessment
        },
    )
    .await
    .unwrap();
    let view = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap()
        .assessment_dependencies
        .remove(0);
    assert!(view.requires_review);
    assert_eq!(view.current_assessment_event_id, second);
    assert!(view.latest_review.is_none());
    assert_eq!(view.review_history.len(), 1);
    assert!(
        view.downstream_reviews
            .iter()
            .all(|r| r.requires_review && r.latest_review.is_none())
    );
    assert_eq!(
        view.downstream_reviews
            .iter()
            .map(|r| r.review_history.len())
            .sum::<usize>(),
        1
    );
    assert!(
        record_downstream_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            downstream_id,
            downstream.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(matches!(
        record_downstream_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            Uuid::new_v4(),
            downstream.clone()
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    assert!(matches!(
        record_downstream_review(
            &f.service,
            f.workspace,
            other,
            f.principal,
            id,
            Uuid::new_v4(),
            DownstreamReviewInput {
                reviewed_assessment_event_id: second,
                ..downstream
            }
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(
        record_dependency_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            review_id,
            review.clone()
        )
        .await
        .unwrap()
        .replayed
    );
    assert!(matches!(
        record_dependency_review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            Uuid::new_v4(),
            review
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    assert!(
        epistemic_workbench::snapshot(&f.service, f.workspace, other)
            .await
            .unwrap()
            .assessment_dependencies
            .is_empty()
    );
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let writer = authz::provision_token(&admin, f.principal, f.workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1)
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let router = niccolo_evidence::api::router(secure);
    let http_dependency = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/assessment-dependencies/{}",
        f.workspace, sit, http_dependency
    );
    let request = |token: &str, path: &str, body: String| {
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    let body = serde_json::to_string(&dependency).unwrap();
    assert_eq!(
        router
            .clone()
            .oneshot(request(&reader, &path, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(&writer, &path, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(&writer, &path, body))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let review_path = format!("{path}/reviews/{}", Uuid::new_v4());
    let body = serde_json::to_string(&DependencyReviewInput {
        reviewed_assessment_event_id: second,
        disposition: ReassessmentDisposition::Deferred,
        note: "Need further inspection".into(),
    })
    .unwrap();
    assert_eq!(
        router
            .clone()
            .oneshot(request(&reader, &review_path, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(&writer, &review_path, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(&writer, &review_path, body))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let downstream_path = format!("{path}/downstream-reviews/{}", Uuid::new_v4());
    let body = serde_json::to_string(&DownstreamReviewInput {
        reviewed_assessment_event_id: second,
        argument_id: routes[2].0,
        grounding_set_id: routes[2].1,
        disposition: ReassessmentDisposition::Retained,
        note: "Checked downstream route independently".into(),
    })
    .unwrap();
    assert_eq!(
        router
            .clone()
            .oneshot(request(&reader, &downstream_path, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request(&writer, &downstream_path, body.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        router
            .oneshot(request(&writer, &downstream_path, body))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let view = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap()
        .assessment_dependencies
        .into_iter()
        .find(|v| v.dependency.dependency_id == http_dependency)
        .unwrap();
    assert!(view.requires_review);
    assert_eq!(
        view.downstream_reviews
            .iter()
            .filter(|r| r.requires_review)
            .count(),
        1
    );
    assert_eq!(
        view.latest_review.unwrap().input.disposition,
        ReassessmentDisposition::Deferred
    );
}

#[tokio::test]
async fn expert_authored_relations_and_joint_grounding_remain_explicit() {
    let f = fixture().await;
    let sit = situation(
        &f,
        "Investigate whether the explanation survives contrary evidence",
    )
    .await;
    let hypothesis = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The payment delay was caused by a manual approval hold.",
    )
    .await;
    let alternative = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The payment delay was caused by a provider outage.",
    )
    .await;
    let assumption = node(
        &f,
        sit,
        ExpertNodeKind::Assumption,
        "The timestamps in the exported ledger are in UTC.",
    )
    .await;
    let finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The manual-hold explanation remains materially supported.",
    )
    .await;

    let relation_id = Uuid::new_v4();
    let first = epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        relation_id,
        RelationFamily::Semantic,
        RelationKind::Contradicts,
        hypothesis,
        alternative,
        "The two explanations make incompatible claims about the immediate cause.",
    )
    .await
    .unwrap();
    let replay = epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        relation_id,
        RelationFamily::Semantic,
        RelationKind::Contradicts,
        alternative,
        hypothesis,
        "The two explanations make incompatible claims about the immediate cause.",
    )
    .await
    .unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(first.left_object_id, replay.left_object_id);
    assert_eq!(first.right_object_id, replay.right_object_id);

    let argument_id = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        finding,
        ArgumentMode::Abductive,
        "The finding depends on the hypothesis plus an explicit timestamp assumption.",
    )
    .await
    .unwrap();
    let grounding_set_id = Uuid::new_v4();
    let grounding = epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        grounding_set_id,
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "These grounds operate jointly; neither is represented as independently sufficient.",
        &[
            NewGroundingMember {
                member_id: Uuid::new_v4(),
                role: GroundingMemberRole::Premise,
                ground_object_id: Some(hypothesis),
                evidence_id: None,
                rationale: "The live explanatory premise.".into(),
            },
            NewGroundingMember {
                member_id: Uuid::new_v4(),
                role: GroundingMemberRole::Assumption,
                ground_object_id: Some(assumption),
                evidence_id: None,
                rationale: "Required for the timeline comparison.".into(),
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(grounding.members.len(), 2);
    assert_eq!(grounding.adequacy, GroundingAdequacy::Partial);

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.nodes.len(), 4);
    assert_eq!(snapshot.relations.len(), 1);
    assert_eq!(snapshot.arguments.len(), 1);
    assert_eq!(snapshot.grounding_sets.len(), 1);
    assert_eq!(snapshot.grounding_sets[0].members.len(), 2);
    assert_eq!(snapshot.reasoning_diagnostics.len(), 1);
    assert_eq!(snapshot.reasoning_diagnostics[0].routes.len(), 1);
    assert!(snapshot.reasoning_diagnostics[0].has_supporting_routes);
    assert!(!snapshot.reasoning_diagnostics[0].has_opposing_routes);
    assert!(!snapshot.reasoning_diagnostics[0].has_mixed_directional_grounding);
    assert!(!snapshot.investigator_free_text_compiler_enabled);
}

#[tokio::test]
async fn relation_contract_rejects_wrong_family_and_cross_situation_endpoint() {
    let f = fixture().await;
    let first_situation = situation(&f, "First Situation").await;
    let second_situation = situation(&f, "Second Situation").await;
    let left = node(
        &f,
        first_situation,
        ExpertNodeKind::Hypothesis,
        "First hypothesis.",
    )
    .await;
    let right = node(
        &f,
        second_situation,
        ExpertNodeKind::Hypothesis,
        "Second hypothesis.",
    )
    .await;

    let wrong_family = epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        first_situation,
        f.principal,
        Uuid::new_v4(),
        RelationFamily::Causal,
        RelationKind::Contradicts,
        left,
        right,
        "Wrong family must not be silently repaired.",
    )
    .await;
    assert!(matches!(wrong_family, Err(WorkbenchError::Invalid)));

    let cross_situation = epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        first_situation,
        f.principal,
        Uuid::new_v4(),
        RelationFamily::Semantic,
        RelationKind::Contradicts,
        left,
        right,
        "Cross-Situation identity must be an explicit future decision, not an accidental edge.",
    )
    .await;
    assert!(matches!(cross_situation, Err(WorkbenchError::Invisible)));
}

#[tokio::test]
async fn grounding_member_target_and_role_are_not_guessed() {
    let f = fixture().await;
    let sit = situation(&f, "Grounding validation").await;
    let finding = node(&f, sit, ExpertNodeKind::Finding, "A finding.").await;
    let premise = node(&f, sit, ExpertNodeKind::Hypothesis, "A premise.").await;
    let argument_id = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        finding,
        ArgumentMode::Inductive,
        "Test argument.",
    )
    .await
    .unwrap();

    let invalid = epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Unspecified,
        "Invalid member is rejected before persistence.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Evidence,
            ground_object_id: Some(premise),
            evidence_id: None,
            rationale: "An epistemic object cannot silently become documentary evidence.".into(),
        }],
    )
    .await;
    assert!(matches!(invalid, Err(WorkbenchError::Invalid)));
}

#[tokio::test]
async fn situation_source_fragments_are_visible_and_can_ground_an_argument() {
    let f = fixture().await;
    let sit = situation(&f, "Ground a finding in documentary evidence").await;
    let document_id = Uuid::new_v4();
    let imported = ingest_source(
        &f.service,
        f.workspace,
        document_id,
        SourceFormat::WhatsappExport,
        "fixture://expert-grounding-chat",
        b"[02/11/2026, 12:00] Ana: approval was placed on hold\n",
    )
    .await
    .unwrap();
    situation_store::add_source(&f.service, f.workspace, f.principal, sit, document_id)
        .await
        .unwrap();

    let finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "A manual approval hold is documented in the supplied chat export.",
    )
    .await;
    let argument_id = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        finding,
        ArgumentMode::Inductive,
        "The finding is grounded in one explicitly selected source fragment.",
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.evidence.len(), 1);
    assert!(!snapshot.evidence_truncated);
    assert_eq!(snapshot.evidence[0].evidence_id, imported.fragment_ids[0]);
    assert_eq!(snapshot.evidence[0].document_id, document_id);
    assert_eq!(snapshot.evidence[0].speaker_label, "Ana");
    assert!(
        snapshot.evidence[0]
            .content
            .contains("approval was placed on hold")
    );

    let grounding = epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The source fragment is an explicit documentary member, not an inferred premise.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Evidence,
            ground_object_id: None,
            evidence_id: Some(imported.fragment_ids[0]),
            rationale: "Investigator selected this exact fragment as documentary grounding.".into(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(grounding.members.len(), 1);
    assert_eq!(grounding.members[0].role, GroundingMemberRole::Evidence);
    assert_eq!(
        grounding.members[0].evidence_id,
        Some(imported.fragment_ids[0])
    );
}

#[tokio::test]
async fn direct_documentary_routes_group_shared_ancestry_without_claiming_independence() {
    use epistemic_workbench::EvidenceLineageStatus;
    let f = fixture().await;
    let sit = situation(&f, "Compare recorded documentary dependence").await;
    let finding = node(&f, sit, ExpertNodeKind::Finding, "A transfer was delayed.").await;
    let mut ids = Vec::new();
    for (locator, content) in [
        (
            "fixture://lineage-a",
            "[02/11/2026, 12:00] Ana: transfer delayed\n[02/11/2026, 12:01] Ana: still delayed\n",
        ),
        (
            "fixture://lineage-b",
            "[02/11/2026, 12:02] Bob: transfer delayed\n",
        ),
        (
            "fixture://lineage-c",
            "[02/11/2026, 12:03] Carol: transfer delayed\n",
        ),
    ] {
        let doc = Uuid::new_v4();
        let imported = ingest_source(
            &f.service,
            f.workspace,
            doc,
            SourceFormat::WhatsappExport,
            locator,
            content.as_bytes(),
        )
        .await
        .unwrap();
        situation_store::add_source(&f.service, f.workspace, f.principal, sit, doc)
            .await
            .unwrap();
        ids.extend(imported.fragment_ids);
    }
    assert_eq!(ids.len(), 4);
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) VALUES($1,$2,$3,'COPIED_FROM')")
        .bind(f.workspace).bind(ids[2]).bind(ids[0]).execute(&admin).await.unwrap();
    let mut route_refs = Vec::new();
    for (i, evidence) in ids.iter().enumerate() {
        let argument = Uuid::new_v4();
        let set = Uuid::new_v4();
        epistemic_workbench::create_argument(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            argument,
            finding,
            ArgumentMode::Inductive,
            "Inspect this documentary route.",
        )
        .await
        .unwrap();
        epistemic_workbench::create_grounding_set(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            argument,
            set,
            if i == 3 {
                GroundingEffect::Challenges
            } else {
                GroundingEffect::Supports
            },
            GroundingAdequacy::Partial,
            "An explicitly selected fragment.",
            &[NewGroundingMember {
                member_id: Uuid::new_v4(),
                role: GroundingMemberRole::Evidence,
                ground_object_id: None,
                evidence_id: Some(*evidence),
                rationale: "Documentary ground.".into(),
            }],
        )
        .await
        .unwrap();
        route_refs.push(epistemic_workbench::GroundingRouteRef {
            argument_id: argument,
            grounding_set_id: set,
        });
    }
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let diagnostic = &snapshot.reasoning_diagnostics[0];
    assert!(diagnostic.has_mixed_directional_grounding);
    assert!(diagnostic.shared_grounds.is_empty());
    let lineage = &diagnostic.direct_evidence_lineage;
    assert_eq!(
        lineage.status,
        EvidenceLineageStatus::RecordedLineageResolved
    );
    assert_eq!(lineage.selected_evidence_count, 4);
    assert_eq!(lineage.components.len(), 2);
    assert!(!lineage.establishes_independence);
    let joined = lineage
        .components
        .iter()
        .find(|c| c.evidence_ids.contains(&ids[0]))
        .unwrap();
    assert_eq!(
        joined
            .evidence_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        ids[..3].iter().copied().collect()
    );
    assert_eq!(
        joined
            .routes
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        route_refs[..3].iter().copied().collect()
    );
    // The public read path uses the restrictive API role and credential gates,
    // not the more permissive legacy integration role.
    {
        use axum::{
            body::Body,
            http::{Request, StatusCode, header},
        };
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1)
            .await
            .unwrap();
        let secure = PgPoolOptions::new()
            .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
            .await
            .unwrap();
        let router = niccolo_evidence::api::router(secure);
        let path = format!(
            "/v1/workspaces/{}/situations/{}/epistemic/workbench",
            f.workspace, sit
        );
        let request = Request::builder()
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {reader}"))
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body["reasoning_diagnostics"][0]["direct_evidence_lineage"]["components"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let unrelated = fixture().await;
        let unrelated_situation = situation(&unrelated, "Not authorized by the reader").await;
        let request = Request::builder()
            .uri(format!(
                "/v1/workspaces/{}/situations/{}/epistemic/workbench",
                unrelated.workspace, unrelated_situation
            ))
            .header(header::AUTHORIZATION, format!("Bearer {reader}"))
            .body(Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert!(matches!(
            response.status(),
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ));
    }
    sqlx::query(
        "UPDATE niccolo.evidence SET admissible=false WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(f.workspace)
    .bind(ids[0])
    .execute(&admin)
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let lineage = &snapshot.reasoning_diagnostics[0].direct_evidence_lineage;
    assert_eq!(lineage.status, EvidenceLineageStatus::Withdrawn);
    assert!(lineage.components.is_empty());
    assert_eq!(lineage.selected_evidence_count, 4);
    assert!(!lineage.establishes_independence);
}

#[tokio::test]
async fn route_applicability_is_attributed_and_condition_revision_triggers_selective_review() {
    use epistemic_workbench::{ApplicabilityAssessment, ReassessmentImpactKind};
    let f = fixture().await;
    let sit = situation(&f, "Distinguish inference applicability from truth").await;
    let premise = node(
        &f,
        sit,
        ExpertNodeKind::Proposition,
        "An invoice was provided.",
    )
    .await;
    let condition = node(
        &f,
        sit,
        ExpertNodeKind::Assumption,
        "The invoice is genuine.",
    )
    .await;
    let conclusion = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The transaction occurred.",
    )
    .await;
    let arg = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        arg,
        conclusion,
        ArgumentMode::Inductive,
        "Invoice interpretation.",
    )
    .await
    .unwrap();
    let mut sets = Vec::new();
    for _ in 0..2 {
        let set = Uuid::new_v4();
        epistemic_workbench::create_grounding_set(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            arg,
            set,
            GroundingEffect::Supports,
            GroundingAdequacy::Partial,
            "Authored route.",
            &[NewGroundingMember {
                member_id: Uuid::new_v4(),
                role: GroundingMemberRole::Premise,
                ground_object_id: Some(premise),
                evidence_id: None,
                rationale: "Document premise.".into(),
            }],
        )
        .await
        .unwrap();
        sets.push(set);
    }
    let first_event = Uuid::new_v4();
    for replay in [false, true] {
        let event = epistemic_workbench::record_route_applicability(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            first_event,
            arg,
            sets[0],
            condition,
            ApplicabilityAssessment::Unassessed,
            "Genuineness must be assessed.",
        )
        .await
        .unwrap();
        assert_eq!(event.replayed, replay);
    }
    let conflict = epistemic_workbench::record_route_applicability(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        first_event,
        arg,
        sets[0],
        condition,
        ApplicabilityAssessment::Satisfied,
        "Changed replay.",
    )
    .await;
    assert!(matches!(conflict, Err(WorkbenchError::Conflict)));
    let cross_sit = situation(&f, "Unrelated condition").await;
    let outside = node(
        &f,
        cross_sit,
        ExpertNodeKind::Proposition,
        "Outside this Situation.",
    )
    .await;
    assert!(matches!(
        epistemic_workbench::record_route_applicability(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            Uuid::new_v4(),
            arg,
            sets[0],
            outside,
            ApplicabilityAssessment::Satisfied,
            "Not scoped."
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(matches!(
        epistemic_workbench::record_route_applicability(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            Uuid::new_v4(),
            arg,
            sets[0],
            conclusion,
            ApplicabilityAssessment::Satisfied,
            "Circular condition."
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    let second_event = Uuid::new_v4();
    epistemic_workbench::record_route_applicability(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        second_event,
        arg,
        sets[0],
        condition,
        ApplicabilityAssessment::NotSatisfied,
        "Authentication failed.",
    )
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let diag = &snapshot.reasoning_diagnostics[0];
    assert!(diag.has_supporting_routes); // authorship is still recorded; not truth or actual applicability
    let route = diag
        .routes
        .iter()
        .find(|r| r.grounding_set_id == sets[0])
        .unwrap();
    assert_eq!(route.applicability_conditions.len(), 1);
    assert_eq!(route.applicability_conditions[0].event_id, second_event);
    assert_eq!(
        route.applicability_conditions[0].assessment,
        ApplicabilityAssessment::NotSatisfied
    );
    assert!(
        diag.routes
            .iter()
            .find(|r| r.grounding_set_id == sets[1])
            .unwrap()
            .applicability_conditions
            .is_empty()
    );
    let revised = node(
        &f,
        sit,
        ExpertNodeKind::Assumption,
        "The invoice may be genuine under a different verification method.",
    )
    .await;
    let relation = Uuid::new_v4();
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        relation,
        RelationFamily::Genealogy,
        RelationKind::Supersedes,
        revised,
        condition,
        "The required condition has been revised.",
    )
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let impacts: Vec<_> = snapshot
        .reassessment_impacts
        .iter()
        .filter(|i| i.trigger_relation_id == relation)
        .collect();
    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0].grounding_set_id, Some(sets[0]));
    assert_eq!(
        impacts[0].impact_kind,
        ReassessmentImpactKind::ApplicabilityCondition
    );
    assert_eq!(impacts[0].latest_review, None);
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.epistemic_route_applicability_events WHERE workspace_id=$1 AND argument_id=$2").bind(f.workspace).bind(arg).fetch_one(&admin).await.unwrap();
    assert_eq!(count, 2);
    // Authenticated HTTP write, idempotence and read-only rejection on the new endpoint.
    {
        use axum::{
            body::Body,
            http::{Request, StatusCode, header},
        };
        use tower::ServiceExt;
        let writer = authz::provision_token(&admin, f.principal, f.workspace, true, true, 1)
            .await
            .unwrap();
        let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1)
            .await
            .unwrap();
        let secure = PgPoolOptions::new()
            .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
            .await
            .unwrap();
        let router = niccolo_evidence::api::router(secure);
        let event = Uuid::new_v4();
        let path = format!(
            "/v1/workspaces/{}/situations/{}/epistemic/arguments/{}/grounding/{}/applicability/{}",
            f.workspace, sit, arg, sets[1], event
        );
        let request = |token: &str| {
            Request::builder().method("POST").uri(&path).header(header::AUTHORIZATION, format!("Bearer {token}")).header(header::CONTENT_TYPE,"application/json").body(Body::from(serde_json::json!({"condition_object_id": revised,"assessment":"CONTESTED","rationale":"Competing verification outcomes."}).to_string())).unwrap()
        };
        assert_eq!(
            router
                .clone()
                .oneshot(request(&reader))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            router
                .clone()
                .oneshot(request(&writer))
                .await
                .unwrap()
                .status(),
            StatusCode::CREATED
        );
        assert_eq!(
            router.oneshot(request(&writer)).await.unwrap().status(),
            StatusCode::OK
        );
    }
    let other = Uuid::new_v4();
    authz::provision_token(&admin, other, f.workspace, true, true, 1)
        .await
        .unwrap();
    epistemic_workbench::record_route_applicability(
        &f.service,
        f.workspace,
        sit,
        other,
        Uuid::new_v4(),
        arg,
        sets[0],
        condition,
        ApplicabilityAssessment::Satisfied,
        "A second investigator disagrees.",
    )
    .await
    .unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let route = snapshot.reasoning_diagnostics[0]
        .routes
        .iter()
        .find(|r| r.grounding_set_id == sets[0])
        .unwrap();
    assert_eq!(route.applicability_conditions.len(), 2);
    assert!(
        route
            .applicability_conditions
            .iter()
            .any(|a| a.recorded_by == f.principal
                && a.assessment == ApplicabilityAssessment::NotSatisfied)
    );
    assert!(
        route
            .applicability_conditions
            .iter()
            .any(|a| a.recorded_by == other && a.assessment == ApplicabilityAssessment::Satisfied)
    );
}

#[tokio::test]
async fn persistent_proposition_requires_explicit_situation_membership() {
    let f = fixture().await;
    let sit_a = situation(&f, "Proposition source situation").await;
    let sit_b = situation(&f, "Unrelated situation").await;
    let proposition = node(
        &f,
        sit_a,
        ExpertNodeKind::Proposition,
        "The transfer occurred after approval.",
    )
    .await;

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit_a)
        .await
        .unwrap();
    let stored = snapshot
        .nodes
        .iter()
        .find(|item| item.object_id == proposition)
        .expect("expert-authored proposition is visible in its Situation");
    assert_eq!(stored.kind, ExpertNodeKind::Proposition);

    let local_hypothesis = node(
        &f,
        sit_b,
        ExpertNodeKind::Hypothesis,
        "Approval timing may explain the transfer.",
    )
    .await;
    let relation = epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit_b,
        f.principal,
        Uuid::new_v4(),
        RelationFamily::Semantic,
        RelationKind::Entails,
        proposition,
        local_hypothesis,
        "A workspace-level proposition cannot leak into another Situation merely because its ID exists.",
    )
    .await;
    assert!(matches!(relation, Err(WorkbenchError::Invisible)));
}

#[tokio::test]
async fn evidence_search_reaches_attached_material_beyond_snapshot_bound() {
    let f = fixture().await;
    let sit = situation(
        &f,
        "Search documentary evidence beyond the bounded Workbench list",
    )
    .await;
    let document_id = Uuid::new_v4();
    let mut body = String::new();
    for minute in 0..300 {
        let marker = if minute == 299 {
            " UNIQUESEARCHMARKER approval was suspended"
        } else {
            " routine status update"
        };
        body.push_str(&format!(
            "[02/11/2026, {:02}:{:02}] Ana: message {minute}{marker}\n",
            12 + (minute / 60),
            minute % 60,
        ));
    }
    let imported = ingest_source(
        &f.service,
        f.workspace,
        document_id,
        SourceFormat::WhatsappExport,
        "fixture://expert-search-chat",
        body.as_bytes(),
    )
    .await
    .unwrap();
    assert_eq!(imported.fragment_ids.len(), 300);
    situation_store::add_source(&f.service, f.workspace, f.principal, sit, document_id)
        .await
        .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.evidence.len(), 256);
    assert!(snapshot.evidence_truncated);
    assert!(
        snapshot
            .evidence
            .iter()
            .all(|item| !item.content.contains("UNIQUESEARCHMARKER"))
    );

    let results = epistemic_workbench::search_evidence(
        &f.service,
        f.workspace,
        sit,
        "UNIQUESEARCHMARKER",
        20,
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].evidence_id, imported.fragment_ids[299]);
    assert!(results[0].content.contains("approval was suspended"));
}

#[tokio::test]
async fn superseding_a_ground_surfaces_only_direct_reassessment_impacts() {
    let f = fixture().await;
    let sit = situation(&f, "Selective reassessment after a premise changes").await;
    let prior = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The payment was delayed because approval was missing.",
    )
    .await;
    let replacement = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The payment was delayed because approval arrived after the cutoff.",
    )
    .await;
    let finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "Approval timing materially affected payment timing.",
    )
    .await;
    let unaffected_finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The source export contains a timestamp.",
    )
    .await;

    let impacted_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        impacted_argument,
        finding,
        ArgumentMode::Abductive,
        "Compare approval-timing explanations.",
    )
    .await
    .unwrap();
    let impacted_set = Uuid::new_v4();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        impacted_argument,
        impacted_set,
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The prior hypothesis is an explicit premise.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(prior),
            evidence_id: None,
            rationale: "This premise carries the old approval-timing explanation.".into(),
        }],
    )
    .await
    .unwrap();

    let unaffected_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        unaffected_argument,
        unaffected_finding,
        ArgumentMode::Inductive,
        "Timestamp presence does not depend on the timing hypothesis.",
    )
    .await
    .unwrap();

    let trigger_relation = Uuid::new_v4();
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        trigger_relation,
        RelationFamily::Genealogy,
        RelationKind::Supersedes,
        replacement,
        prior,
        "The replacement hypothesis incorporates newly established cutoff timing.",
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.reassessment_impacts.len(), 1);
    let impact = &snapshot.reassessment_impacts[0];
    assert_eq!(impact.trigger_relation_id, trigger_relation);
    assert_eq!(impact.relation_kind, RelationKind::Supersedes);
    assert_eq!(impact.revision_object_id, replacement);
    assert_eq!(impact.prior_object_id, prior);
    assert_eq!(impact.argument_id, impacted_argument);
    assert_eq!(impact.grounding_set_id, Some(impacted_set));
    assert_eq!(impact.conclusion_object_id, finding);
    assert_eq!(
        impact.impact_kind,
        epistemic_workbench::ReassessmentImpactKind::GroundMember
    );
    assert_ne!(impact.argument_id, unaffected_argument);
}
#[tokio::test]
async fn refinement_alone_does_not_trigger_reassessment_but_reinterpretation_does() {
    let f = fixture().await;
    let sit = situation(&f, "Refinement versus reinterpretation reassessment").await;
    let prior = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The delay may be related to approval timing.",
    )
    .await;
    let revision = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The delay may be related specifically to approval arriving after the cutoff.",
    )
    .await;
    let finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "Approval timing remains relevant to the delay.",
    )
    .await;
    let argument_id = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        finding,
        ArgumentMode::Abductive,
        "The argument depends on the current approval-timing hypothesis.",
    )
    .await
    .unwrap();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The prior hypothesis is an explicit premise.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(prior),
            evidence_id: None,
            rationale: "This premise carries the original interpretation.".into(),
        }],
    )
    .await
    .unwrap();

    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        Uuid::new_v4(),
        RelationFamily::Genealogy,
        RelationKind::Refines,
        revision,
        prior,
        "The newer hypothesis is more specific but does not yet replace the prior one.",
    )
    .await
    .unwrap();
    let refined = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert!(refined.reassessment_impacts.is_empty());

    let trigger_relation = Uuid::new_v4();
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        trigger_relation,
        RelationFamily::Genealogy,
        RelationKind::Reinterprets,
        revision,
        prior,
        "The investigator now intends this as a revised reading of the prior hypothesis.",
    )
    .await
    .unwrap();
    let reinterpreted = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(reinterpreted.reassessment_impacts.len(), 1);
    assert_eq!(
        reinterpreted.reassessment_impacts[0].trigger_relation_id,
        trigger_relation
    );
    assert_eq!(
        reinterpreted.reassessment_impacts[0].relation_kind,
        RelationKind::Reinterprets
    );
}

#[tokio::test]
async fn retracting_an_argument_conclusion_surfaces_a_conclusion_impact() {
    let f = fixture().await;
    let sit = situation(&f, "Selective reassessment after a conclusion is retracted").await;
    let prior_finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The approval hold caused the transfer delay.",
    )
    .await;
    let retraction = node(
        &f,
        sit,
        ExpertNodeKind::Proposition,
        "The causal finding is withdrawn pending review of the cutoff evidence.",
    )
    .await;
    let argument_id = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_id,
        prior_finding,
        ArgumentMode::Causal,
        "This argument previously concluded that the hold caused the delay.",
    )
    .await
    .unwrap();

    let trigger_relation = Uuid::new_v4();
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        trigger_relation,
        RelationFamily::Genealogy,
        RelationKind::Retracts,
        retraction,
        prior_finding,
        "The investigator explicitly withdraws the prior conclusion from active use.",
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.reassessment_impacts.len(), 1);
    let impact = &snapshot.reassessment_impacts[0];
    assert_eq!(impact.trigger_relation_id, trigger_relation);
    assert_eq!(impact.relation_kind, RelationKind::Retracts);
    assert_eq!(impact.argument_id, argument_id);
    assert_eq!(impact.grounding_set_id, None);
    assert_eq!(impact.conclusion_object_id, prior_finding);
    assert_eq!(
        impact.impact_kind,
        epistemic_workbench::ReassessmentImpactKind::Conclusion
    );
}

#[tokio::test]
async fn snapshot_surfaces_recursive_support_without_inventing_independent_corroboration() {
    let f = fixture().await;
    let sit = situation(
        &f,
        "Circular grounding remains visible without confidence inflation",
    )
    .await;
    let a = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The approval hold explains the transfer delay.",
    )
    .await;
    let b = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The transfer delay implies that the approval hold remained effective.",
    )
    .await;

    let argument_a = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_a,
        a,
        ArgumentMode::Abductive,
        "Argument A currently uses hypothesis B as support.",
    )
    .await
    .unwrap();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_a,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "B is used as one route to A.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(b),
            evidence_id: None,
            rationale: "B is the explicit premise in this route.".into(),
        }],
    )
    .await
    .unwrap();

    let argument_b = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_b,
        b,
        ArgumentMode::Abductive,
        "Argument B currently uses hypothesis A as support.",
    )
    .await
    .unwrap();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument_b,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "A is used as one route to B.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(a),
            evidence_id: None,
            rationale: "A is the explicit premise in this route.".into(),
        }],
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.grounding_cycles.len(), 1);
    let cycle = &snapshot.grounding_cycles[0];
    assert_eq!(cycle.object_ids, vec![a.min(b), a.max(b)]);
    assert_eq!(cycle.argument_ids.len(), 2);
    assert!(cycle.argument_ids.contains(&argument_a));
    assert!(cycle.argument_ids.contains(&argument_b));
    assert!(!cycle.has_external_anchor);
    assert!(cycle.unanchored_recursive_support);
    assert!(cycle.external_grounding_routes.is_empty());
    let outside = node(
        &f,
        sit,
        ExpertNodeKind::Proposition,
        "The transaction date was recorded separately.",
    )
    .await;
    let contextual_set = Uuid::new_v4();
    for (id, role) in [
        (contextual_set, GroundingMemberRole::Context),
        (Uuid::new_v4(), GroundingMemberRole::Premise),
    ] {
        epistemic_workbench::create_grounding_set(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            argument_a,
            id,
            GroundingEffect::Supports,
            GroundingAdequacy::Partial,
            "An authored route outside the cycle.",
            &[NewGroundingMember {
                member_id: Uuid::new_v4(),
                role,
                ground_object_id: Some(outside),
                evidence_id: None,
                rationale: "Separate date record.".into(),
            }],
        )
        .await
        .unwrap();
        let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
            .await
            .unwrap();
        let cycle = &snapshot.grounding_cycles[0];
        let route = snapshot
            .reasoning_diagnostics
            .iter()
            .flat_map(|d| d.routes.iter())
            .find(|r| r.argument_id == argument_a && r.grounding_set_id == id)
            .unwrap();
        assert!(!route.recursive_support_cycle);
        if role == GroundingMemberRole::Context {
            assert!(cycle.unanchored_recursive_support);
            assert!(cycle.external_grounding_routes.is_empty());
            assert_eq!(route.non_context_member_count, 0);
        } else {
            assert!(cycle.has_external_anchor);
            assert_eq!(
                cycle.external_only_grounding_routes,
                vec![epistemic_workbench::GroundingRouteRef {
                    argument_id: argument_a,
                    grounding_set_id: id
                }]
            );
        }
    }
}

#[tokio::test]
async fn reassessment_walks_downstream_ground_dependencies_without_touching_unrelated_arguments() {
    let f = fixture().await;
    let sit = situation(
        &f,
        "Transitive reassessment follows only explicit grounding",
    )
    .await;
    let old = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The approval hold remained active through the cutoff.",
    )
    .await;
    let revised = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The approval hold ended before the cutoff.",
    )
    .await;
    let middle = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "Approval timing materially affected transfer timing.",
    )
    .await;
    let downstream = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The operating explanation should be reconsidered.",
    )
    .await;
    let unrelated = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The source export contains a timestamp.",
    )
    .await;

    let middle_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        middle_argument,
        middle,
        ArgumentMode::Abductive,
        "The middle finding depends on the current approval-timing hypothesis.",
    )
    .await
    .unwrap();
    let middle_set = Uuid::new_v4();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        middle_argument,
        middle_set,
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The old hypothesis is an explicit premise for the middle finding.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(old),
            evidence_id: None,
            rationale: "The timing hypothesis is the premise being revised.".into(),
        }],
    )
    .await
    .unwrap();

    let downstream_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        downstream_argument,
        downstream,
        ArgumentMode::Policy,
        "The downstream finding depends on the middle finding.",
    )
    .await
    .unwrap();
    let downstream_set = Uuid::new_v4();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        downstream_argument,
        downstream_set,
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The middle finding is an explicit premise for the downstream conclusion.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(middle),
            evidence_id: None,
            rationale: "The downstream conclusion depends on the middle finding.".into(),
        }],
    )
    .await
    .unwrap();

    let unrelated_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        unrelated_argument,
        unrelated,
        ArgumentMode::Inductive,
        "Timestamp presence is independent of the approval-timing hypothesis.",
    )
    .await
    .unwrap();

    let trigger_relation = Uuid::new_v4();
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        trigger_relation,
        RelationFamily::Genealogy,
        RelationKind::Supersedes,
        revised,
        old,
        "The revised hypothesis replaces the prior approval-timing hypothesis.",
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let impacts = snapshot
        .reassessment_impacts
        .iter()
        .filter(|impact| impact.trigger_relation_id == trigger_relation)
        .collect::<Vec<_>>();
    assert_eq!(impacts.len(), 2);
    assert!(impacts.iter().any(|impact| {
        impact.argument_id == middle_argument
            && impact.grounding_set_id == Some(middle_set)
            && impact.conclusion_object_id == middle
            && impact.depth == 1
    }));
    assert!(impacts.iter().any(|impact| {
        impact.argument_id == downstream_argument
            && impact.grounding_set_id == Some(downstream_set)
            && impact.conclusion_object_id == downstream
            && impact.depth == 2
    }));
    assert!(
        impacts
            .iter()
            .all(|impact| impact.argument_id != unrelated_argument)
    );
}

#[tokio::test]
async fn reassessment_review_is_append_only_and_latest_event_is_visible() {
    let f = fixture().await;
    let sit = situation(&f, "Investigator reviews a selective reassessment impact").await;
    let prior = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The approval hold remained active through the cutoff.",
    )
    .await;
    let revision = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The approval hold ended before the cutoff.",
    )
    .await;
    let finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "Approval timing affected transfer timing.",
    )
    .await;
    let argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument,
        finding,
        ArgumentMode::Abductive,
        "The finding depends on the approval-timing hypothesis.",
    )
    .await
    .unwrap();
    let grounding = Uuid::new_v4();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        argument,
        grounding,
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The prior hypothesis is the explicit premise under review.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(prior),
            evidence_id: None,
            rationale: "This is the premise changed by the later genealogy relation.".into(),
        }],
    )
    .await
    .unwrap();
    let trigger = Uuid::new_v4();
    epistemic_workbench::create_relation(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        trigger,
        RelationFamily::Genealogy,
        RelationKind::Supersedes,
        revision,
        prior,
        "The investigator explicitly supersedes the old timing hypothesis.",
    )
    .await
    .unwrap();

    let first_event = Uuid::new_v4();
    let first = epistemic_workbench::record_reassessment_review(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        first_event,
        trigger,
        argument,
        Some(grounding),
        ReassessmentDisposition::RevisionRequired,
        "The affected grounding route must be revised before relying on this argument.",
    )
    .await
    .unwrap();
    assert!(!first.replayed);

    let replay = epistemic_workbench::record_reassessment_review(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        first_event,
        trigger,
        argument,
        Some(grounding),
        ReassessmentDisposition::RevisionRequired,
        "The affected grounding route must be revised before relying on this argument.",
    )
    .await
    .unwrap();
    assert!(replay.replayed);

    let second_event = Uuid::new_v4();
    epistemic_workbench::record_reassessment_review(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        second_event,
        trigger,
        argument,
        Some(grounding),
        ReassessmentDisposition::Retained,
        "Independent grounding was checked; the argument remains usable despite this changed premise.",
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let impact = snapshot
        .reassessment_impacts
        .iter()
        .find(|impact| {
            impact.trigger_relation_id == trigger
                && impact.argument_id == argument
                && impact.grounding_set_id == Some(grounding)
        })
        .expect("the changed explicit premise remains a visible reassessment impact");
    let latest = impact
        .latest_review
        .as_ref()
        .expect("latest review is attached");
    assert_eq!(latest.review_event_id, second_event);
    assert_eq!(latest.disposition, ReassessmentDisposition::Retained);

    let invalid = epistemic_workbench::record_reassessment_review(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        Uuid::new_v4(),
        trigger,
        Uuid::new_v4(),
        Some(grounding),
        ReassessmentDisposition::Deferred,
        "An arbitrary non-impact cannot be reviewed into existence.",
    )
    .await;
    assert!(matches!(invalid, Err(WorkbenchError::Invisible)));
}
