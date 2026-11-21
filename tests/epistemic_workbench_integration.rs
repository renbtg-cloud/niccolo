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
