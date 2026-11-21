#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    authz,
    postgres_ingest::ingest_source,
    epistemic_workbench::{
        self, ArgumentMode, ExpertNodeKind, GroundingAdequacy, GroundingEffect,
        GroundingMemberRole, NewGroundingMember, RelationFamily, RelationKind, WorkbenchError,
    },
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
    situation_store::create(
        &f.service,
        f.workspace,
        f.principal,
        situation,
        focus,
    )
    .await
    .unwrap();
    situation
}

async fn node(
    f: &Fixture,
    situation: Uuid,
    kind: ExpertNodeKind,
    text: &str,
) -> Uuid {
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
async fn expert_authored_relations_and_joint_grounding_remain_explicit() {
    let f = fixture().await;
    let sit = situation(&f, "Investigate whether the explanation survives contrary evidence").await;
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
    situation_store::add_source(
        &f.service,
        f.workspace,
        f.principal,
        sit,
        document_id,
    )
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
    assert!(snapshot.evidence[0].content.contains("approval was placed on hold"));

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
    assert_eq!(grounding.members[0].evidence_id, Some(imported.fragment_ids[0]));
}
