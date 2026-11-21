#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    authz,
    epistemic_reassessment::{self, ReassessmentChangeKind},
    epistemic_workbench::{
        self, ArgumentMode, ExpertNodeKind, GroundingAdequacy, GroundingEffect,
        GroundingMemberRole, NewGroundingMember, RelationFamily, RelationKind, WorkbenchError,
    },
    situation_store,
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
async fn changed_ground_marks_only_dependency_chain_for_reassessment_without_falsifying_it() {
    let f = fixture().await;
    let sit = situation(&f, "Selective reassessment chain").await;
    let original = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The payment delay was caused by a manual approval hold.",
    )
    .await;
    let replacement = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "The payment delay may have combined a manual hold with provider latency.",
    )
    .await;
    let independent = node(
        &f,
        sit,
        ExpertNodeKind::Hypothesis,
        "Provider logs independently show a latency interval.",
    )
    .await;
    let first_finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The delay mechanism requires renewed review.",
    )
    .await;
    let downstream_finding = node(
        &f,
        sit,
        ExpertNodeKind::Finding,
        "The operational explanation remains usable for a downstream recommendation.",
    )
    .await;

    let first_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        first_argument,
        first_finding,
        ArgumentMode::Abductive,
        "First finding depends on alternative explicit grounding routes.",
    )
    .await
    .unwrap();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        first_argument,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "Route one uses the original hypothesis.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(original),
            evidence_id: None,
            rationale: "Original explanatory route.".into(),
        }],
    )
    .await
    .unwrap();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        first_argument,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "Route two remains independent of the changed hypothesis.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(independent),
            evidence_id: None,
            rationale: "Independent route must survive as a distinct route.".into(),
        }],
    )
    .await
    .unwrap();

    let second_argument = Uuid::new_v4();
    epistemic_workbench::create_argument(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        second_argument,
        downstream_finding,
        ArgumentMode::Policy,
        "Downstream conclusion explicitly depends on the first finding.",
    )
    .await
    .unwrap();
    epistemic_workbench::create_grounding_set(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        second_argument,
        Uuid::new_v4(),
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "The downstream route consumes the first finding.",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(first_finding),
            evidence_id: None,
            rationale: "Explicit downstream dependency.".into(),
        }],
    )
    .await
    .unwrap();

    let event_id = Uuid::new_v4();
    let recorded = epistemic_reassessment::record_object_change(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        event_id,
        original,
        Some(replacement),
        ReassessmentChangeKind::Revised,
        "The investigator authored a revised explanation and wants downstream dependencies reviewed.",
    )
    .await
    .unwrap();
    assert!(!recorded.replayed);
    assert!(!recorded.automatic_conclusion_status_change);
    assert_eq!(recorded.impacts.len(), 2);
    assert_eq!(recorded.impacts[0].argument_id, first_argument);
    assert_eq!(recorded.impacts[0].minimum_depth, 1);
    assert_eq!(recorded.impacts[0].impact_scope, "DIRECT_GROUND");
    assert_eq!(recorded.impacts[1].argument_id, second_argument);
    assert_eq!(recorded.impacts[1].minimum_depth, 2);
    assert_eq!(recorded.impacts[1].impact_scope, "DOWNSTREAM_CONCLUSION");

    let replay = epistemic_reassessment::record_object_change(
        &f.service,
        f.workspace,
        sit,
        f.principal,
        event_id,
        original,
        Some(replacement),
        ReassessmentChangeKind::Revised,
        "The investigator authored a revised explanation and wants downstream dependencies reviewed.",
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.impacts, recorded.impacts);

    let history = epistemic_reassessment::list(&f.service, f.workspace, sit, 100)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].event_id, event_id);
    assert!(!history[0].automatic_conclusion_status_change);
}

#[tokio::test]
async fn reassessment_rejects_cross_situation_replacement() {
    let f = fixture().await;
    let first = situation(&f, "First reassessment Situation").await;
    let second = situation(&f, "Second reassessment Situation").await;
    let trigger = node(&f, first, ExpertNodeKind::Hypothesis, "Original.").await;
    let foreign_replacement =
        node(&f, second, ExpertNodeKind::Hypothesis, "Foreign revision.").await;

    let result = epistemic_reassessment::record_object_change(
        &f.service,
        f.workspace,
        first,
        f.principal,
        Uuid::new_v4(),
        trigger,
        Some(foreign_replacement),
        ReassessmentChangeKind::Revised,
        "Cross-Situation replacement must not be guessed into the current model.",
    )
    .await;
    assert!(matches!(
        result,
        Err(epistemic_reassessment::ReassessmentError::Invisible)
    ));
}
