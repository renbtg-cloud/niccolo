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
    let sit = situation(&f, "Search documentary evidence beyond the bounded Workbench list").await;
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
    situation_store::add_source(
        &f.service,
        f.workspace,
        f.principal,
        sit,
        document_id,
    )
    .await
    .unwrap();

    let snapshot = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert_eq!(snapshot.evidence.len(), 256);
    assert!(snapshot.evidence_truncated);
    assert!(snapshot
        .evidence
        .iter()
        .all(|item| !item.content.contains("UNIQUESEARCHMARKER")));

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
    assert_eq!(impact.impact_kind, epistemic_workbench::ReassessmentImpactKind::GroundMember);
    assert_ne!(impact.argument_id, unaffected_argument);
}
