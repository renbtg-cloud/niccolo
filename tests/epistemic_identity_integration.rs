#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    analysis_claims::{self, NewClaim},
    analysis_output, analysis_snapshot,
    analysis_structure::AnalysisScope,
    authz,
    epistemic_identity::{
        self, EpistemicIdentityError, InterpretationProducer, MeaningConfirmation, NewUtterance,
    },
    situation_store,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

struct Fixture {
    admin: PgPool,
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
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'epistemic-identity-test')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let _token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    Fixture {
        admin,
        service,
        workspace,
        principal,
    }
}

async fn claim_in_new_analysis(
    f: &Fixture,
    situation: Uuid,
    ordinal: i32,
    text: &str,
) -> (AnalysisScope, Uuid) {
    let analysis = Uuid::new_v4();
    analysis_snapshot::begin(
        &f.service,
        f.workspace,
        f.principal,
        situation,
        analysis,
        "Test semantic identity",
        None,
    )
    .await
    .unwrap();
    analysis_output::publish(
        &f.service,
        f.workspace,
        f.principal,
        situation,
        analysis,
        "Structured answer",
        "Fixture coverage",
    )
    .await
    .unwrap();
    let claim_id = Uuid::new_v4();
    analysis_claims::record_claim(
        &f.service,
        f.workspace,
        situation,
        analysis,
        f.principal,
        NewClaim {
            claim_id,
            ordinal,
            kind: "INFERENCE".into(),
            text: text.into(),
            qualification: Some("test occurrence, not semantic identity".into()),
        },
    )
    .await
    .unwrap();
    (
        AnalysisScope {
            workspace: f.workspace,
            situation,
            analysis,
        },
        claim_id,
    )
}

#[tokio::test]
async fn one_persistent_proposition_can_link_distinct_analysis_claim_occurrences() {
    let f = fixture().await;
    let situation = Uuid::new_v4();
    situation_store::create(
        &f.service,
        f.workspace,
        f.principal,
        situation,
        "Did the warning reach the manager?",
    )
    .await
    .unwrap();
    let (scope_a, claim_a) =
        claim_in_new_analysis(&f, situation, 1, "The manager received the warning").await;
    let (scope_b, claim_b) =
        claim_in_new_analysis(&f, situation, 1, "The manager received the warning").await;
    assert_ne!(claim_a, claim_b);
    assert_ne!(scope_a.analysis, scope_b.analysis);

    let proposition_id = Uuid::new_v4();
    let proposition = epistemic_identity::create_proposition(
        &f.service,
        f.workspace,
        f.principal,
        proposition_id,
        "The manager received the warning.",
    )
    .await
    .unwrap();
    assert!(!proposition.replayed);
    epistemic_identity::link_claim_to_proposition(
        &f.service,
        scope_a,
        f.principal,
        claim_a,
        proposition_id,
        "Same semantic proposition in first analysis",
    )
    .await
    .unwrap();
    epistemic_identity::link_claim_to_proposition(
        &f.service,
        scope_b,
        f.principal,
        claim_b,
        proposition_id,
        "Same semantic proposition in re-expressed analysis occurrence",
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_claim_propositions WHERE workspace_id=$1 AND proposition_id=$2",
    )
    .bind(f.workspace)
    .bind(proposition_id)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn meaning_is_unconfirmed_until_user_confirms_and_confirmation_never_establishes_truth() {
    let f = fixture().await;
    let situation = Uuid::new_v4();
    situation_store::create(
        &f.service,
        f.workspace,
        f.principal,
        situation,
        "Understand a human statement",
    )
    .await
    .unwrap();
    let utterance_id = Uuid::new_v4();
    let utterance = epistemic_identity::create_utterance(
        &f.service,
        f.workspace,
        f.principal,
        NewUtterance {
            utterance_id,
            situation_id: Some(situation),
            utterance_text: "He was an asshole.",
            language: "en",
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(utterance.language, "en");

    let interpretation_id = Uuid::new_v4();
    let first_revision = Uuid::new_v4();
    let first = epistemic_identity::create_interpretation(
        &f.service,
        f.workspace,
        f.principal,
        interpretation_id,
        utterance_id,
        first_revision,
        "The speaker expresses a strongly negative evaluation of him.",
        "Conservative local meaning; no specific behavior inferred.",
        InterpretationProducer::Model,
    )
    .await
    .unwrap();
    assert_eq!(first.meaning_confirmation, MeaningConfirmation::Unconfirmed);
    assert!(first.meaning_is_provisional);
    assert!(!first.confirmation_establishes_truth);

    let confirmed_event = Uuid::new_v4();
    let confirmed = epistemic_identity::record_meaning_confirmation(
        &f.service,
        f.workspace,
        f.principal,
        interpretation_id,
        first_revision,
        confirmed_event,
        MeaningConfirmation::Confirmed,
        "Yes, that is what I meant.",
    )
    .await
    .unwrap();
    assert_eq!(
        confirmed.meaning_confirmation,
        MeaningConfirmation::Confirmed
    );
    assert!(!confirmed.meaning_is_provisional);
    assert!(!confirmed.confirmation_establishes_truth);

    let rejected = epistemic_identity::record_meaning_confirmation(
        &f.service,
        f.workspace,
        f.principal,
        interpretation_id,
        first_revision,
        Uuid::new_v4(),
        MeaningConfirmation::Rejected,
        "Correction: that is not what I meant.",
    )
    .await
    .unwrap();
    assert_eq!(rejected.meaning_confirmation, MeaningConfirmation::Rejected);
    assert!(rejected.meaning_is_provisional);

    // Replaying an older confirmation event must not make it the effective state again.
    let replay = epistemic_identity::record_meaning_confirmation(
        &f.service,
        f.workspace,
        f.principal,
        interpretation_id,
        first_revision,
        confirmed_event,
        MeaningConfirmation::Confirmed,
        "Yes, that is what I meant.",
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.meaning_confirmation, MeaningConfirmation::Rejected);
    assert!(replay.meaning_is_provisional);
    assert!(!replay.confirmation_establishes_truth);
}

#[tokio::test]
async fn revising_meaning_exposes_claims_that_depended_on_the_old_revision() {
    let f = fixture().await;
    let situation = Uuid::new_v4();
    situation_store::create(
        &f.service,
        f.workspace,
        f.principal,
        situation,
        "Clarify what the speaker meant",
    )
    .await
    .unwrap();
    let (scope, claim_id) = claim_in_new_analysis(
        &f,
        situation,
        1,
        "The speaker had a strongly negative view of the manager",
    )
    .await;
    let utterance_id = Uuid::new_v4();
    epistemic_identity::create_utterance(
        &f.service,
        f.workspace,
        f.principal,
        NewUtterance {
            utterance_id,
            situation_id: Some(situation),
            utterance_text: "He was an asshole.",
            language: "en",
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
        },
    )
    .await
    .unwrap();
    let interpretation_id = Uuid::new_v4();
    let rev1 = Uuid::new_v4();
    epistemic_identity::create_interpretation(
        &f.service,
        f.workspace,
        f.principal,
        interpretation_id,
        utterance_id,
        rev1,
        "The speaker expresses a strongly negative evaluation of him.",
        "Initial conservative interpretation.",
        InterpretationProducer::Model,
    )
    .await
    .unwrap();
    epistemic_identity::link_claim_interpretation_dependency(
        &f.service,
        scope,
        f.principal,
        claim_id,
        interpretation_id,
        rev1,
        "This claim uses the current understood meaning of the utterance.",
    )
    .await
    .unwrap();
    assert!(
        epistemic_identity::list_stale_claim_interpretation_dependencies(
            &f.service,
            f.workspace,
            interpretation_id,
        )
        .await
        .unwrap()
        .is_empty()
    );

    let rev2 = Uuid::new_v4();
    let revised = epistemic_identity::revise_interpretation(
        &f.service,
        f.workspace,
        f.principal,
        interpretation_id,
        rev1,
        rev2,
        "The speaker refers to repeated public humiliation of junior employees.",
        "User clarified what behavior the negative evaluation referred to.",
        InterpretationProducer::Human,
    )
    .await
    .unwrap();
    assert_eq!(revised.revision_id, rev2);
    assert_eq!(
        revised.meaning_confirmation,
        MeaningConfirmation::Unconfirmed
    );

    let stale = epistemic_identity::list_stale_claim_interpretation_dependencies(
        &f.service,
        f.workspace,
        interpretation_id,
    )
    .await
    .unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].claim_id, claim_id);
    assert_eq!(stale[0].interpretation_revision_id, rev1);
}

#[tokio::test]
async fn mvp_semantic_utterance_boundary_is_english_only_and_replays_are_strict() {
    let f = fixture().await;
    let utterance_id = Uuid::new_v4();
    let non_english = epistemic_identity::create_utterance(
        &f.service,
        f.workspace,
        f.principal,
        NewUtterance {
            utterance_id,
            situation_id: None,
            utterance_text: "Ele entregou o documento e vazou.",
            language: "pt-BR",
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
        },
    )
    .await;
    assert!(matches!(non_english, Err(EpistemicIdentityError::Invalid)));

    let first = epistemic_identity::create_utterance(
        &f.service,
        f.workspace,
        f.principal,
        NewUtterance {
            utterance_id,
            situation_id: None,
            utterance_text: "He handed over the document and just went away.",
            language: "en",
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
        },
    )
    .await
    .unwrap();
    assert!(!first.replayed);
    let replay = epistemic_identity::create_utterance(
        &f.service,
        f.workspace,
        f.principal,
        NewUtterance {
            utterance_id,
            situation_id: None,
            utterance_text: "He handed over the document and just went away.",
            language: "en",
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
        },
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    let conflict = epistemic_identity::create_utterance(
        &f.service,
        f.workspace,
        f.principal,
        NewUtterance {
            utterance_id,
            situation_id: None,
            utterance_text: "He leaked the document.",
            language: "en",
            speaker_actor_id: None,
            source_evidence_id: None,
            source_contribution_id: None,
        },
    )
    .await;
    assert!(matches!(conflict, Err(EpistemicIdentityError::Conflict)));
}
