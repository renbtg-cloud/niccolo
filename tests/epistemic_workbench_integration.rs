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

fn recommendation_input(ground: Uuid, recipient: Uuid) -> epistemic_workbench::action_recommendation::RecommendationInput {
    use epistemic_workbench::action_recommendation::{
        RecommendationAlternative, RecommendationInput, RecommendationKind, Reversibility,
    };
    RecommendationInput {
        kind: RecommendationKind::ReviewPrivileges,
        recipient_actor_id: recipient,
        purpose: "Inspect access without presuming misconduct".into(),
        context: "A suspected leak remains unresolved".into(),
        rationale: "Broad privileges are a separate structural concern".into(),
        ground_object_ids: vec![ground],
        qualified_basis: vec![],
        conditions: "Requires separate human approval".into(),
        alternatives: vec![RecommendationAlternative {
            kind: RecommendationKind::Inaction,
            rationale: "Wait for additional evidence".into(),
            expected_consequences: "Exposure may remain".into(),
        }],
        expected_consequences: "A clearer understanding of access".into(),
        uncertainty: "No proof of a leak".into(),
        risk: "Investigation may become visible".into(),
        reversibility: Reversibility::Reversible,
        revises_recommendation_id: None,
    }
}

#[tokio::test]
async fn recommendation_qualified_basis_preserves_alternative_structural_grounds() {
    use epistemic_workbench::action_recommendation::{self, BasisKind, QualifiedBasis, RecommendationInput};
    let f = fixture().await;
    let sit = situation(&f, "Protect access without prejudging a suspected leak").await;
    let suspicion = node(&f, sit, ExpertNodeKind::Hypothesis, "Possible unauthorized disclosure").await;
    let structural = node(&f, sit, ExpertNodeKind::Proposition, "Excessively broad privileges").await;
    let recipient = Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, recipient, "Reviewer").await.unwrap();
    let base = recommendation_input(suspicion, recipient);
    let record = |id, input| action_recommendation::record(&f.service, f.workspace, sit, f.principal, id, input);
    let original = Uuid::new_v4();
    assert!(record(original, base.clone()).await.unwrap().input.qualified_basis.is_empty());
    let groups = vec![QualifiedBasis {kind: BasisKind::ContextOnly, ground_object_ids: vec![suspicion], rationale: "Suspicion prompted investigation".into()}, QualifiedBasis {kind: BasisKind::AlternativeSufficient, ground_object_ids: vec![structural], rationale: "Structural risk justifies review regardless of misconduct".into()}];
    let input = RecommendationInput {ground_object_ids: vec![suspicion, structural], qualified_basis: groups.clone(), revises_recommendation_id: Some(original), ..base};
    let id = Uuid::new_v4();
    assert_eq!(record(id, input.clone()).await.unwrap().input.qualified_basis, groups);
    assert!(record(id, input.clone()).await.unwrap().replayed);
    assert!(matches!(record(Uuid::new_v4(), RecommendationInput { qualified_basis: vec![groups[0].clone()], revises_recommendation_id: None, ..input.clone() }).await, Err(WorkbenchError::Invalid)));
    let history = action_recommendation::list(&f.service, f.workspace, sit).await.unwrap();
    assert!(history[0].input.qualified_basis.is_empty());
    assert_eq!(history[1].input, input);
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let mut payload = serde_json::to_value(&input).unwrap(); payload.as_object_mut().unwrap().remove("qualified_basis"); payload["revises_recommendation_id"] = serde_json::Value::Null;
    let invalid = serde_json::json!([{"kind":"ALTERNATIVE_SUFFICIENT","ground_object_ids":[Uuid::new_v4()],"rationale":"Unknown ground"}]);
    let result = sqlx::query("INSERT INTO niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id,recipient_actor_id,ground_object_ids,payload,recorded_by_principal_id,qualified_basis) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(recipient).bind(&input.ground_object_ids).bind(payload).bind(f.principal).bind(invalid).execute(&mut *tx).await.unwrap_err();
    assert_eq!(result.as_database_error().unwrap().code().as_deref(), Some("23514")); tx.rollback().await.unwrap();
}

#[tokio::test]
async fn recommendation_basis_changes_surface_alternatives_without_deciding_appropriateness() {
    use epistemic_workbench::action_recommendation::{self, BasisKind, QualifiedBasis, RecommendationInput};
    let f = fixture().await;
    let sit = situation(&f, "Structural risk after suspicion is rejected").await;
    let suspicion = node(&f, sit, ExpertNodeKind::Hypothesis, "Possible unauthorized disclosure").await;
    let revised = node(&f, sit, ExpertNodeKind::Hypothesis, "Unauthorized disclosure is unsupported").await;
    let structural = node(&f, sit, ExpertNodeKind::Proposition, "Privileges are excessively broad").await;
    let downstream = node(&f, sit, ExpertNodeKind::Finding, "The suspicion may justify a review").await;
    let argument = Uuid::new_v4(); let set = Uuid::new_v4();
    epistemic_workbench::create_argument(&f.service, f.workspace, sit, f.principal, argument, downstream, ArgumentMode::Abductive, "Reason from the suspicion").await.unwrap();
    epistemic_workbench::create_grounding_set(&f.service, f.workspace, sit, f.principal, argument, set, GroundingEffect::Supports, GroundingAdequacy::Partial, "Dependent on suspicion", &[NewGroundingMember {member_id: Uuid::new_v4(), role: GroundingMemberRole::Premise, ground_object_id: Some(suspicion), evidence_id: None, rationale: "Premise".into()}]).await.unwrap();
    let recipient = Uuid::new_v4(); niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, recipient, "Reviewer").await.unwrap();
    let original = recommendation_input(suspicion, recipient);
    let qualified = RecommendationInput {ground_object_ids: vec![suspicion, structural, downstream], qualified_basis: vec![QualifiedBasis {kind: BasisKind::ContextOnly, ground_object_ids: vec![suspicion], rationale: "Motivating concern".into()}, QualifiedBasis {kind: BasisKind::AlternativeSufficient, ground_object_ids: vec![structural], rationale: "Independent structural rationale".into()}, QualifiedBasis {kind: BasisKind::AlternativeSufficient, ground_object_ids: vec![downstream], rationale: "Dependent rationale".into()}], ..original.clone()};
    let rec = Uuid::new_v4(); let unaffected_rec = Uuid::new_v4(); let legacy_rec = Uuid::new_v4(); let argument_rec = Uuid::new_v4();
    for (id, input) in [(rec, qualified.clone()), (unaffected_rec, recommendation_input(structural, recipient)), (legacy_rec, original), (argument_rec, recommendation_input(argument, recipient))] { action_recommendation::record(&f.service, f.workspace, sit, f.principal, id, input).await.unwrap(); }
    let before = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    assert!(before.recommendation_basis_impacts.is_empty());
    let relation = Uuid::new_v4(); epistemic_workbench::create_relation(&f.service, f.workspace, sit, f.principal, relation, RelationFamily::Genealogy, RelationKind::Supersedes, revised, suspicion, "Earlier hypothesis no longer supported").await.unwrap();
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(before.action_recommendations, after.action_recommendations); assert!(after.action_decisions.is_empty());
    let impacts = &after.recommendation_basis_impacts;
    assert_eq!(impacts.len(), 3);
    let impact = impacts.iter().find(|i| i.recommendation_id == rec).unwrap();
    assert_eq!(impact.trigger_relation_id, relation); assert_eq!(impact.affected_ground_object_ids, vec![suspicion, downstream]); assert_eq!(impact.affected_basis_indices, vec![0, 2]); assert_eq!(impact.alternative_basis_indices_not_linked_to_trigger, vec![1]);
    assert!(!impact.grounds_unqualified); assert!(!impact.historical_recommendation);
    assert!(impacts.iter().find(|i| i.recommendation_id == legacy_rec).unwrap().grounds_unqualified);
    assert_eq!(impacts.iter().find(|i| i.recommendation_id == argument_rec).unwrap().affected_ground_object_ids, vec![argument]);
    let successor = Uuid::new_v4(); action_recommendation::record(&f.service, f.workspace, sit, f.principal, successor, RecommendationInput {revises_recommendation_id: Some(rec), ..recommendation_input(structural, recipient)}).await.unwrap();
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    assert!(after.recommendation_basis_impacts.iter().find(|i| i.recommendation_id == rec).unwrap().historical_recommendation);
    assert!(!after.recommendation_basis_impacts.iter().any(|i| i.recommendation_id == successor || i.recommendation_id == unaffected_rec));
}

#[tokio::test]
async fn retrospective_action_assessments_keep_hindsight_and_contemporaneous_reasonableness_separate() {
    use epistemic_workbench::{action_recommendation, action_decision::{self, DecisionInput, DecisionDisposition}, assessment::{self, AssessmentInput, AssessmentTarget, AssessmentDimension, AssessmentResult, AssessmentConfidence, AssessmentGround}};
    let f = fixture().await;
    let sit = situation(&f, "January decision reviewed after a March discovery").await;
    let other = situation(&f, "Separate investigation").await;
    let known = node(&f, sit, ExpertNodeKind::Proposition, "No unauthorized connection known in January").await;
    let actual = node(&f, sit, ExpertNodeKind::Proposition, "March evidence describes a connection dating to December").await;
    let obtainable = node(&f, sit, ExpertNodeKind::Hypothesis, "Whether a January check could have revealed the connection is unresolved").await;
    let recipient = Uuid::new_v4(); niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, recipient, "Reviewer").await.unwrap();
    let recommendation = Uuid::new_v4(); action_recommendation::record(&f.service, f.workspace, sit, f.principal, recommendation, recommendation_input(known, recipient)).await.unwrap();
    let decision = Uuid::new_v4(); action_decision::record(&f.service, f.workspace, sit, f.principal, decision, DecisionInput {recommendation_id: recommendation, disposition: DecisionDisposition::SelectAlternative, alternative_index: Some(0), rationale: "Retain original access until further grounds are known".into(), revises_decision_id: None}).await.unwrap();
    let first = AssessmentInput {target: AssessmentTarget::Decision {decision_id: decision}, dimension: AssessmentDimension::DecisionContemporaneousReasonableness, result: AssessmentResult::Adequate, confidence: AssessmentConfidence::Moderate, criterion: "Reasonableness relative to information then available".into(), context: "January decision; assessed in April".into(), rationale: "No known link in January".into(), grounds: vec![AssessmentGround::Object {object_id: known}], revises_event_id: None};
    let second = AssessmentInput {dimension: AssessmentDimension::DecisionActualCircumstancesAdequacy, result: AssessmentResult::Inadequate, criterion: "Appropriateness relative to reconstructed circumstances".into(), rationale: "March discovery suggests actual exposure at decision time".into(), grounds: vec![AssessmentGround::Object {object_id: actual}], ..first.clone()};
    let third = AssessmentInput {dimension: AssessmentDimension::DecisionReasonablyObtainableInformation, result: AssessmentResult::Unknown, criterion: "Whether proportionate inquiry could have changed the basis".into(), rationale: "Available checks require independent examination".into(), grounds: vec![AssessmentGround::Object {object_id: obtainable}], ..first.clone()};
    let strategic = AssessmentInput {target: AssessmentTarget::Recommendation {recommendation_id: recommendation}, dimension: AssessmentDimension::RecommendationAppropriateness, result: AssessmentResult::Adequate, criterion: "Current protective purpose".into(), rationale: "Privilege review may be appropriate independently of the suspected connection".into(), ..first.clone()};
    let record = |id,input| assessment::record(&f.service, f.workspace, sit, f.principal, id, input);
    let first_id = Uuid::new_v4();
    record(first_id, first.clone()).await.unwrap();
    for input in [second.clone(), third.clone(), strategic.clone()] { record(Uuid::new_v4(), input).await.unwrap(); }
    assert!(matches!(record(Uuid::new_v4(), AssessmentInput {target: AssessmentTarget::Recommendation {recommendation_id: recommendation}, ..first.clone()}).await, Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(), AssessmentInput {target: AssessmentTarget::Decision {decision_id: Uuid::new_v4()}, ..first.clone()}).await, Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(), AssessmentInput {dimension: AssessmentDimension::DecisionActualCircumstancesAdequacy, revises_event_id: Some(first_id), ..first.clone()}).await, Err(WorkbenchError::Invalid)));
    assert!(matches!(assessment::record(&f.service, f.workspace, other, f.principal, Uuid::new_v4(), first.clone()).await, Err(WorkbenchError::Invisible)));
    let history = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(history.assessments.iter().map(|a| &a.input).collect::<Vec<_>>(), vec![&first, &second, &third, &strategic]);
    assert_eq!(history.action_decisions.len(), 1); assert_eq!(history.action_recommendations.len(), 1);
    let next = Uuid::new_v4(); action_decision::record(&f.service, f.workspace, sit, f.principal, next, DecisionInput {recommendation_id: recommendation, disposition: DecisionDisposition::SelectPrimary, alternative_index: None, rationale: "Reconsider in April".into(), revises_decision_id: Some(decision)}).await.unwrap();
    assert_eq!(epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap().assessments[0].input.target, first.target);
    let mut tx = f.service.begin().await.unwrap(); sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id,target_decision_id,revises_event_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'DECISION_CONTEMPORANEOUS_REASONABLENESS','ADEQUATE','MODERATE',$6,$7,'Retargeted',$8)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(next).bind(first_id).bind(&first.criterion).bind(&first.context).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514")); tx.rollback().await.unwrap();
}

#[tokio::test]
async fn action_recommendations_are_attributed_revisable_and_not_authorizations() {
    use epistemic_workbench::action_recommendation::{self, RecommendationInput};
    let f = fixture().await;
    let sit = situation(&f, "Possible exposure of sensitive records").await;
    let other = situation(&f, "Unrelated investigation").await;
    let ground = node(&f, sit, ExpertNodeKind::Proposition, "Privileges remain broad").await;
    let foreign = node(&f, other, ExpertNodeKind::Proposition, "Unrelated record").await;
    let recipient = Uuid::new_v4();
    let another_recipient = Uuid::new_v4();
    for actor in [recipient, another_recipient] {
        niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, actor, "Reviewer").await.unwrap();
    }
    let input = recommendation_input(ground, recipient);
    let before = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    let record = |id, input| action_recommendation::record(&f.service, f.workspace, sit, f.principal, id, input);
    let id = Uuid::new_v4();
    let first = record(id, input.clone()).await.unwrap();
    assert_eq!(first.recorded_by, f.principal);
    assert!(!first.replayed);
    assert!(record(id, input.clone()).await.unwrap().replayed);
    assert!(matches!(record(id, RecommendationInput { risk: "Changed".into(), ..input.clone() }).await, Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(), RecommendationInput { ground_object_ids: vec![foreign], ..input.clone() }).await, Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(), RecommendationInput { recipient_actor_id: Uuid::new_v4(), ..input.clone() }).await, Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(), RecommendationInput { recipient_actor_id: another_recipient, revises_recommendation_id: Some(id), ..input.clone() }).await, Err(WorkbenchError::Invalid)));
    let revision = RecommendationInput { revises_recommendation_id: Some(id), risk: "Review discreetly".into(), ..input.clone() };
    let second_id = Uuid::new_v4();
    record(second_id, revision.clone()).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(), revision).await, Err(WorkbenchError::Conflict)));
    let items = action_recommendation::list(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].input, input);
    assert_eq!(items[1].input.revises_recommendation_id, Some(id));
    assert!(action_recommendation::list(&f.service, f.workspace, other).await.unwrap().is_empty());
    let separate = fixture().await;
    assert!(matches!(action_recommendation::list(&separate.service, separate.workspace, sit).await, Err(WorkbenchError::Invisible)));
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(before.nodes, after.nodes);
    assert_eq!(before.arguments, after.arguments);
    assert_eq!(before.assessments, after.assessments);
    assert!(after.question_decisions.is_empty());
    assert_eq!(after.action_recommendations, items);

    // Direct SQL cannot inject authority, foreign grounds, or update historical records.
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let mut payload = serde_json::to_value(&input).unwrap();
    payload["authorized"] = true.into();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id,recipient_actor_id,ground_object_ids,payload,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6::jsonb,$7)")
        .bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(recipient).bind(vec![ground]).bind(payload.to_string()).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514"));
    tx.rollback().await.unwrap();
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let mut payload = serde_json::to_value(&input).unwrap();
    payload["purpose"] = "\t\u{00a0}\u{3000}".into();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id,recipient_actor_id,ground_object_ids,payload,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6::jsonb,$7)")
        .bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(recipient).bind(vec![ground]).bind(payload.to_string()).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514"));
    tx.rollback().await.unwrap();
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let payload = serde_json::to_string(&RecommendationInput { ground_object_ids: vec![foreign], ..input.clone() }).unwrap();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_action_recommendations(workspace_id,situation_id,recommendation_id,recipient_actor_id,ground_object_ids,payload,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6::jsonb,$7)")
        .bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(recipient).bind(vec![foreign]).bind(payload).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514"));
    tx.rollback().await.unwrap();
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let rejected = sqlx::query("UPDATE niccolo.epistemic_action_recommendations SET payload=payload WHERE recommendation_id=$1").bind(id).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("42501"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn action_recommendation_api_requires_write_and_snapshot_requires_read() {
    use axum::{body::{Body, to_bytes}, http::{Request, StatusCode, header}};
    use tower::ServiceExt;
    let f = fixture().await;
    let sit = situation(&f, "Review a proposed preventive intervention").await;
    let other = situation(&f, "Separate investigation").await;
    let ground = node(&f, sit, ExpertNodeKind::Proposition, "Privileges remain broad").await;
    let recipient = Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, recipient, "Reviewer").await.unwrap();
    let input = recommendation_input(ground, recipient);
    let admin = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let writer = authz::provision_token(&admin, f.principal, f.workspace, true, true, 1).await.unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1).await.unwrap();
    let stranger = fixture().await;
    let stranger_token = authz::provision_token(&admin, stranger.principal, stranger.workspace, true, true, 1).await.unwrap();
    let secure = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();
    let router = niccolo_evidence::api::router(secure);
    let id = Uuid::new_v4();
    let path = format!("/v1/workspaces/{}/situations/{}/epistemic/action-recommendations/{}", f.workspace, sit, id);
    let request = |token: &str, body: serde_json::Value| Request::builder().method("POST").uri(&path)
        .header(header::AUTHORIZATION, format!("Bearer {token}")).header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let body = serde_json::to_value(&input).unwrap();
    assert_eq!(router.clone().oneshot(request(&reader, body.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&stranger_token, body.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    let mut invalid = body.clone();
    invalid["execute"] = true.into();
    assert_eq!(router.clone().oneshot(request(&writer, invalid)).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
    let created = router.clone().oneshot(request(&writer, body.clone())).await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[header::CACHE_CONTROL], "no-store");
    let record: serde_json::Value = serde_json::from_slice(&to_bytes(created.into_body(), 1024*1024).await.unwrap()).unwrap();
    assert_eq!(record["recommendation"]["recorded_by"], serde_json::json!(f.principal));
    assert_eq!(record["recommendation"]["input"], body);
    assert_eq!(router.clone().oneshot(request(&writer, body)).await.unwrap().status(), StatusCode::OK);
    let snapshot_request = |token: &str, situation| Request::builder().uri(format!("/v1/workspaces/{}/situations/{}/epistemic/workbench", f.workspace, situation))
        .header(header::AUTHORIZATION, format!("Bearer {token}")).body(Body::empty()).unwrap();
    assert_eq!(router.clone().oneshot(snapshot_request(&stranger_token, sit)).await.unwrap().status(), StatusCode::FORBIDDEN);
    for (situation, count) in [(sit, 1), (other, 0)] {
        let response = router.clone().oneshot(snapshot_request(&reader, situation)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let snapshot: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 1024*1024).await.unwrap()).unwrap();
        assert_eq!(snapshot["action_recommendations"].as_array().unwrap().len(), count);
        assert_eq!(snapshot["question_decisions"], serde_json::json!([]));
        if count == 1 {
            assert_eq!(snapshot["action_recommendations"][0]["recommendation_id"], serde_json::json!(id));
        }
    }
}

#[tokio::test]
async fn action_decisions_pin_actual_options_without_authorizing_or_executing() {
    use epistemic_workbench::{action_decision::{self, DecisionDisposition, DecisionInput}, action_recommendation};
    let f = fixture().await;
    let sit = situation(&f, "Consider a preventive review").await;
    let other = situation(&f, "Another investigation").await;
    let ground = node(&f, sit, ExpertNodeKind::Proposition, "Privileges are broad").await;
    let foreign_ground = node(&f, other, ExpertNodeKind::Proposition, "Another concern").await;
    let recipient = Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, recipient, "Reviewer").await.unwrap();
    let recommendation = Uuid::new_v4();
    let recommendation_input = recommendation_input(ground, recipient);
    action_recommendation::record(&f.service, f.workspace, sit, f.principal, recommendation, recommendation_input.clone()).await.unwrap();
    let foreign_recommendation = Uuid::new_v4();
    action_recommendation::record(&f.service, f.workspace, other, f.principal, foreign_recommendation, action_recommendation::RecommendationInput { ground_object_ids: vec![foreign_ground], ..recommendation_input.clone() }).await.unwrap();
    let before = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    let choice = DecisionInput { recommendation_id: recommendation, disposition: DecisionDisposition::SelectAlternative, alternative_index: Some(0), rationale: "Prefer inaction while obtaining more context".into(), revises_decision_id: None };
    let record = |id, input| action_decision::record(&f.service, f.workspace, sit, f.principal, id, input);
    let first_id = Uuid::new_v4();
    let first = record(first_id, choice.clone()).await.unwrap();
    assert!(!first.replayed);
    assert_eq!(first.recorded_by, f.principal);
    assert!(record(first_id, choice.clone()).await.unwrap().replayed);
    assert!(matches!(record(first_id, DecisionInput { rationale: "Different".into(), ..choice.clone() }).await, Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(), DecisionInput { alternative_index: Some(1), ..choice.clone() }).await, Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(), DecisionInput { recommendation_id: foreign_recommendation, ..choice.clone() }).await, Err(WorkbenchError::Invisible)));

    let revised_recommendation = Uuid::new_v4();
    action_recommendation::record(&f.service, f.workspace, sit, f.principal, revised_recommendation, action_recommendation::RecommendationInput { revises_recommendation_id: Some(recommendation), risk: "Changed context".into(), ..recommendation_input }).await.unwrap();
    let revision = DecisionInput { disposition: DecisionDisposition::Defer, alternative_index: None, revises_decision_id: Some(first_id), rationale: "Reconsider the originally proposed options".into(), ..choice.clone() };
    assert!(matches!(record(Uuid::new_v4(), DecisionInput { recommendation_id: revised_recommendation, ..revision.clone() }).await, Err(WorkbenchError::Invalid)));
    let other_principal = Uuid::new_v4();
    let admin = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    authz::provision_token(&admin, other_principal, f.workspace, true, true, 1).await.unwrap();
    assert!(matches!(action_decision::record(&f.service, f.workspace, sit, other_principal, Uuid::new_v4(), revision.clone()).await, Err(WorkbenchError::Invalid)));
    let second_id = Uuid::new_v4();
    record(second_id, revision.clone()).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(), revision).await, Err(WorkbenchError::Conflict)));
    // A decision about a new recommendation revision is a new explicit choice, never auto-rebound.
    let new_choice = DecisionInput { recommendation_id: revised_recommendation, disposition: DecisionDisposition::SelectPrimary, alternative_index: None, rationale: "Consider the changed proposal".into(), revises_decision_id: None };
    record(Uuid::new_v4(), new_choice).await.unwrap();
    let history = action_decision::list(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].input, choice);
    assert_eq!(history[1].input.recommendation_id, recommendation);
    assert_eq!(history[2].input.recommendation_id, revised_recommendation);
    assert!(action_decision::list(&f.service, f.workspace, other).await.unwrap().is_empty());
    let separate = fixture().await;
    assert!(matches!(action_decision::list(&separate.service, separate.workspace, sit).await, Err(WorkbenchError::Invisible)));
    let after = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(before.nodes, after.nodes);
    assert_eq!(before.assessments, after.assessments);
    assert_eq!(before.arguments, after.arguments);
    assert!(after.question_decisions.is_empty());
    assert_eq!(after.action_recommendations.len(), 2);
    assert_eq!(after.action_decisions, history);

    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_action_decisions(workspace_id,situation_id,decision_id,recommendation_id,disposition,alternative_index,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,'SELECT_ALTERNATIVE',7,'Invalid option',$5)")
        .bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(recommendation).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514"));
    tx.rollback().await.unwrap();
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let rejected = sqlx::query("UPDATE niccolo.epistemic_action_decisions SET disposition='DECLINE' WHERE decision_id=$1").bind(first_id).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("42501"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn intervention_records_preserve_occurrence_and_causality_independently() {
    use epistemic_workbench::intervention::{self, CausalAssessmentInput, CausalAssessmentKind, Characterization, InterventionInput, RecordKind};
    let f = fixture().await;
    let sit = situation(&f, "Assess changes after a privilege review").await;
    let other = situation(&f, "Unrelated circumstances").await;
    let record = |id, input| intervention::record(&f.service, f.workspace, sit, f.principal, id, input);
    let execution = Uuid::new_v4();
    let event = Uuid::new_v4();
    let report = InterventionInput { kind: RecordKind::ExecutionOccurrence, linked_decision_id: None, characterization: Characterization::Reported, description: "A manager reports access was restricted".into(), event_temporal_scope: "January".into(), acquisition_temporal_scope: "March interview".into(), basis_evidence_id: None, revises_record_id: None };
    assert!(!record(execution, report.clone()).await.unwrap().replayed);
    assert!(record(execution, report.clone()).await.unwrap().replayed);
    assert!(matches!(record(execution, InterventionInput { description: "Changed".into(), ..report.clone() }).await, Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(), InterventionInput { characterization: Characterization::Observed, ..report.clone() }).await, Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(), InterventionInput { basis_evidence_id: Some(Uuid::new_v4()), ..report.clone() }).await, Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(), InterventionInput { linked_decision_id: Some(Uuid::new_v4()), ..report.clone() }).await, Err(WorkbenchError::Invisible)));
    let observation = InterventionInput { kind: RecordKind::EventObservation, description: "Employee reportedly seeks another role".into(), event_temporal_scope: "Unknown start date".into(), ..report.clone() };
    record(event, observation.clone()).await.unwrap();
    let (records, assessments) = intervention::list(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(records.len(), 2);
    assert!(assessments.is_empty());
    let causal = CausalAssessmentInput { execution_record_id: execution, event_observation_record_id: event, assessment: CausalAssessmentKind::PossiblyInduced, rationale: "Reported behavior may follow the change".into(), alternative_explanations: "Existing career plans".into(), uncertainty: "Onset and cause unknown".into(), basis_evidence_id: None, revises_assessment_id: None };
    let assess = |id, input| intervention::assess(&f.service, f.workspace, sit, f.principal, id, input);
    let assessment_id = Uuid::new_v4();
    assert!(!assess(assessment_id, causal.clone()).await.unwrap().replayed);
    assert!(assess(assessment_id, causal.clone()).await.unwrap().replayed);
    assert!(matches!(assess(Uuid::new_v4(), CausalAssessmentInput { execution_record_id: event, event_observation_record_id: execution, ..causal.clone() }).await, Err(WorkbenchError::Invalid)));
    let foreign = Uuid::new_v4();
    intervention::record(&f.service, f.workspace, other, f.principal, foreign, observation.clone()).await.unwrap();
    assert!(matches!(assess(Uuid::new_v4(), CausalAssessmentInput { event_observation_record_id: foreign, ..causal.clone() }).await, Err(WorkbenchError::Invisible)));
    let revised_event = Uuid::new_v4();
    record(revised_event, InterventionInput { revises_record_id: Some(event), event_temporal_scope: "December or earlier".into(), ..observation.clone() }).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(), InterventionInput { revises_record_id: Some(event), ..observation.clone() }).await, Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(), InterventionInput { revises_record_id: Some(execution), ..observation }).await, Err(WorkbenchError::Invalid)));
    let revision = CausalAssessmentInput { assessment: CausalAssessmentKind::Undetermined, revises_assessment_id: Some(assessment_id), ..causal.clone() };
    assert!(matches!(assess(Uuid::new_v4(), CausalAssessmentInput { event_observation_record_id: revised_event, ..revision.clone() }).await, Err(WorkbenchError::Invalid)));
    assess(Uuid::new_v4(), revision.clone()).await.unwrap();
    assert!(matches!(assess(Uuid::new_v4(), revision).await, Err(WorkbenchError::Conflict)));
    let (records, assessments) = intervention::list(&f.service, f.workspace, sit).await.unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(assessments.len(), 2);
    assert_eq!(assessments[0].input, causal);
    assert_eq!(assessments[1].input.event_observation_record_id, event);
    assert!(intervention::list(&f.service, f.workspace, other).await.unwrap().1.is_empty());
    let separate = fixture().await;
    assert!(matches!(intervention::list(&separate.service, separate.workspace, sit).await, Err(WorkbenchError::Invisible)));
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_intervention_causal_assessments(workspace_id,situation_id,assessment_id,execution_record_id,event_observation_record_id,assessment,rationale,alternative_explanations,uncertainty,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'UNDETERMINED','Rationale','Alternatives','Unknown',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(event).bind(execution).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn causal_reviews_pin_changed_basis_and_reopen_only_on_new_changes() {
    use epistemic_workbench::intervention::{self, CausalAssessmentInput, CausalAssessmentKind, CausalReviewDisposition, CausalReviewInput, Characterization, InterventionInput, RecordKind};
    let f = fixture().await;
    let sit = situation(&f, "Intervention review").await;
    let other = situation(&f, "Unrelated context").await;
    let execution = Uuid::new_v4(); let event = Uuid::new_v4(); let assessment_id = Uuid::new_v4();
    let record = |id, input| intervention::record(&f.service, f.workspace, sit, f.principal, id, input);
    let base = InterventionInput { kind: RecordKind::ExecutionOccurrence, linked_decision_id: None, characterization: Characterization::Reported, description: "Review reported".into(), event_temporal_scope: "January".into(), acquisition_temporal_scope: "March".into(), basis_evidence_id: None, revises_record_id: None };
    let observed = InterventionInput { kind: RecordKind::EventObservation, description: "Employee reportedly explores other work".into(), ..base.clone() };
    record(execution, base.clone()).await.unwrap(); record(event, observed.clone()).await.unwrap();
    let causal = CausalAssessmentInput { execution_record_id: execution, event_observation_record_id: event, assessment: CausalAssessmentKind::Undetermined, rationale: "Limited timing information".into(), alternative_explanations: "Prior plans".into(), uncertainty: "Onset unknown".into(), basis_evidence_id: None, revises_assessment_id: None };
    intervention::assess(&f.service, f.workspace, sit, f.principal, assessment_id, causal.clone()).await.unwrap();
    let review = |id, input| intervention::review(&f.service, f.workspace, sit, f.principal, id, input);
    let first = CausalReviewInput { assessment_id, reviewed_execution_record_id: execution, reviewed_event_record_id: event, disposition: CausalReviewDisposition::Retained, note: "Retain historical judgment as undetermined".into() };
    assert!(matches!(review(Uuid::new_v4(), first.clone()).await, Err(WorkbenchError::Invalid)));
    let revised = Uuid::new_v4();
    record(revised, InterventionInput { revises_record_id: Some(event), event_temporal_scope: "December".into(), ..observed.clone() }).await.unwrap();
    assert!(matches!(review(Uuid::new_v4(), first.clone()).await, Err(WorkbenchError::Invalid)));
    let review_input = CausalReviewInput { reviewed_event_record_id: revised, ..first };
    let review_id = Uuid::new_v4();
    assert!(!review(review_id, review_input.clone()).await.unwrap().replayed);
    assert!(review(review_id, review_input.clone()).await.unwrap().replayed);
    let get = || epistemic_workbench::snapshot(&f.service, f.workspace, sit);
    let view = get().await.unwrap().intervention_causal_assessments.pop().unwrap();
    assert!(!view.requires_review); assert!(view.event_observation_revised); assert_eq!(view.latest_review.unwrap().id, review_id); assert_eq!(view.assessment.input, causal);
    let deferred = CausalReviewInput { disposition: CausalReviewDisposition::Deferred, ..review_input.clone() };
    review(Uuid::new_v4(), deferred).await.unwrap();
    assert!(get().await.unwrap().intervention_causal_assessments[0].requires_review);
    review(Uuid::new_v4(), review_input.clone()).await.unwrap();
    let revised_again = Uuid::new_v4();
    record(revised_again, InterventionInput { revises_record_id: Some(revised), event_temporal_scope: "November".into(), ..observed.clone() }).await.unwrap();
    let view = get().await.unwrap().intervention_causal_assessments.pop().unwrap();
    assert!(view.requires_review); assert!(view.latest_review.is_none()); assert_eq!(view.review_history.len(), 3); assert_eq!(view.current_event_record_id, revised_again);
    assert!(matches!(review(Uuid::new_v4(), review_input.clone()).await, Err(WorkbenchError::Invalid)));
    assert!(review(review_id, review_input.clone()).await.unwrap().replayed);
    let foreign = Uuid::new_v4(); intervention::record(&f.service, f.workspace, other, f.principal, foreign, observed).await.unwrap();
    assert!(matches!(review(Uuid::new_v4(), CausalReviewInput { reviewed_event_record_id: foreign, ..review_input.clone() }).await, Err(WorkbenchError::Invalid)));
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let rejected = sqlx::query("INSERT INTO niccolo.epistemic_intervention_causal_reviews(workspace_id,situation_id,review_id,assessment_id,reviewed_execution_record_id,reviewed_event_record_id,disposition,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,'RETAINED','Stale',$7)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(assessment_id).bind(execution).bind(revised).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(rejected.as_database_error().unwrap().code().as_deref(), Some("23514")); tx.rollback().await.unwrap();
}

#[tokio::test]
async fn intervention_api_preserves_attribution_and_selective_reassessment() {
    use axum::{body::{Body, to_bytes}, http::{Request, StatusCode, header}};
    use tower::ServiceExt;
    let f = fixture().await;
    let sit = situation(&f, "Intervention feedback").await;
    let other = situation(&f, "Separate investigation").await;
    let admin = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let author = Uuid::new_v4();
    let writer = authz::provision_token(&admin, author, f.workspace, true, true, 1).await.unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1).await.unwrap();
    let secure = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();
    let router = niccolo_evidence::api::router(secure);
    let request = |token: &str, situation, collection: &str, id, body: serde_json::Value| Request::builder().method("POST")
        .uri(format!("/v1/workspaces/{}/situations/{}/epistemic/{}/{}", f.workspace, situation, collection, id))
        .header(header::AUTHORIZATION, format!("Bearer {token}")).header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())).unwrap();
    let execution = Uuid::new_v4();
    let event = Uuid::new_v4();
    let unrelated = Uuid::new_v4();
    let record = serde_json::json!({"kind":"EXECUTION_OCCURRENCE","linked_decision_id":null,"characterization":"REPORTED","description":"Review reportedly occurred","event_temporal_scope":"January","acquisition_temporal_scope":"March","basis_evidence_id":null,"revises_record_id":null});
    assert_eq!(router.clone().oneshot(request(&reader, sit, "intervention-records", execution, record.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    let mut invalid = record.clone(); invalid["execute"] = true.into();
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-records", execution, invalid)).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
    let created = router.clone().oneshot(request(&writer, sit, "intervention-records", execution, record.clone())).await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[header::CACHE_CONTROL], "no-store");
    let saved: serde_json::Value = serde_json::from_slice(&to_bytes(created.into_body(), 1024*1024).await.unwrap()).unwrap();
    assert_eq!(saved["record"]["recorded_by"], serde_json::json!(author));
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-records", execution, record.clone())).await.unwrap().status(), StatusCode::OK);
    let mut observation = record.clone(); observation["kind"] = "EVENT_OBSERVATION".into(); observation["description"] = "Employee explores other work".into();
    for id in [event, unrelated] { assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-records", id, observation.clone())).await.unwrap().status(), StatusCode::CREATED); }
    let assessment_id = Uuid::new_v4();
    let causal = serde_json::json!({"execution_record_id":execution,"event_observation_record_id":event,"assessment":"POSSIBLY_INDUCED","rationale":"Reported timing","alternative_explanations":"Prior plans","uncertainty":"Causation unclear","basis_evidence_id":null,"revises_assessment_id":null});
    assert_eq!(router.clone().oneshot(request(&reader, sit, "intervention-causal-assessments", assessment_id, causal.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&writer, other, "intervention-causal-assessments", assessment_id, causal.clone())).await.unwrap().status(), StatusCode::NOT_FOUND);
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-causal-assessments", assessment_id, causal.clone())).await.unwrap().status(), StatusCode::CREATED);
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-causal-assessments", assessment_id, causal.clone())).await.unwrap().status(), StatusCode::OK);
    let mut independent = causal.clone(); independent["event_observation_record_id"] = serde_json::json!(unrelated);
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-causal-assessments", Uuid::new_v4(), independent)).await.unwrap().status(), StatusCode::CREATED);
    observation["revises_record_id"] = serde_json::json!(event); observation["event_temporal_scope"] = "December".into();
    let revised_event = Uuid::new_v4();
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-records", revised_event, observation)).await.unwrap().status(), StatusCode::CREATED);
    let snapshot = router.clone().oneshot(Request::builder().uri(format!("/v1/workspaces/{}/situations/{}/epistemic/workbench", f.workspace, sit)).header(header::AUTHORIZATION, format!("Bearer {reader}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(snapshot.status(), StatusCode::OK);
    let data: serde_json::Value = serde_json::from_slice(&to_bytes(snapshot.into_body(), 1024*1024).await.unwrap()).unwrap();
    assert_eq!(data["intervention_records"].as_array().unwrap().len(), 4);
    let assessments = data["intervention_causal_assessments"].as_array().unwrap();
    assert_eq!(assessments.len(), 2);
    assert_eq!(assessments[0]["assessment"]["input"], causal);
    assert_eq!(assessments[0]["requires_review"], true);
    assert_eq!(assessments[1]["requires_review"], false);
    assert_eq!(data["action_decisions"], serde_json::json!([]));
    let review_id = Uuid::new_v4();
    let review = serde_json::json!({"assessment_id":assessment_id,"reviewed_execution_record_id":execution,"reviewed_event_record_id":revised_event,"disposition":"RETAINED","note":"Historical judgment still justified as tentative"});
    assert_eq!(router.clone().oneshot(request(&reader, sit, "intervention-causal-reviews", review_id, review.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-causal-reviews", review_id, review.clone())).await.unwrap().status(), StatusCode::CREATED);
    assert_eq!(router.clone().oneshot(request(&writer, sit, "intervention-causal-reviews", review_id, review)).await.unwrap().status(), StatusCode::OK);
    let snapshot = router.oneshot(Request::builder().uri(format!("/v1/workspaces/{}/situations/{}/epistemic/workbench", f.workspace, sit)).header(header::AUTHORIZATION, format!("Bearer {reader}")).body(Body::empty()).unwrap()).await.unwrap();
    let data: serde_json::Value = serde_json::from_slice(&to_bytes(snapshot.into_body(), 1024*1024).await.unwrap()).unwrap();
    assert_eq!(data["intervention_causal_assessments"][0]["requires_review"], false);
    assert_eq!(data["intervention_causal_assessments"][0]["latest_review"]["recorded_by"], serde_json::json!(author));
}

#[tokio::test]
async fn action_decision_api_authenticates_the_decider_and_preserves_pinned_history() {
    use axum::{body::{Body, to_bytes}, http::{Request, StatusCode, header}};
    use epistemic_workbench::{action_decision::{DecisionDisposition, DecisionInput}, action_recommendation};
    use tower::ServiceExt;
    let f = fixture().await;
    let sit = situation(&f, "Decide whether to review privileges").await;
    let other = situation(&f, "Separate investigation").await;
    let ground = node(&f, sit, ExpertNodeKind::Proposition, "Privileges remain broad").await;
    let recipient = Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service, f.workspace, f.principal, recipient, "Reviewer").await.unwrap();
    let recommendation = Uuid::new_v4();
    let rec_input = recommendation_input(ground, recipient);
    action_recommendation::record(&f.service, f.workspace, sit, f.principal, recommendation, rec_input.clone()).await.unwrap();
    let admin = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let decider = Uuid::new_v4();
    let writer = authz::provision_token(&admin, decider, f.workspace, true, true, 1).await.unwrap();
    let other_writer = authz::provision_token(&admin, f.principal, f.workspace, true, true, 1).await.unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), f.workspace, true, false, 1).await.unwrap();
    let stranger = fixture().await;
    let stranger_token = authz::provision_token(&admin, stranger.principal, stranger.workspace, true, true, 1).await.unwrap();
    let secure = PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();
    let router = niccolo_evidence::api::router(secure);
    let id = Uuid::new_v4();
    let choice = DecisionInput { recommendation_id: recommendation, disposition: DecisionDisposition::SelectAlternative, alternative_index: Some(0), rationale: "Prefer the recorded alternative".into(), revises_decision_id: None };
    let request = |token: &str, situation, id, body: serde_json::Value| Request::builder().method("POST")
        .uri(format!("/v1/workspaces/{}/situations/{}/epistemic/action-decisions/{}", f.workspace, situation, id))
        .header(header::AUTHORIZATION, format!("Bearer {token}")).header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let body = serde_json::to_value(&choice).unwrap();
    assert_eq!(router.clone().oneshot(request(&reader, sit, id, body.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&stranger_token, sit, id, body.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&writer, other, id, body.clone())).await.unwrap().status(), StatusCode::NOT_FOUND);
    let mut invalid = body.clone(); invalid["authorize"] = true.into();
    assert_eq!(router.clone().oneshot(request(&writer, sit, id, invalid)).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
    let created = router.clone().oneshot(request(&writer, sit, id, body.clone())).await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(created.headers()[header::CACHE_CONTROL], "no-store");
    let stored: serde_json::Value = serde_json::from_slice(&to_bytes(created.into_body(), 1024*1024).await.unwrap()).unwrap();
    assert_eq!(stored["decision"]["recorded_by"], serde_json::json!(decider));
    assert_eq!(stored["decision"]["input"], body);
    assert_eq!(router.clone().oneshot(request(&writer, sit, id, body)).await.unwrap().status(), StatusCode::OK);
    let revised_rec = Uuid::new_v4();
    action_recommendation::record(&f.service, f.workspace, sit, f.principal, revised_rec, action_recommendation::RecommendationInput { revises_recommendation_id: Some(recommendation), ..rec_input }).await.unwrap();
    let revision = serde_json::to_value(DecisionInput { disposition: DecisionDisposition::Defer, alternative_index: None, revises_decision_id: Some(id), ..choice }).unwrap();
    let revision_id = Uuid::new_v4();
    assert_eq!(router.clone().oneshot(request(&other_writer, sit, revision_id, revision.clone())).await.unwrap().status(), StatusCode::BAD_REQUEST);
    assert_eq!(router.clone().oneshot(request(&writer, sit, revision_id, revision)).await.unwrap().status(), StatusCode::CREATED);
    let snapshot = router.oneshot(Request::builder().uri(format!("/v1/workspaces/{}/situations/{}/epistemic/workbench", f.workspace, sit))
        .header(header::AUTHORIZATION, format!("Bearer {reader}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(snapshot.status(), StatusCode::OK);
    assert_eq!(snapshot.headers()[header::CACHE_CONTROL], "no-store");
    let data: serde_json::Value = serde_json::from_slice(&to_bytes(snapshot.into_body(), 1024*1024).await.unwrap()).unwrap();
    assert_eq!(data["action_decisions"].as_array().unwrap().len(), 2);
    assert_eq!(data["action_decisions"][0]["input"]["recommendation_id"], serde_json::json!(recommendation));
    assert_eq!(data["action_decisions"][1]["input"]["recommendation_id"], serde_json::json!(recommendation));
    assert_eq!(data["action_recommendations"].as_array().unwrap().len(), 2);
    assert_eq!(data["question_decisions"], serde_json::json!([]));
}

#[tokio::test]
async fn actor_epistemic_states_are_independent_attributed_and_scoped() {
    use epistemic_workbench::actor_state::{self,StateInput,StateDimension,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Assess awareness without assuming knowledge").await;
    let target=node(&f,sit,ExpertNodeKind::Proposition,"The ledger was delivered").await;
    let actor=Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,"Interviewee").await.unwrap();
    let input=StateInput {subject_actor_id:actor,target_object_id:Some(target),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::Exposure,judgment:StateJudgment::Indicated,context:"Delivery interview".into(),temporal_scope:"Before the hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported being shown the ledger".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|event,input|actor_state::record(&f.service,f.workspace,sit,f.principal,event,input);
    let exposed=Uuid::new_v4();
    assert!(!record(exposed,input.clone()).await.unwrap().replayed);
    assert!(record(exposed,input.clone()).await.unwrap().replayed);
    assert!(matches!(record(exposed,StateInput{judgment:StateJudgment::Unknown,..input.clone()}).await,Err(WorkbenchError::Conflict)));
    let belief=Uuid::new_v4();
    record(belief,StateInput{dimension:StateDimension::Belief,judgment:StateJudgment::Unknown,..input.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::EvidenceSupportedKnowledge,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Comprehension,revises_event_id:Some(exposed),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    let other=situation(&f,"Separate investigation").await;
    let foreign=node(&f,other,ExpertNodeKind::Proposition,"Foreign proposition").await;
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:Some(foreign),..input.clone()}).await,Err(WorkbenchError::Invisible)));
    let revised=Uuid::new_v4();
    record(revised,StateInput{judgment:StateJudgment::Contested,revises_event_id:Some(exposed),..input.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),3);
    assert_eq!(snapshot.actor_epistemic_states[0].input,input);
    assert_eq!(snapshot.actor_epistemic_states[1].event_id,belief);
    assert_eq!(snapshot.actor_epistemic_states[2].input.revises_event_id,Some(exposed));
    assert!(snapshot.assessments.is_empty() && snapshot.arguments.is_empty());
    assert!(epistemic_workbench::snapshot(&f.service,f.workspace,other).await.unwrap().actor_epistemic_states.is_empty());
}

#[tokio::test]
async fn distributed_actor_records_do_not_create_collective_knowledge() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Review distributed information").await;
    let target=node(&f,sit,ExpertNodeKind::Proposition,"The ledger was delivered").await;
    let a=Uuid::new_v4();let b=Uuid::new_v4();let group=Uuid::new_v4();
    for (id,label) in [(a,"Witness"),(b,"Clerk"),(group,"Organization")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let document=Uuid::new_v4();
    let sources=ingest_source(&f.service,f.workspace,document,SourceFormat::WhatsappExport,"fixture://distributed",b"[02/11/2026, 12:00] Witness: I saw the envelope\n[02/11/2026, 12:01] Clerk: I saw a receipt\n").await.unwrap();
    situation_store::add_source(&f.service,f.workspace,f.principal,sit,document).await.unwrap();
    assert_eq!(sources.fragment_ids.len(),2);
    let first=StateInput{subject_actor_id:a,target_object_id:Some(target),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::Exposure,judgment:StateJudgment::Indicated,context:"Delivery review".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Attributed observation of envelope".into(),basis_evidence_id:Some(sources.fragment_ids[0]),revises_event_id:None};
    let second=StateInput{subject_actor_id:b,dimension:StateDimension::Access,rationale:"Attributed observation of receipt".into(),basis_evidence_id:Some(sources.fragment_ids[1]),..first.clone()};
    let first_id=Uuid::new_v4();let second_id=Uuid::new_v4();let revised_id=Uuid::new_v4();
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    record(first_id,first.clone()).await.unwrap();record(second_id,second.clone()).await.unwrap();
    record(revised_id,StateInput{judgment:StateJudgment::Contested,revises_event_id:Some(first_id),..first.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),3);
    assert_eq!(snapshot.actor_epistemic_states[0].input,first);
    assert_eq!(snapshot.actor_epistemic_states[1].input,second);
    assert_eq!(snapshot.actor_epistemic_states[2].input.revises_event_id,Some(first_id));
    assert!(snapshot.actor_epistemic_states.iter().all(|event|event.input.subject_actor_id!=group&&event.input.dimension!=StateDimension::EvidenceSupportedKnowledge));
    assert!(snapshot.assessments.is_empty()&&snapshot.arguments.is_empty());
}

#[tokio::test]
async fn silence_does_not_establish_assent_or_actual_nonresponse() {
    use epistemic_workbench::{
        actor_state::{self, StateDimension as D, StateInput, StateJudgment as J},
        inquiry_response::{self, ResponseInput},
    };
    let f = fixture().await;
    let sit = situation(&f, "AT-19: an executive equates no objection with agreement").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Do you agree with the proposed change?").await;
    let executive = Uuid::new_v4();
    let agree = Uuid::new_v4();
    let dissent = Uuid::new_v4();
    let silent = Uuid::new_v4();
    let unread = Uuid::new_v4();
    let fearful = Uuid::new_v4();
    for (id, label) in [(executive,"Executive"),(agree,"Agrees"),(dissent,"Dissents"),(silent,"Silent"),(unread,"Unread"),(fearful,"Fearful")] {
        niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();
    }
    let doc = Uuid::new_v4();
    let imported = ingest_source(&f.service,f.workspace,doc,SourceFormat::WhatsappExport,"fixture://at19",b"[02/11/2026, 12:00] Executive: Do you agree with the proposed change?\n[02/11/2026, 12:01] Agrees: Yes\n[02/11/2026, 12:02] Dissents: No\n").await.unwrap();
    situation_store::add_source(&f.service,f.workspace,f.principal,sit,doc).await.unwrap();
    for (speaker,text,basis) in [(agree,"Yes",imported.fragment_ids[1]),(dissent,"No",imported.fragment_ids[2])] {
        inquiry_response::record(&f.service,f.workspace,sit,f.principal,q,Uuid::new_v4(),ResponseInput{utterance_id:Uuid::new_v4(),utterance_text:text.into(),speaker_actor_id:Some(speaker),source_evidence_id:Some(basis),source_contribution_id:None,rationale:"Original response occurrence".into()}).await.unwrap();
    }
    let base = StateInput {subject_actor_id:silent,target_object_id:Some(q),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:Some(executive),influence_target:None,dimension:D::ResponseObservedAbsence,judgment:J::Indicated,context:"Partial group chat export; other channels not collected".into(),temporal_scope:"12:00–12:02 on 2 November".into(),orientation_scope:None,orientation_goal:None,rationale:"No reply by this participant appears within the inspected export; this does not establish no reply elsewhere".into(),basis_evidence_id:Some(imported.fragment_ids[0]),revises_event_id:None};
    let record = |id,input| actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let absence = Uuid::new_v4();
    record(absence,base.clone()).await.unwrap();
    assert!(record(absence,base.clone()).await.unwrap().replayed);
    assert!(matches!(record(Uuid::new_v4(),StateInput{basis_evidence_id:None,..base.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(absence),..base.clone()}).await,Err(WorkbenchError::Invalid)));
    for (subject_actor_id,dimension,judgment,rationale) in [
        (silent,D::ResponseActualNonresponse,J::Unknown,"Other channels have not been examined"),
        (silent,D::ResponseExpectation,J::Indicated,"A reply was requested"),
        (silent,D::ResponseOpportunity,J::Unknown,"Availability not known"),
        (unread,D::Examination,J::NotIndicated,"Attributed report of unread proposal"),
        (unread,D::ResponseOpportunity,J::Unknown,"No established opportunity"),
        (fearful,D::Pressure,J::Indicated,"Attributed concern about retaliation"),
        (fearful,D::ResponseDeliberateNonresponse,J::Contested,"Possible intentional avoidance; alternative explanations remain"),
    ] {
        // Examination is an undirected state; response and pressure facets may name a counterpart.
        let counterparty_actor_id = (dimension != D::Examination).then_some(executive);
        record(Uuid::new_v4(),StateInput{subject_actor_id,dimension,judgment,rationale:rationale.into(),counterparty_actor_id,basis_evidence_id:None,..base.clone()}).await.unwrap();
    }
    for subject_actor_id in [agree,silent,unread,fearful] {
        record(Uuid::new_v4(),StateInput{subject_actor_id,dimension:D::ResponseInterpretedAssent,judgment:J::Indicated,rationale:"Executive interprets this participant as agreeing; not an actual consent finding".into(),basis_evidence_id:None,..base.clone()}).await.unwrap();
    }
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:D::ResponseInterpretedAssent,counterparty_actor_id:None,..base.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:D::ResponseActualNonresponse,revises_event_id:Some(absence),..base.clone()}).await,Err(WorkbenchError::Invalid)));
    let revision = Uuid::new_v4();
    record(revision,StateInput{judgment:J::Contested,revises_event_id:Some(absence),rationale:"Another collection path may contain a reply".into(),..base.clone()}).await.unwrap();
    let snapshot = epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.question_responses.len(),2);
    assert_eq!(snapshot.actor_epistemic_states.len(),13);
    assert_eq!(snapshot.actor_epistemic_states.iter().filter(|e|e.input.dimension==D::ResponseInterpretedAssent).count(),4);
    assert!(!snapshot.actor_epistemic_states.iter().any(|e|matches!(e.input.dimension,D::Consent|D::Compliance|D::Loyalty|D::EvidenceSupportedKnowledge)));
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.event_id==absence&&e.input==base));
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.event_id==revision&&e.input.revises_event_id==Some(absence)));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty()&&snapshot.response_interpretations.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'RESPONSE_OBSERVED_ABSENCE','INDICATED','context','time','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(silent).bind(q).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn actor_state_basis_and_writer_authority_are_checked_at_the_boundary() {
    use axum::{body::Body,http::{Request,StatusCode,header}};
    use epistemic_workbench::actor_state::{self,StateInput,StateDimension,StateJudgment};
    use tower::ServiceExt;
    let f=fixture().await;
    let sit=situation(&f,"Record attributed knowledge evidence").await;
    let other=situation(&f,"Not this investigation").await;
    let target=node(&f,sit,ExpertNodeKind::Proposition,"The ledger was delivered").await;
    let foreign=node(&f,other,ExpertNodeKind::Proposition,"Unrelated ledger").await;
    let actor=Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,"Witness").await.unwrap();
    let document=Uuid::new_v4();
    let imported=ingest_source(&f.service,f.workspace,document,SourceFormat::WhatsappExport,"fixture://state-basis",b"[02/11/2026, 12:00] Witness: I read the ledger\n").await.unwrap();
    let basis=imported.fragment_ids[0];
    let input=StateInput{subject_actor_id:actor,target_object_id:Some(target),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::EvidenceSupportedKnowledge,judgment:StateJudgment::Indicated,context:"Interview".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Explicit source report; not established fact".into(),basis_evidence_id:Some(basis),revises_event_id:None};
    let id=Uuid::new_v4();
    assert!(matches!(actor_state::record(&f.service,f.workspace,sit,f.principal,id,input.clone()).await,Err(WorkbenchError::Invisible)));
    situation_store::add_source(&f.service,f.workspace,f.principal,sit,document).await.unwrap();
    let admin=PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let writer=authz::provision_token(&admin,f.principal,f.workspace,true,true,1).await.unwrap();
    let reader=authz::provision_token(&admin,Uuid::new_v4(),f.workspace,true,false,1).await.unwrap();
    let secure=PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();
    let router=niccolo_evidence::api::router(secure);
    let path=format!("/v1/workspaces/{}/situations/{}/epistemic/actor-states/{}",f.workspace,sit,id);
    let request=|token:&str|Request::builder().method("POST").uri(&path).header(header::AUTHORIZATION,format!("Bearer {token}")).header(header::CONTENT_TYPE,"application/json").body(Body::from(serde_json::to_string(&input).unwrap())).unwrap();
    assert_eq!(router.clone().oneshot(request(&reader)).await.unwrap().status(),StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&writer)).await.unwrap().status(),StatusCode::CREATED);
    assert_eq!(router.oneshot(request(&writer)).await.unwrap().status(),StatusCode::OK);
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states[0].recorded_by,f.principal);
    assert_eq!(snapshot.actor_epistemic_states[0].input,input);
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'EXPOSURE','INDICATED','context','time','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(foreign).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn higher_order_actor_states_pin_exact_prior_events_without_depth_or_truth_promotion() {
    use epistemic_workbench::actor_state::{self,StateInput,StateDimension,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Nested hypotheses about awareness").await;
    let target=node(&f,sit,ExpertNodeKind::Proposition,"The ledger was delivered").await;
    let a=Uuid::new_v4(); let b=Uuid::new_v4();
    for (actor,label) in [(a,"Actor A"),(b,"Actor B")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,label).await.unwrap();}
    let base=StateInput{subject_actor_id:a,target_object_id:Some(target),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::ClaimedKnowledge,judgment:StateJudgment::Indicated,context:"Interview".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Attributed report".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|event,input|actor_state::record(&f.service,f.workspace,sit,f.principal,event,input);
    let root=Uuid::new_v4();record(root,base.clone()).await.unwrap();
    let mut previous=root;let mut chain=vec![];
    for level in 0..8 {let id=Uuid::new_v4();let input=StateInput{subject_actor_id:if level%2==0{b}else{a},target_object_id:None,target_state_event_id:Some(previous),dimension:StateDimension::Belief,judgment:StateJudgment::Unknown,..base.clone()};record(id,input.clone()).await.unwrap();chain.push((id,input));previous=id;}
    let malformed=StateInput{target_state_event_id:Some(root),..base.clone()};
    assert!(matches!(record(Uuid::new_v4(),malformed).await,Err(WorkbenchError::Invalid)));
    let self_id=Uuid::new_v4();assert!(matches!(record(self_id,StateInput{target_object_id:None,target_state_event_id:Some(self_id),..base.clone()}).await,Err(WorkbenchError::Invalid)));
    let other=situation(&f,"Separate investigation").await;let foreign=node(&f,other,ExpertNodeKind::Proposition,"Another ledger").await;
    let foreign_event=Uuid::new_v4();actor_state::record(&f.service,f.workspace,other,f.principal,foreign_event,StateInput{target_object_id:Some(foreign),..base.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(foreign_event),..base.clone()}).await,Err(WorkbenchError::Invisible)));
    let new_root=Uuid::new_v4();record(new_root,StateInput{judgment:StateJudgment::Contested,revises_event_id:Some(root),..base.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),10);
    for (event,input) in chain {assert_eq!(snapshot.actor_epistemic_states.iter().find(|e|e.event_id==event).unwrap().input,input);}
    assert_eq!(snapshot.actor_epistemic_states[1].input.target_state_event_id,Some(root));
    assert!(snapshot.assessments.is_empty() && snapshot.arguments.is_empty());
    let mut tx=f.service.begin().await.unwrap();sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$3,'BELIEF','UNKNOWN','context','time','rationale',$5)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(a).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();
}

#[tokio::test]
async fn actor_epistemic_orientations_distinguish_immediate_reactions_from_persistent_dispositions() {
    use epistemic_workbench::actor_state::{self,StateInput,StateDimension,StateJudgment,OrientationScope};
    let f=fixture().await;
    let sit=situation(&f,"Investigate curiosity and aversion").await;
    let target=node(&f,sit,ExpertNodeKind::Question,"What did the ledger show?").await;
    let actor=Uuid::new_v4();niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,"Interviewee").await.unwrap();
    let item=StateInput{subject_actor_id:actor,target_object_id:Some(target),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::Curiosity,judgment:StateJudgment::Indicated,context:"Interview".into(),temporal_scope:"When asked".into(),orientation_scope:Some(OrientationScope::ImmediateReaction),orientation_goal:Some("Clarify the ledger".into()),rationale:"Asked a follow-up question".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let curiosity=Uuid::new_v4();record(curiosity,item.clone()).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{orientation_scope:None,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{orientation_goal:None,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Belief,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{orientation_scope:Some(OrientationScope::PersistentDisposition),revises_event_id:Some(curiosity),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{orientation_goal:Some("Different goal".into()),revises_event_id:Some(curiosity),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let aversion=Uuid::new_v4();record(aversion,StateInput{dimension:StateDimension::Aversion,judgment:StateJudgment::Unknown,orientation_scope:Some(OrientationScope::PersistentDisposition),orientation_goal:Some("Avoid further documentation".into()),..item.clone()}).await.unwrap();
    let revision=Uuid::new_v4();record(revision,StateInput{judgment:StateJudgment::Contested,revises_event_id:Some(curiosity),..item.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),3);
    assert_eq!(snapshot.actor_epistemic_states[0].input,item);
    assert_eq!(snapshot.actor_epistemic_states[1].input.orientation_scope,Some(OrientationScope::PersistentDisposition));
    assert!(snapshot.question_decisions.is_empty() && snapshot.question_responses.is_empty() && snapshot.arguments.is_empty());
    let mut tx=f.service.begin().await.unwrap();sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'CURIOSITY','INDICATED','context','time','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(target).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();
}

#[tokio::test]
async fn actor_epistemic_act_records_occurrence_not_an_instruction_or_trait() {
    use axum::{body::Body,http::{Request,StatusCode,header}};
    use epistemic_workbench::actor_act::{self,ActInput,ActKind,ActCharacterization};
    use tower::ServiceExt;
    let f=fixture().await;let sit=situation(&f,"Document a reported delegation").await;
    let q=node(&f,sit,ExpertNodeKind::Question,"What did the ledger show?").await;
    let a=Uuid::new_v4();let b=Uuid::new_v4();
    for (id,label) in [(a,"Manager"),(b,"Analyst")]{niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let input=ActInput{subject_actor_id:a,target_object_id:q,act_kind:ActKind::Delegating,characterization:ActCharacterization::Reported,delegate_actor_id:Some(b),context:"Meeting".into(),temporal_scope:"Before hearing".into(),rationale:"Reported asking analyst to inspect the ledger".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_act::record(&f.service,f.workspace,sit,f.principal,id,input);
    let act=Uuid::new_v4();assert!(!record(act,input.clone()).await.unwrap().replayed);assert!(record(act,input.clone()).await.unwrap().replayed);
    assert!(matches!(record(act,ActInput{characterization:ActCharacterization::Contested,..input.clone()}).await,Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(),ActInput{act_kind:ActKind::Ignoring,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),ActInput{delegate_actor_id:Some(a),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),ActInput{characterization:ActCharacterization::Observed,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),ActInput{delegate_actor_id:Some(Uuid::new_v4()),..input.clone()}).await,Err(WorkbenchError::Invisible)));
    let other=situation(&f,"Unrelated investigation").await;let foreign=node(&f,other,ExpertNodeKind::Question,"Another question?").await;
    assert!(matches!(record(Uuid::new_v4(),ActInput{target_object_id:foreign,..input.clone()}).await,Err(WorkbenchError::Invisible)));
    let document=Uuid::new_v4();let imported=ingest_source(&f.service,f.workspace,document,SourceFormat::WhatsappExport,"fixture://epistemic-act",b"[02/11/2026, 12:00] Manager: Analyst, inspect the ledger\n").await.unwrap();
    let observed=ActInput{characterization:ActCharacterization::Observed,basis_evidence_id:Some(imported.fragment_ids[0]),revises_event_id:Some(act),..input.clone()};
    assert!(matches!(record(Uuid::new_v4(),observed.clone()).await,Err(WorkbenchError::Invisible)));
    situation_store::add_source(&f.service,f.workspace,f.principal,sit,document).await.unwrap();
    let revised=Uuid::new_v4();
    let admin=PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let writer=authz::provision_token(&admin,f.principal,f.workspace,true,true,1).await.unwrap();let reader=authz::provision_token(&admin,Uuid::new_v4(),f.workspace,true,false,1).await.unwrap();
    let secure=PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();let router=niccolo_evidence::api::router(secure);
    let path=format!("/v1/workspaces/{}/situations/{}/epistemic/actor-acts/{}",f.workspace,sit,revised);
    let request=|token:&str|Request::builder().method("POST").uri(&path).header(header::AUTHORIZATION,format!("Bearer {token}")).header(header::CONTENT_TYPE,"application/json").body(Body::from(serde_json::to_string(&observed).unwrap())).unwrap();
    assert_eq!(router.clone().oneshot(request(&reader)).await.unwrap().status(),StatusCode::FORBIDDEN);assert_eq!(router.clone().oneshot(request(&writer)).await.unwrap().status(),StatusCode::CREATED);assert_eq!(router.oneshot(request(&writer)).await.unwrap().status(),StatusCode::OK);
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();assert_eq!(snapshot.actor_epistemic_acts.len(),2);assert_eq!(snapshot.actor_epistemic_acts[0].input,input);assert_eq!(snapshot.actor_epistemic_acts[1].input,observed);assert_eq!(snapshot.actor_epistemic_acts[1].recorded_by,f.principal);
    assert!(snapshot.actor_epistemic_states.is_empty()&&snapshot.question_decisions.is_empty()&&snapshot.question_responses.is_empty()&&snapshot.arguments.is_empty());
    let mut tx=f.service.begin().await.unwrap();sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.actor_epistemic_act_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,act_kind,characterization,delegate_actor_id,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'IGNORING','REPORTED',$6,'context','time','rationale',$7)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(a).bind(q).bind(b).bind(f.principal).execute(&mut *tx).await.unwrap_err();assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();
}

#[tokio::test]
async fn directed_actor_trust_and_loyalty_remain_distinct_dimensions_not_permissions() {
    use epistemic_workbench::actor_state::{self,StateInput,StateDimension,StateJudgment};
    let f=fixture().await;let sit=situation(&f,"Qualify trust and conflicting loyalties").await;
    let cause=node(&f,sit,ExpertNodeKind::Proposition,"Protect whistleblower confidentiality").await;
    let a=Uuid::new_v4();let b=Uuid::new_v4();let c=Uuid::new_v4();
    for (id,label) in [(a,"Investigator"),(b,"Analyst"),(c,"Institution")]{niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let input=StateInput{subject_actor_id:a,target_object_id:None,target_state_event_id:None,target_actor_id:Some(b),counterparty_actor_id:None,influence_target:None,dimension:StateDimension::TrustCompetence,judgment:StateJudgment::Indicated,context:"Technical inquiry".into(),temporal_scope:"Current investigation".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported trust in technical skill".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let competence=Uuid::new_v4();record(competence,input.clone()).await.unwrap();
    let confidentiality=Uuid::new_v4();record(confidentiality,StateInput{dimension:StateDimension::TrustConfidentiality,judgment:StateJudgment::DistrustIndicated,..input.clone()}).await.unwrap();
    let truth=Uuid::new_v4();record(truth,StateInput{dimension:StateDimension::TrustTruthfulness,judgment:StateJudgment::Unknown,..input.clone()}).await.unwrap();
    let loyalty=Uuid::new_v4();record(loyalty,StateInput{dimension:StateDimension::Loyalty,target_actor_id:Some(c),..input.clone()}).await.unwrap();
    let principle=Uuid::new_v4();record(principle,StateInput{dimension:StateDimension::Loyalty,target_actor_id:None,target_object_id:Some(cause),..input.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_actor_id:None,target_object_id:Some(cause),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Belief,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Loyalty,judgment:StateJudgment::DistrustIndicated,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_actor_id:Some(Uuid::new_v4()),..input.clone()}).await,Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_actor_id:Some(c),revises_event_id:Some(competence),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();assert_eq!(snapshot.actor_epistemic_states.len(),5);
    assert_eq!(snapshot.actor_epistemic_states[0].input,input);
    assert_eq!(snapshot.actor_epistemic_states[1].input.judgment,StateJudgment::DistrustIndicated);
    assert!(snapshot.actor_epistemic_states.iter().all(|e|e.input.subject_actor_id==a));
    assert!(snapshot.assessments.is_empty()&&snapshot.arguments.is_empty()&&snapshot.question_decisions.is_empty());
    let mut tx=f.service.begin().await.unwrap();sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'TRUST_COMPETENCE','INDICATED','context','time','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(a).bind(cause).bind(f.principal).execute(&mut *tx).await.unwrap_err();assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();
}

#[tokio::test]
async fn modeled_secrecy_does_not_establish_knowledge_or_change_access() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Review compartmentalization").await;
    let other_sit=situation(&f,"Unrelated situation").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"A confidential account exists").await;
    let actor=Uuid::new_v4();let other=Uuid::new_v4();
    for (id,label) in [(actor,"Excluded audience"),(other,"Observer")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:actor,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::ModeledSecrecyExistence,judgment:StateJudgment::Indicated,context:"Investigation".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported withholding expectation".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let prior=Uuid::new_v4();record(prior,item.clone()).await.unwrap();
    for dimension in [StateDimension::ModeledSecrecyContent,StateDimension::ModeledSecrecySourceIdentity,StateDimension::ModeledSecrecyOrigin,StateDimension::ModeledSecrecyRationale,StateDimension::ModeledSecrecyConclusions,StateDimension::ModeledSecrecyPartialContent] {
        record(Uuid::new_v4(),StateInput{dimension,judgment:StateJudgment::Contested,..item.clone()}).await.unwrap();
    }
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Access,revises_event_id:Some(prior),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(other),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(prior),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),7);
    assert!(snapshot.actor_epistemic_states.iter().all(|e|e.input.dimension.is_modeled_secrecy_facet()));
    assert!(snapshot.nodes.iter().any(|n|n.object_id==issue));
    assert!(epistemic_workbench::snapshot(&f.service,f.workspace,other_sit).await.unwrap().actor_epistemic_states.is_empty());
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::Access,..item.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),8);
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'MODELED_SECRECY_EXISTENCE','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(prior).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn possible_internal_mechanisms_do_not_establish_self_deception() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Examine competing explanations for a report").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"The investigator overlooked a contradiction").await;
    let actor=Uuid::new_v4();let other=Uuid::new_v4();
    for (id,label) in [(actor,"Investigator"),(other,"Witness")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:actor,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::Rationalization,judgment:StateJudgment::Contested,context:"Review".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Time pressure remains an alternative explanation".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let prior=Uuid::new_v4();record(prior,item.clone()).await.unwrap();
    for dimension in [StateDimension::SelectiveAttention,StateDimension::MotivatedInterpretation,StateDimension::ConflictingEvidenceAvoidance] {
        record(Uuid::new_v4(),StateInput{dimension,..item.clone()}).await.unwrap();
    }
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::SelfDeception,revises_event_id:Some(prior),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(other),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(prior),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),4);
    assert!(!snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::SelfDeception));
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::SelfDeception,judgment:StateJudgment::Unknown,..item.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),5);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::SelfDeception&&e.input.judgment==StateJudgment::Unknown));
    assert!(snapshot.actor_epistemic_acts.is_empty()&&snapshot.question_decisions.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'SELF_DECEPTION','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(prior).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn deception_attempt_can_fail_for_target_and_affect_other_audience() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Review possible misdirection of investigators").await;
    let issue=node(&f,sit,ExpertNodeKind::Question,"Was the ledger account misleading?").await;
    let narrator=Uuid::new_v4();let intended=Uuid::new_v4();let observer=Uuid::new_v4();
    for (id,label) in [(narrator,"Narrator"),(intended,"Investigator"),(observer,"Observer")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:narrator,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::DeceptionAttempt,judgment:StateJudgment::Indicated,context:"Account review".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported attempt to misdirect review".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    record(Uuid::new_v4(),item.clone()).await.unwrap();
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::DeceptionIntention,judgment:StateJudgment::Contested,..item.clone()}).await.unwrap();
    for dimension in [StateDimension::DeceptionIntendedTarget,StateDimension::DeceptionIntendedEffect,StateDimension::DeceptionActualEffect] {
        assert!(matches!(record(Uuid::new_v4(),StateInput{dimension,..item.clone()}).await,Err(WorkbenchError::Invalid)));
        record(Uuid::new_v4(),StateInput{dimension,counterparty_actor_id:Some(intended),judgment:if dimension==StateDimension::DeceptionActualEffect{StateJudgment::NotIndicated}else{StateJudgment::Indicated},..item.clone()}).await.unwrap();
    }
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::DeceptionActualEffect,counterparty_actor_id:Some(observer),rationale:"Observer reportedly accepted the account".into(),..item.clone()}).await.unwrap();
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::DeceptionPerceivedSuccess,judgment:StateJudgment::Unknown,..item.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),7);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::DeceptionActualEffect&&e.input.counterparty_actor_id==Some(intended)&&e.input.judgment==StateJudgment::NotIndicated));
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::DeceptionActualEffect&&e.input.counterparty_actor_id==Some(observer)&&e.input.judgment==StateJudgment::Indicated));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'DECEPTION_ACTUAL_EFFECT','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(narrator).bind(issue).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn narrative_means_do_not_establish_deception_or_effect() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Review how an account was composed").await;
    let issue=node(&f,sit,ExpertNodeKind::Question,"Which ledger entries were included?").await;
    let speaker=Uuid::new_v4();let audience=Uuid::new_v4();
    for (id,label) in [(speaker,"Narrator"),(audience,"Listener")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:speaker,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::NarrativeOmission,judgment:StateJudgment::Contested,context:"Account review".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Entries may be omitted due to time pressure".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    for dimension in [StateDimension::NarrativeFalseStatement,StateDimension::NarrativeSelection,StateDimension::NarrativeOmission,StateDimension::NarrativeFraming,StateDimension::NarrativeOrdering,StateDimension::NarrativeTiming,StateDimension::NarrativeAggregation,StateDimension::NarrativeDecontextualization] {
        let id=Uuid::new_v4();record(id,StateInput{dimension,..item.clone()}).await.unwrap();
        assert!(matches!(record(Uuid::new_v4(),StateInput{dimension,target_object_id:None,target_state_event_id:Some(id),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    }
    record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(audience),..item.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),9);
    assert!(snapshot.actor_epistemic_states.iter().all(|e|e.input.judgment==StateJudgment::Contested));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_actor_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'NARRATIVE_OMISSION','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(speaker).bind(audience).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn power_and_authority_do_not_imply_directed_asymmetry() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Who controls access to the ledger?").await;
    let issue=node(&f,sit,ExpertNodeKind::Question,"Who sees the original ledger?").await;
    let subject=Uuid::new_v4();let other=Uuid::new_v4();
    for (id,label) in [(subject,"Analyst"),(other,"Manager")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:subject,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::EpistemicPower,judgment:StateJudgment::Indicated,context:"Ledger review".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported control of access".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    record(Uuid::new_v4(),item.clone()).await.unwrap();
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::FormalAuthority,judgment:StateJudgment::NotIndicated,..item.clone()}).await.unwrap();
    for dimension in [StateDimension::AsymmetryInformation,StateDimension::AsymmetryAccess,StateDimension::AsymmetryUnderstanding,StateDimension::AsymmetryMetaKnowledge,StateDimension::AsymmetryInfluence] {
        let directed=StateInput{dimension,counterparty_actor_id:Some(other),..item.clone()};
        let id=Uuid::new_v4();record(id,directed.clone()).await.unwrap();
        assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:None,..directed.clone()}).await,Err(WorkbenchError::Invalid)));
        assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(subject),..directed.clone()}).await,Err(WorkbenchError::Invalid)));
        assert!(matches!(record(Uuid::new_v4(),StateInput{subject_actor_id:other,counterparty_actor_id:Some(subject),revises_event_id:Some(id),..directed}).await,Err(WorkbenchError::Invalid)));
    }
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),7);
    assert!(snapshot.actor_epistemic_states.iter().all(|e|e.input.subject_actor_id==subject));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'ASYMMETRY_ACCESS','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(subject).bind(issue).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn epistemic_influence_separates_facets_and_exact_targets() {
    use epistemic_workbench::actor_state::{self,InfluenceTarget,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Examine influence on investigation priorities").await;
    let issue=node(&f,sit,ExpertNodeKind::Question,"Should the investigator inspect the payroll ledger?").await;
    let affected=Uuid::new_v4();let other=Uuid::new_v4();
    for (id,label) in [(affected,"Investigator"),(other,"Analyst")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:affected,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:Some(other),influence_target:Some(InfluenceTarget::InvestigationPriority),dimension:StateDimension::InfluenceAct,judgment:StateJudgment::Indicated,context:"Payroll investigation".into(),temporal_scope:"Before hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported request to examine different records".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let act=Uuid::new_v4();record(act,item.clone()).await.unwrap();
    for (dimension,judgment,counterparty_actor_id) in [(StateDimension::InfluenceCapacity,StateJudgment::Contested,Some(other)),(StateDimension::InfluenceIntendedEffect,StateJudgment::Unknown,Some(other)),(StateDimension::InfluenceActualEffect,StateJudgment::NotIndicated,None),(StateDimension::InfluencePerceivedSuccess,StateJudgment::Unknown,Some(other))] {record(Uuid::new_v4(),StateInput{dimension,judgment,counterparty_actor_id,..item.clone()}).await.unwrap();}
    record(Uuid::new_v4(),StateInput{influence_target:Some(InfluenceTarget::Attention),..item.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{influence_target:None,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:None,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{influence_target:Some(InfluenceTarget::Attention),revises_event_id:Some(act),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),6);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::InfluenceActualEffect&&e.input.judgment==StateJudgment::NotIndicated));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,counterparty_actor_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,'INFLUENCE_ACT','INDICATED','context','scope','rationale',$7)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(affected).bind(issue).bind(other).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn vulnerability_qualifications_do_not_establish_compromise_or_response() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Examine exposure of a resource").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"Centralized ledger access could enable tampering").await;
    let a=Uuid::new_v4();let b=Uuid::new_v4();
    for (id,label) in [(a,"Custodian"),(b,"Administrator")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:a,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::VulnerabilityPotential,judgment:StateJudgment::Indicated,context:"Centralized ledger administration".into(),temporal_scope:"Before audit".into(),orientation_scope:None,orientation_goal:None,rationale:"Potential loss of ledger integrity".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let prior=Uuid::new_v4();record(prior,item.clone()).await.unwrap();
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::VulnerabilityActual,judgment:StateJudgment::Unknown,..item.clone()}).await.unwrap();
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::VulnerabilityPerceived,counterparty_actor_id:Some(b),..item.clone()}).await.unwrap();
    record(Uuid::new_v4(),StateInput{dimension:StateDimension::VulnerabilityMisattributed,judgment:StateJudgment::Contested,..item.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::VulnerabilityActual,revises_event_id:Some(prior),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(prior),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(a),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),4);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::VulnerabilityActual&&e.input.judgment==StateJudgment::Unknown));
    assert!(snapshot.assessments.is_empty()&&snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty()&&snapshot.arguments.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'VULNERABILITY_POTENTIAL','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(a).bind(prior).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn vulnerability_and_compliance_do_not_establish_consent_or_loyalty() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Examine response under dependence").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"The employee accepted a change to the bonus").await;
    let affected=Uuid::new_v4();let other=Uuid::new_v4();
    for (id,label) in [(affected,"Employee"),(other,"Manager")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:affected,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:Some(other),influence_target:None,dimension:StateDimension::Compliance,judgment:StateJudgment::Indicated,context:"Bonus review".into(),temporal_scope:"At decision".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported acceptance of the change".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let compliance=Uuid::new_v4();record(compliance,item.clone()).await.unwrap();
    for (dimension,judgment,counterparty_actor_id) in [(StateDimension::Consent,StateJudgment::NotIndicated,Some(other)),(StateDimension::Submission,StateJudgment::Contested,Some(other)),(StateDimension::Vulnerability,StateJudgment::Indicated,None),(StateDimension::CoercionSusceptibility,StateJudgment::Unknown,Some(other))] {record(Uuid::new_v4(),StateInput{dimension,judgment,counterparty_actor_id,..item.clone()}).await.unwrap();}
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Consent,revises_event_id:Some(compliance),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Vulnerability,target_object_id:None,target_state_event_id:Some(compliance),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),5);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::Consent&&e.input.judgment==StateJudgment::NotIndicated));
    assert!(!snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::Loyalty));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'VULNERABILITY','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(affected).bind(compliance).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn threat_capability_intent_perceived_credibility_and_outcome_are_independent() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Qualify a reported threat").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"The employee was warned about a bonus").await;
    let affected=Uuid::new_v4();let issuer=Uuid::new_v4();
    for (id,label) in [(affected,"Employee"),(issuer,"Manager")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:affected,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:Some(issuer),influence_target:None,dimension:StateDimension::ThreatCapability,judgment:StateJudgment::Contested,context:"Bonus review".into(),temporal_scope:"Before decision".into(),orientation_scope:None,orientation_goal:None,rationale:"Unclear whether manager controls the bonus".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let capability=Uuid::new_v4();record(capability,item.clone()).await.unwrap();
    for (dimension,judgment,counterparty_actor_id) in [(StateDimension::CommunicatedThreat,StateJudgment::Indicated,Some(issuer)),(StateDimension::ThreatIntent,StateJudgment::Unknown,Some(issuer)),(StateDimension::ThreatPerceivedCredibility,StateJudgment::Indicated,None),(StateDimension::ThreatOutcome,StateJudgment::NotIndicated,None)] {record(Uuid::new_v4(),StateInput{dimension,judgment,counterparty_actor_id,..item.clone()}).await.unwrap();}
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:None,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::ThreatIntent,counterparty_actor_id:None,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::ThreatIntent,revises_event_id:Some(capability),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),5);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::ThreatIntent&&e.input.judgment==StateJudgment::Unknown));
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::ThreatOutcome&&e.input.judgment==StateJudgment::NotIndicated));
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'THREAT_INTENT','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(affected).bind(issue).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn contextual_pressure_counterparty_is_optional_exact_and_workspace_scoped() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Attribute a possible source of pressure").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"The employee was warned about a bonus").await;
    let affected=Uuid::new_v4();let source=Uuid::new_v4();let alternative=Uuid::new_v4();
    for (id,label) in [(affected,"Employee"),(source,"Manager"),(alternative,"Director")] {niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,id,label).await.unwrap();}
    let item=StateInput{subject_actor_id:affected,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:Some(source),influence_target:None,dimension:StateDimension::Pressure,judgment:StateJudgment::Contested,context:"Bonus review".into(),temporal_scope:"Before decision".into(),orientation_scope:None,orientation_goal:None,rationale:"Possible manager involvement".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let first=Uuid::new_v4();record(first,item.clone()).await.unwrap();
    assert!(record(first,item.clone()).await.unwrap().replayed);
    assert!(matches!(record(first,StateInput{counterparty_actor_id:Some(alternative),..item.clone()}).await,Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(alternative),revises_event_id:Some(first),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(affected),..item.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{counterparty_actor_id:Some(Uuid::new_v4()),..item.clone()}).await,Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Belief,..item.clone()}).await,Err(WorkbenchError::Invalid)));
    let unknown=Uuid::new_v4();record(unknown,StateInput{counterparty_actor_id:None,judgment:StateJudgment::Unknown,..item.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),2);
    assert_eq!(snapshot.actor_epistemic_states.iter().find(|e|e.event_id==first).unwrap().input.counterparty_actor_id,Some(source));
    assert_eq!(snapshot.actor_epistemic_states.iter().find(|e|e.event_id==unknown).unwrap().input.counterparty_actor_id,None);
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_object_id,counterparty_actor_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id,revises_event_id) VALUES($1,$2,$3,$4,$5,$6,'PRESSURE','INDICATED','Bonus review','Before decision','changed counterparty',$7,$8)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(affected).bind(issue).bind(alternative).bind(f.principal).bind(first).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn contextual_pressure_does_not_establish_coercion_or_consent() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Distinguish incentives, threats and coercion").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"The employee was warned about losing a bonus").await;
    let actor=Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,"Employee").await.unwrap();
    let input=StateInput{subject_actor_id:actor,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::CommunicatedThreat,judgment:StateJudgment::Indicated,context:"Bonus review".into(),temporal_scope:"Before decision".into(),orientation_scope:None,orientation_goal:None,rationale:"Reported warning of adverse outcome".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let threat=Uuid::new_v4();record(threat,input.clone()).await.unwrap();
    for (dimension,judgment) in [(StateDimension::Incentive,StateJudgment::Indicated),(StateDimension::Pressure,StateJudgment::Contested),(StateDimension::AdverseCondition,StateJudgment::Unknown),(StateDimension::Coercion,StateJudgment::Unknown),(StateDimension::Constraint,StateJudgment::Indicated),(StateDimension::Dependency,StateJudgment::Contested),(StateDimension::Leverage,StateJudgment::Unknown)] {
        record(Uuid::new_v4(),StateInput{dimension,judgment,..input.clone()}).await.unwrap();
    }
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::Coercion,revises_event_id:Some(threat),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(threat),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),8);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::Coercion&&e.input.judgment==StateJudgment::Unknown));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'COERCION','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(threat).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn contextual_autonomy_is_independent_of_consent_and_system_permissions() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Examine autonomy under constraints").await;
    let issue=node(&f,sit,ExpertNodeKind::Question,"Can the employee challenge the decision?").await;
    let actor=Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,"Employee").await.unwrap();
    let input=StateInput{subject_actor_id:actor,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::AutonomyDecide,judgment:StateJudgment::Indicated,context:"Employment decision".into(),temporal_scope:"At hearing".into(),orientation_scope:None,orientation_goal:None,rationale:"Employee can choose between options".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let decide=Uuid::new_v4();record(decide,input.clone()).await.unwrap();
    for (dimension,judgment) in [(StateDimension::AutonomyRefuse,StateJudgment::NotIndicated),(StateDimension::AutonomyDisagree,StateJudgment::Contested),(StateDimension::AutonomyExit,StateJudgment::Unknown),(StateDimension::AutonomySeekReview,StateJudgment::Indicated),(StateDimension::AutonomyAcquireInformation,StateJudgment::Unknown),(StateDimension::AutonomyReviseCommitments,StateJudgment::NotIndicated)] {
        record(Uuid::new_v4(),StateInput{dimension,judgment,..input.clone()}).await.unwrap();
    }
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::AutonomyRefuse,revises_event_id:Some(decide),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(decide),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),7);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.input.dimension==StateDimension::AutonomyRefuse&&e.input.judgment==StateJudgment::NotIndicated));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_acts.is_empty());
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'AUTONOMY_REFUSE','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(decide).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn protective_motives_are_distinct_attributed_hypotheses_not_diagnoses() {
    use epistemic_workbench::actor_state::{self,StateDimension,StateInput,StateJudgment};
    let f=fixture().await;
    let sit=situation(&f,"Examine alleged motives").await;
    let issue=node(&f,sit,ExpertNodeKind::Hypothesis,"The analyst avoided disclosing a mistake").await;
    let actor=Uuid::new_v4();
    niccolo_evidence::actors::create(&f.service,f.workspace,f.principal,actor,"Analyst").await.unwrap();
    let input=StateInput{subject_actor_id:actor,target_object_id:Some(issue),target_state_event_id:None,target_actor_id:None,counterparty_actor_id:None,influence_target:None,dimension:StateDimension::EgoProtection,judgment:StateJudgment::Contested,context:"Review of disclosure".into(),temporal_scope:"At review".into(),orientation_scope:None,orientation_goal:None,rationale:"Possible avoidance of embarrassment".into(),basis_evidence_id:None,revises_event_id:None};
    let record=|id,input|actor_state::record(&f.service,f.workspace,sit,f.principal,id,input);
    let ego=Uuid::new_v4();
    record(ego,input.clone()).await.unwrap();
    let identity=Uuid::new_v4();
    record(identity,StateInput{dimension:StateDimension::IdentityProtection,judgment:StateJudgment::Unknown,rationale:"Professional group identity was not established".into(),..input.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),StateInput{dimension:StateDimension::IdentityProtection,revises_event_id:Some(ego),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),StateInput{target_object_id:None,target_state_event_id:Some(ego),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.actor_epistemic_states.len(),2);
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.event_id==ego&&e.input.dimension==StateDimension::EgoProtection));
    assert!(snapshot.actor_epistemic_states.iter().any(|e|e.event_id==identity&&e.input.dimension==StateDimension::IdentityProtection));
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.actor_epistemic_state_events(workspace_id,situation_id,event_id,subject_actor_id,target_state_event_id,dimension,judgment,context,temporal_scope,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'EGO_PROTECTION','INDICATED','context','scope','rationale',$6)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(actor).bind(ego).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn inquiry_decision_records_exact_considered_judgments_without_execution() {
    use axum::{body::Body,http::{Request,StatusCode,header}};
    use epistemic_workbench::{assessment::{AssessmentInput,AssessmentTarget,AssessmentDimension,AssessmentResult,AssessmentConfidence},inquiry_decision::{InquiryDecisionInput,InquiryDisposition}};
    use tower::ServiceExt;
    let f=fixture().await;
    let sit=situation(&f,"Select an investigative question").await;
    let a=node(&f,sit,ExpertNodeKind::Question,"Inspect the audit log?").await;
    let b=node(&f,sit,ExpertNodeKind::Question,"Interview the employee?").await;
    let c=node(&f,sit,ExpertNodeKind::Question,"Inspect another source?").await;
    let other=situation(&f,"Another investigation").await;
    let foreign=node(&f,other,ExpertNodeKind::Question,"Foreign question?").await;
    let cost=AssessmentInput {target:AssessmentTarget::Object {object_id:b},dimension:AssessmentDimension::AcquisitionCost,result:AssessmentResult::HighCost,confidence:AssessmentConfidence::Low,criterion:"Time".into(),context:"Current inquiry".into(),rationale:"Interview would require travel".into(),grounds:vec![],revises_event_id:None};
    let old=Uuid::new_v4();
    epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,old,cost.clone()).await.unwrap();
    let input=InquiryDecisionInput {question_id:a,considered_question_ids:vec![a,b],assessment_event_ids:vec![old],disposition:InquiryDisposition::PursueNext,rationale:"Prefer the less intrusive inquiry".into()};
    let id=Uuid::new_v4();
    let record=|id,input| epistemic_workbench::inquiry_decision::record(&f.service,f.workspace,sit,f.principal,id,input);
    assert!(!record(id,input.clone()).await.unwrap().replayed);
    assert!(record(id,input.clone()).await.unwrap().replayed);
    assert!(matches!(record(id,InquiryDecisionInput {disposition:InquiryDisposition::Defer,..input.clone()}).await,Err(WorkbenchError::Conflict)));
    assert!(matches!(record(Uuid::new_v4(),InquiryDecisionInput {considered_question_ids:vec![a,a],..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),InquiryDecisionInput {considered_question_ids:vec![a,foreign],..input.clone()}).await,Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(),InquiryDecisionInput {considered_question_ids:vec![a,c],..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),InquiryDecisionInput {assessment_event_ids:vec![Uuid::new_v4()],..input.clone()}).await,Err(WorkbenchError::Invisible)));
    let new=Uuid::new_v4();
    epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,new,AssessmentInput {result:AssessmentResult::LowCost,revises_event_id:Some(old),..cost}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.question_decisions.len(),1);
    assert_eq!(snapshot.question_decisions[0].input,input);
    assert!(snapshot.question_responses.is_empty() && snapshot.arguments.is_empty());
    use epistemic_workbench::inquiry_decision_review::{self,DecisionBasisReviewInput,DecisionBasisDisposition};
    assert_eq!(snapshot.question_decision_basis_changes.len(),1);
    let change=&snapshot.question_decision_basis_changes[0];
    assert_eq!((change.decision_id,change.original_assessment_event_id,change.current_assessment_event_id),(id,old,new));
    assert!(change.requires_review);
    let review=DecisionBasisReviewInput {original_assessment_event_id:old,reviewed_assessment_event_id:new,disposition:DecisionBasisDisposition::Retained,note:"The lower cost does not alter the privacy rationale".into()};
    let review_id=Uuid::new_v4();
    let review_record=|event,input| inquiry_decision_review::record(&f.service,f.workspace,sit,f.principal,id,event,input);
    assert!(!review_record(review_id,review.clone()).await.unwrap().replayed);
    assert!(review_record(review_id,review.clone()).await.unwrap().replayed);
    assert!(matches!(review_record(Uuid::new_v4(),DecisionBasisReviewInput{reviewed_assessment_event_id:Uuid::new_v4(),..review.clone()}).await,Err(WorkbenchError::Conflict)));
    let after=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert!(!after.question_decision_basis_changes[0].requires_review);
    assert_eq!(after.question_decisions[0].input,input);
    let newest=Uuid::new_v4();
    let latest=after.assessments.iter().find(|a|a.event_id==new).unwrap().input.clone();
    epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,newest,AssessmentInput {result:AssessmentResult::HighCost,revises_event_id:Some(new),..latest}).await.unwrap();
    let after=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    let change=&after.question_decision_basis_changes[0];
    assert_eq!(change.current_assessment_event_id,newest);
    assert!(change.requires_review && change.latest_review.is_none());
    assert_eq!(change.review_history.len(),1);
    assert!(review_record(review_id,review.clone()).await.unwrap().replayed);
    assert!(matches!(review_record(Uuid::new_v4(),review.clone()).await,Err(WorkbenchError::Conflict)));
    assert!(record(id,input.clone()).await.unwrap().replayed);
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.epistemic_question_decisions(workspace_id,situation_id,decision_id,question_id,considered_question_ids,assessment_event_ids,disposition,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,'PURSUE_NEXT','invalid basis',$7)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(a).bind(vec![a,c]).bind(vec![old]).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
    let admin=PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap()).await.unwrap();
    let writer=authz::provision_token(&admin,f.principal,f.workspace,true,true,1).await.unwrap();
    let reader=authz::provision_token(&admin,Uuid::new_v4(),f.workspace,true,false,1).await.unwrap();
    let secure=PgPoolOptions::new().connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap()).await.unwrap();
    let router=niccolo_evidence::api::router(secure);
    let path=format!("/v1/workspaces/{}/situations/{}/epistemic/question-decisions/{}",f.workspace,sit,Uuid::new_v4());
    let request=|token:&str| Request::builder().method("POST").uri(&path).header(header::AUTHORIZATION,format!("Bearer {token}")).header(header::CONTENT_TYPE,"application/json").body(Body::from(serde_json::to_string(&input).unwrap())).unwrap();
    assert_eq!(router.clone().oneshot(request(&reader)).await.unwrap().status(),StatusCode::FORBIDDEN);
    assert_eq!(router.clone().oneshot(request(&writer)).await.unwrap().status(),StatusCode::CREATED);
    assert_eq!(router.oneshot(request(&writer)).await.unwrap().status(),StatusCode::OK);
}

#[tokio::test]
async fn investigative_selection_criteria_are_attributed_and_question_scoped() {
    use epistemic_workbench::assessment::*;
    let f = fixture().await;
    let sit = situation(&f, "Choose an inquiry without granting authority").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Can we inspect the ledger?").await;
    let p = node(&f, sit, ExpertNodeKind::Proposition, "The ledger exists").await;
    let other = situation(&f, "Other inquiry").await;
    let foreign = node(&f, other, ExpertNodeKind::Question, "Other question?").await;
    let criteria = [
        (AssessmentDimension::AcquisitionCost, AssessmentResult::LowCost),
        (AssessmentDimension::Accessibility, AssessmentResult::PartiallyAccessible),
        (AssessmentDimension::ContaminationRisk, AssessmentResult::HighRisk),
        (AssessmentDimension::Urgency, AssessmentResult::Deferrable),
        (AssessmentDimension::AuthorizationStatus, AssessmentResult::AuthorizationReported),
        (AssessmentDimension::InvestigationCoverage, AssessmentResult::Unexamined),
    ];
    for (dimension, result) in criteria {
        let input = AssessmentInput { target: AssessmentTarget::Object {object_id:q}, dimension, result,
            confidence: AssessmentConfidence::Low, criterion:"Current selection decision".into(), context:"Available ledger".into(),
            rationale:"Attributed judgment, not verified permission".into(), grounds:vec![], revises_event_id:None };
        let record = |input| epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,Uuid::new_v4(),input);
        assert_eq!(record(input.clone()).await.unwrap().input, input);
        assert!(matches!(record(AssessmentInput {target:AssessmentTarget::Object {object_id:p}, ..input.clone()}).await, Err(WorkbenchError::Invalid)));
        assert!(matches!(record(AssessmentInput {target:AssessmentTarget::Object {object_id:foreign}, ..input.clone()}).await, Err(WorkbenchError::Invisible)));
        assert!(matches!(record(AssessmentInput {result:AssessmentResult::Credible, ..input}).await, Err(WorkbenchError::Invalid)));
    }
    let snapshot = epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.assessments.len(), criteria.len());
    let inquiry = snapshot.question_inquiries.iter().find(|i| i.question_object_id == q).unwrap();
    assert_eq!(inquiry.selection_criteria.len(), criteria.len());
    for criterion in &inquiry.selection_criteria {
        assert_eq!(criterion.current_events.len(), 1);
        assert!(snapshot.assessments.iter().any(|a| a.event_id == criterion.current_events[0] && a.input.dimension == criterion.dimension));
    }
    assert!(snapshot.arguments.is_empty());
    assert!(snapshot.question_responses.is_empty());
    let initial = snapshot.assessments.iter().find(|a| a.input.dimension == AssessmentDimension::Urgency).unwrap();
    let revision = Uuid::new_v4();
    let competing = Uuid::new_v4();
    for (id,result,revises_event_id) in [(revision,AssessmentResult::Urgent,Some(initial.event_id)),(competing,AssessmentResult::Deferrable,None)] {
        epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,id,AssessmentInput {result,revises_event_id,..initial.input.clone()}).await.unwrap();
    }
    let after = epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    let urgency = after.question_inquiries.iter().find(|i| i.question_object_id == q).unwrap().selection_criteria.iter().find(|c| c.dimension == AssessmentDimension::Urgency).unwrap();
    assert_eq!(urgency.current_events.len(),2);
    assert!(urgency.current_events.contains(&revision) && urgency.current_events.contains(&competing));
    assert!(!urgency.current_events.contains(&initial.event_id));
    let mut tx = f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid = sqlx::query("INSERT INTO niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id,target_object_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,'URGENCY','URGENT','LOW','deadline','context','rationale',$5)")
        .bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(p).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(), Some("23514"));
    tx.rollback().await.unwrap();
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
    assert_eq!(inquiry.selection_criteria.len(), 6);
    assert!(inquiry.selection_criteria.iter().all(|c| c.current_events.is_empty()));
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
        interpretation_dependency::{
            self, InterpretationDependencyInput, InterpretationReviewInput,
        },
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
    let review_input = InterpretationReviewInput {
        argument_id: routes[0].0,
        grounding_set_id: routes[0].1,
        reviewed_revision_id: revision,
        reviewed_confirmation_event_id: Some(event),
        disposition: ReassessmentDisposition::Retained,
        note: "This route still has adequate partial support".into(),
    };
    let review_id = Uuid::new_v4();
    let review = |event, input| {
        interpretation_dependency::review(
            &f.service,
            f.workspace,
            sit,
            f.principal,
            id,
            event,
            input,
        )
    };
    assert!(matches!(
        review(
            Uuid::new_v4(),
            InterpretationReviewInput {
                argument_id: routes[1].0,
                grounding_set_id: routes[1].1,
                ..review_input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Invalid)
    ));
    assert!(matches!(
        review(
            Uuid::new_v4(),
            InterpretationReviewInput {
                reviewed_confirmation_event_id: None,
                ..review_input.clone()
            }
        )
        .await,
        Err(WorkbenchError::Conflict)
    ));
    assert!(
        !review(review_id, review_input.clone())
            .await
            .unwrap()
            .replayed
    );
    assert!(
        review(review_id, review_input.clone())
            .await
            .unwrap()
            .replayed
    );
    let retained = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let view = &retained.interpretation_dependencies[0];
    assert!(!view.requires_review && view.downstream_reviews[0].requires_review);
    assert_eq!(
        view.latest_review.as_ref().unwrap().input.disposition,
        ReassessmentDisposition::Retained
    );
    let downstream_review = InterpretationReviewInput {
        argument_id: routes[2].0,
        grounding_set_id: routes[2].1,
        disposition: ReassessmentDisposition::Deferred,
        note: "Inspect this consequence separately".into(),
        ..review_input.clone()
    };
    review(Uuid::new_v4(), downstream_review.clone())
        .await
        .unwrap();
    let deferred = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    let view = &deferred.interpretation_dependencies[0];
    assert!(view.downstream_reviews[0].requires_review);
    assert_eq!(
        view.downstream_reviews[0]
            .latest_review
            .as_ref()
            .unwrap()
            .input
            .disposition,
        ReassessmentDisposition::Deferred
    );
    review(
        Uuid::new_v4(),
        InterpretationReviewInput {
            disposition: ReassessmentDisposition::Retained,
            note: "Consequence reviewed".into(),
            ..downstream_review
        },
    )
    .await
    .unwrap();
    let all_retained = epistemic_workbench::snapshot(&f.service, f.workspace, sit)
        .await
        .unwrap();
    assert!(!all_retained.interpretation_dependencies[0].downstream_reviews[0].requires_review);
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
    assert!(after.interpretation_dependencies[0].requires_review);
    assert!(after.interpretation_dependencies[0].downstream_reviews[0].requires_review);
    assert!(after.interpretation_dependencies[0].latest_review.is_none());
    assert_eq!(after.interpretation_dependencies[0].review_history.len(), 1);
    assert_eq!(
        after.interpretation_dependencies[0].downstream_reviews[0]
            .review_history
            .len(),
        2
    );
    assert!(
        review(review_id, review_input.clone())
            .await
            .unwrap()
            .replayed
    );
    assert!(matches!(
        review(Uuid::new_v4(), review_input).await,
        Err(WorkbenchError::Conflict)
    ));
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
async fn low_value_objections_remain_representable_without_automatic_veto() {
    use epistemic_workbench::{assessment::*, inquiry_decision::{self, InquiryDecisionInput, InquiryDisposition}};
    let f = fixture().await;
    let sit = situation(&f, "AT-26: repeated provenance challenges near a deadline").await;
    let finding = node(&f, sit, ExpertNodeKind::Finding, "Ledger discrepancy warrants review").await;
    let objection = node(&f, sit, ExpertNodeKind::Hypothesis, "The discrepancy might be an alternative export format").await;
    let ground = node(&f, sit, ExpertNodeKind::Proposition, "The alternative format was previously examined").await;
    let q = node(&f, sit, ExpertNodeKind::Question, "Inspect the underlying ledger?").await;
    let argument_id = Uuid::new_v4();
    let grounding_set_id = Uuid::new_v4();
    epistemic_workbench::create_argument(&f.service, f.workspace, sit, f.principal, argument_id, finding, ArgumentMode::Abductive, "Challenge the finding").await.unwrap();
    epistemic_workbench::create_grounding_set(&f.service, f.workspace, sit, f.principal, argument_id, grounding_set_id, GroundingEffect::Challenges, GroundingAdequacy::Partial, "Alternative previously raised", &[NewGroundingMember {member_id: Uuid::new_v4(), role: GroundingMemberRole::Premise, ground_object_id: Some(objection), evidence_id: None, rationale: "Objection remains in the record".into()}]).await.unwrap();
    let before = epistemic_workbench::snapshot(&f.service, f.workspace, sit).await.unwrap();
    let input = AssessmentInput {target: AssessmentTarget::Route {argument_id, grounding_set_id}, dimension: AssessmentDimension::ObjectionNovelty, result: AssessmentResult::Duplicative, confidence: AssessmentConfidence::Moderate, criterion: "Whether the objection changes the already examined alternatives".into(), context: "Before collection deadline".into(), rationale: "Prior review is documented".into(), grounds: vec![AssessmentGround::Object {object_id: ground}], revises_event_id: None};
    let assess = |id, input| epistemic_workbench::assessment::record(&f.service, f.workspace, sit, f.principal, id, input);
    let first = Uuid::new_v4();
    assess(first, input.clone()).await.unwrap();
    for (dimension, result) in [(AssessmentDimension::ObjectionMateriality, AssessmentResult::LimitedMateriality), (AssessmentDimension::ObjectionReviewCost, AssessmentResult::HighCost), (AssessmentDimension::Relevance, AssessmentResult::LimitedRelevance), (AssessmentDimension::DiscriminatoryPower, AssessmentResult::LimitedPower), (AssessmentDimension::ObjectionAttentionDisposition, AssessmentResult::DeferReview)] {
        assess(Uuid::new_v4(), AssessmentInput{dimension, result, ..input.clone()}).await.unwrap();
    }
    // A competing judgment about priority stays independently visible.
    assess(Uuid::new_v4(), AssessmentInput{dimension: AssessmentDimension::ObjectionAttentionDisposition, result: AssessmentResult::ReviewNow, rationale: "Another criterion calls for immediate checking".into(), ..input.clone()}).await.unwrap();
    assert!(matches!(assess(Uuid::new_v4(), AssessmentInput{result: AssessmentResult::ReviewNow, ..input.clone()}).await, Err(WorkbenchError::Invalid)));
    // An explicit human inquiry decision can still proceed. No objection is erased.
    let decision = InquiryDecisionInput{question_id:q, considered_question_ids:vec![q], assessment_event_ids:vec![], disposition:InquiryDisposition::PursueNext, rationale:"Collect potentially decisive information before deadline".into()};
    inquiry_decision::record(&f.service,f.workspace,sit,f.principal,Uuid::new_v4(),decision.clone()).await.unwrap();
    let revised = Uuid::new_v4();
    assess(revised, AssessmentInput{result:AssessmentResult::PartiallyNovel,revises_event_id:Some(first),rationale:"A new aspect is now identified".into(),..input.clone()}).await.unwrap();
    let after = epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(after.nodes,before.nodes);
    assert_eq!(after.arguments,before.arguments);
    assert_eq!(after.grounding_sets,before.grounding_sets);
    assert_eq!(after.question_decisions[0].input,decision);
    assert_eq!(after.assessments.len(),8);
    assert!(after.assessments.iter().any(|a|a.event_id==first&&a.input.result==AssessmentResult::Duplicative));
    assert!(after.assessments.iter().any(|a|a.event_id==revised&&a.input.revises_event_id==Some(first)));
    assert!(after.assessment_dependencies.is_empty());
    // Direct storage must not retarget an objection judgment to a different object.
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id,revises_event_id,target_object_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'OBJECTION_NOVELTY','NOVEL','LOW',$6,$7,'Retargeted',$8)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(revised).bind(objection).bind(&input.criterion).bind(&input.context).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn pinned_integrity_decision_basis_preserves_concern_and_revision_without_veto() {
    use epistemic_workbench::{assessment::*,inquiry_decision::{self,InquiryDecisionInput,InquiryDisposition}};
    let f=fixture().await;
    let sit=situation(&f,"Compare questions with integrity concerns").await;
    let q=node(&f,sit,ExpertNodeKind::Question,"Inspect the ledger?").await;
    let other=node(&f,sit,ExpertNodeKind::Question,"Interview another person?").await;
    let input=AssessmentInput{target:AssessmentTarget::Object{object_id:q},dimension:AssessmentDimension::IntegrityEvidenceSelection,result:AssessmentResult::CompromiseIndicated,confidence:AssessmentConfidence::Low,criterion:"Sampling boundaries".into(),context:"Initial investigation".into(),rationale:"Possible exclusion of relevant entries".into(),grounds:vec![],revises_event_id:None};
    let old=Uuid::new_v4();
    let other_id=Uuid::new_v4();
    let assess=|id,input|epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,id,input);
    assess(old,input.clone()).await.unwrap();
    assess(other_id,AssessmentInput{target:AssessmentTarget::Object{object_id:other},..input.clone()}).await.unwrap();
    let decision=InquiryDecisionInput{question_id:q,considered_question_ids:vec![q],assessment_event_ids:vec![old],disposition:InquiryDisposition::PursueNext,rationale:"Inspect to clarify the concern".into()};
    let record=|id,input|inquiry_decision::record(&f.service,f.workspace,sit,f.principal,id,input);
    let decision_id=Uuid::new_v4();
    assert_eq!(record(decision_id,decision.clone()).await.unwrap().input,decision);
    assert!(matches!(record(Uuid::new_v4(),InquiryDecisionInput{assessment_event_ids:vec![other_id],..decision.clone()}).await,Err(WorkbenchError::Invalid)));
    let new=Uuid::new_v4();
    assess(new,AssessmentInput{result:AssessmentResult::Contested,revises_event_id:Some(old),..input.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.question_decisions[0].input,decision);
    assert_eq!(snapshot.question_decision_basis_changes.len(),1);
    let change=&snapshot.question_decision_basis_changes[0];
    assert_eq!((change.decision_id,change.original_assessment_event_id,change.current_assessment_event_id),(decision_id,old,new));
    assert!(change.requires_review);
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let invalid=sqlx::query("INSERT INTO niccolo.epistemic_question_decisions(workspace_id,situation_id,decision_id,question_id,considered_question_ids,assessment_event_ids,disposition,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,'PURSUE_NEXT','invalid basis',$7)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(q).bind(vec![q]).bind(vec![other_id]).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(invalid.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();
}

#[tokio::test]
async fn investigative_integrity_is_multidimensional_contestable_and_not_a_veto() {
    use epistemic_workbench::assessment::*;
    let f=fixture().await;
    let sit=situation(&f,"Review investigation quality").await;
    let other=situation(&f,"Separate investigation").await;
    let q=node(&f,sit,ExpertNodeKind::Question,"Was the omission intentional?").await;
    let ground=node(&f,sit,ExpertNodeKind::Proposition,"A competing explanation was recorded").await;
    let foreign=node(&f,other,ExpertNodeKind::Proposition,"Unrelated material").await;
    let input=AssessmentInput{target:AssessmentTarget::Object{object_id:q},dimension:AssessmentDimension::IntegrityQuestionFraming,result:AssessmentResult::ConcernIndicated,confidence:AssessmentConfidence::Low,criterion:"Open framing".into(),context:"Before interviews".into(),rationale:"Question may presuppose an intention".into(),grounds:vec![AssessmentGround::Object{object_id:ground}],revises_event_id:None};
    let record=|id,input|epistemic_workbench::assessment::record(&f.service,f.workspace,sit,f.principal,id,input);
    let framing=Uuid::new_v4();record(framing,input.clone()).await.unwrap();
    for dimension in [AssessmentDimension::IntegrityInvestigativeIndependence,AssessmentDimension::IntegrityEvidenceSelection,AssessmentDimension::IntegrityProvenance,AssessmentDimension::IntegrityHypothesisCoverage,AssessmentDimension::IntegrityReasoningQuality,AssessmentDimension::IntegrityCriteriaConsistency,AssessmentDimension::IntegrityDissentPreservation,AssessmentDimension::IntegrityHistoricalTraceability,AssessmentDimension::IntegrityReviewability] {
        record(Uuid::new_v4(),AssessmentInput{dimension,result:AssessmentResult::Unknown,..input.clone()}).await.unwrap();
    }
    record(Uuid::new_v4(),AssessmentInput{result:AssessmentResult::NoConcernIndicated,rationale:"Alternative assessment of wording".into(),..input.clone()}).await.unwrap();
    assert!(matches!(record(Uuid::new_v4(),AssessmentInput{result:AssessmentResult::Unreliable,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    assert!(matches!(record(Uuid::new_v4(),AssessmentInput{grounds:vec![AssessmentGround::Object{object_id:foreign}],..input.clone()}).await,Err(WorkbenchError::Invisible)));
    assert!(matches!(record(Uuid::new_v4(),AssessmentInput{dimension:AssessmentDimension::IntegrityReviewability,revises_event_id:Some(framing),..input.clone()}).await,Err(WorkbenchError::Invalid)));
    record(Uuid::new_v4(),AssessmentInput{target:AssessmentTarget::Assessment{assessment_event_id:framing},dimension:AssessmentDimension::IntegrityReviewability,result:AssessmentResult::Contested,..input.clone()}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.assessments.len(),12);
    assert!(snapshot.assessments.iter().any(|e|e.event_id==framing&&e.input.result==AssessmentResult::ConcernIndicated));
    assert!(snapshot.assessments.iter().any(|e|e.input.dimension==AssessmentDimension::IntegrityQuestionFraming&&e.input.result==AssessmentResult::NoConcernIndicated));
    assert!(snapshot.question_decisions.is_empty()&&snapshot.actor_epistemic_states.is_empty());
    use epistemic_workbench::inquiry_decision::{self,InquiryDecisionInput,InquiryDisposition};
    let decision=inquiry_decision::record(&f.service,f.workspace,sit,f.principal,Uuid::new_v4(),InquiryDecisionInput{question_id:q,considered_question_ids:vec![q],assessment_event_ids:vec![],disposition:InquiryDisposition::PursueNext,rationale:"Human elects to ask the question with the concern in mind".into()}).await.unwrap();
    assert_eq!(decision.input.disposition,InquiryDisposition::PursueNext);
    let mut tx=f.service.begin().await.unwrap();
    sqlx::query("SELECT set_config('niccolo.workspace_id',$1,true)").bind(f.workspace.to_string()).execute(&mut *tx).await.unwrap();
    let error=sqlx::query("INSERT INTO niccolo.epistemic_assessment_events(workspace_id,situation_id,event_id,target_object_id,dimension,result,confidence,criterion,context,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,'INTEGRITY_QUESTION_FRAMING','RELIABLE','LOW','criterion','context','rationale',$5)").bind(f.workspace).bind(sit).bind(Uuid::new_v4()).bind(q).bind(f.principal).execute(&mut *tx).await.unwrap_err();
    assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));tx.rollback().await.unwrap();
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
async fn compromise_containment_recovery_are_independent_and_selective() {
    use epistemic_workbench::assessment::*;
    let f=fixture().await;
    let sit=situation(&f,"Review a possibly compromised ledger").await;
    let premise=node(&f,sit,ExpertNodeKind::Proposition,"Reported ledger entry").await;
    let alternate=node(&f,sit,ExpertNodeKind::Proposition,"Independently reported receipt").await;
    let conclusion=node(&f,sit,ExpertNodeKind::Hypothesis,"Payment occurred").await;
    let mut routes=vec![];
    for ground in [premise,alternate]{
        let argument_id=Uuid::new_v4();let grounding_set_id=Uuid::new_v4();
        epistemic_workbench::create_argument(&f.service,f.workspace,sit,f.principal,argument_id,conclusion,ArgumentMode::Abductive,"Explicit route").await.unwrap();
        epistemic_workbench::create_grounding_set(&f.service,f.workspace,sit,f.principal,argument_id,grounding_set_id,GroundingEffect::Supports,GroundingAdequacy::Partial,"Explicit ground",&[NewGroundingMember{member_id:Uuid::new_v4(),role:GroundingMemberRole::Premise,ground_object_id:Some(ground),evidence_id:None,rationale:"Reported observation".into()}]).await.unwrap();
        routes.push((argument_id,grounding_set_id));
    }
    let before=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    let input=AssessmentInput{target:AssessmentTarget::Object{object_id:premise},dimension:AssessmentDimension::MaterialCompromise,result:AssessmentResult::ConcernIndicated,confidence:AssessmentConfidence::Low,criterion:"Ledger integrity".into(),context:"Current review".into(),rationale:"Possible missing entries".into(),grounds:vec![],revises_event_id:None};
    let assess=|id,item|record(&f.service,f.workspace,sit,f.principal,id,item);
    let compromise=Uuid::new_v4();let containment=Uuid::new_v4();let recovery=Uuid::new_v4();
    assess(compromise,input.clone()).await.unwrap();
    let containment_input=AssessmentInput{dimension:AssessmentDimension::RelianceContainment,result:AssessmentResult::PauseIndicated,..input.clone()};
    assess(containment,containment_input.clone()).await.unwrap();
    assess(recovery,AssessmentInput{dimension:AssessmentDimension::RecoveryReadiness,result:AssessmentResult::RecoveryNotReady,..input.clone()}).await.unwrap();
    assert!(matches!(assess(Uuid::new_v4(),AssessmentInput{dimension:AssessmentDimension::RecoveryReadiness,result:AssessmentResult::PauseIndicated,..input.clone()}).await,Err(WorkbenchError::Invalid)));
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.nodes,before.nodes);assert_eq!(snapshot.arguments,before.arguments);assert_eq!(snapshot.grounding_sets,before.grounding_sets);assert!(snapshot.assessment_dependencies.is_empty());
    let dep=Uuid::new_v4();
    record_dependency(&f.service,f.workspace,sit,f.principal,dep,DependencyInput{argument_id:routes[0].0,grounding_set_id:routes[0].1,assessment_event_id:containment,rationale:"Review reliance on the ledger".into()}).await.unwrap();
    let revision=Uuid::new_v4();
    assess(revision,AssessmentInput{result:AssessmentResult::LimitIndicated,revises_event_id:Some(containment),..containment_input}).await.unwrap();
    let snapshot=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert_eq!(snapshot.assessments.len(),4);assert_eq!(snapshot.assessment_dependencies.len(),1);
    assert!(snapshot.assessment_dependencies[0].requires_review);
    assert_eq!(snapshot.assessment_dependencies[0].current_assessment_event_id,revision);
    assert_eq!(snapshot.nodes,before.nodes);assert_eq!(snapshot.arguments,before.arguments);assert_eq!(snapshot.grounding_sets,before.grounding_sets);
    assert!(snapshot.question_decisions.is_empty());
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
async fn information_gap_support_is_visible_without_inferring_absence_or_misconduct() {
    let f=fixture().await;
    let sit=situation(&f,"Review a gap used as support").await;
    let hypothesis=node(&f,sit,ExpertNodeKind::Hypothesis,"A payment record may have been concealed").await;
    let unrelated=node(&f,sit,ExpertNodeKind::Hypothesis,"Another explanation").await;
    let gap=node(&f,sit,ExpertNodeKind::InformationGap,"Whether the relevant ledger interval was collected").await;
    let fref=&f;
    let route=|conclusion,effect,role|async move {
        let arg=Uuid::new_v4();let set=Uuid::new_v4();
        epistemic_workbench::create_argument(&fref.service,fref.workspace,sit,fref.principal,arg,conclusion,ArgumentMode::Abductive,"Authored route only").await.unwrap();
        epistemic_workbench::create_grounding_set(&fref.service,fref.workspace,sit,fref.principal,arg,set,effect,GroundingAdequacy::Partial,"Gap considered in this context",&[NewGroundingMember{member_id:Uuid::new_v4(),role,ground_object_id:Some(gap),evidence_id:None,rationale:"Possible gap relevance".into()}]).await.unwrap();
        epistemic_workbench::GroundingRouteRef{argument_id:arg,grounding_set_id:set}
    };
    route(unrelated,GroundingEffect::Supports,GroundingMemberRole::Context).await;
    route(unrelated,GroundingEffect::Challenges,GroundingMemberRole::Premise).await;
    let before=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert!(before.reasoning_diagnostics.iter().all(|d|d.information_gap_supports.is_empty()));
    let support=route(hypothesis,GroundingEffect::Supports,GroundingMemberRole::Premise).await;
    let after=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    let diagnostic=after.reasoning_diagnostics.iter().find(|d|d.conclusion_object_id==hypothesis).unwrap();
    assert_eq!(diagnostic.information_gap_supports.len(),1);
    assert_eq!(diagnostic.information_gap_supports[0].gap_object_id,gap);
    assert_eq!(diagnostic.information_gap_supports[0].supporting_routes,vec![support]);
    assert!(diagnostic.contradictory_supports.is_empty());
    assert!(after.reasoning_diagnostics.iter().find(|d|d.conclusion_object_id==unrelated).unwrap().information_gap_supports.is_empty());
    assert!(after.assessments.is_empty()&&after.question_decisions.is_empty());
}

#[tokio::test]
async fn contradictory_support_is_an_exact_structural_review_candidate_not_a_verdict() {
    let f=fixture().await;
    let sit=situation(&f,"Examine potential self-sealing reasoning").await;
    let hypothesis=node(&f,sit,ExpertNodeKind::Hypothesis,"The record was intentionally concealed").await;
    let left=node(&f,sit,ExpertNodeKind::Proposition,"The ledger was made available").await;
    let right=node(&f,sit,ExpertNodeKind::Proposition,"The ledger was not made available").await;
    let unrelated=node(&f,sit,ExpertNodeKind::Hypothesis,"An unrelated account").await;
    let fref=&f;
    let route=|conclusion,ground,effect,role|async move {
        let arg=Uuid::new_v4(); let set=Uuid::new_v4();
        epistemic_workbench::create_argument(&fref.service,fref.workspace,sit,fref.principal,arg,conclusion,ArgumentMode::Abductive,"An explicitly authored route").await.unwrap();
        epistemic_workbench::create_grounding_set(&fref.service,fref.workspace,sit,fref.principal,arg,set,effect,GroundingAdequacy::Partial,"Attributed support",&[NewGroundingMember{member_id:Uuid::new_v4(),role,ground_object_id:Some(ground),evidence_id:None,rationale:"Explicit ground".into()}]).await.unwrap();
        epistemic_workbench::GroundingRouteRef{argument_id:arg,grounding_set_id:set}
    };
    let a=route(hypothesis,left,GroundingEffect::Supports,GroundingMemberRole::Premise).await;
    let b=route(hypothesis,right,GroundingEffect::Supports,GroundingMemberRole::Premise).await;
    let _context=route(unrelated,left,GroundingEffect::Supports,GroundingMemberRole::Context).await;
    let _actual=route(unrelated,right,GroundingEffect::Supports,GroundingMemberRole::Premise).await;
    let _opposition=route(unrelated,left,GroundingEffect::Challenges,GroundingMemberRole::Premise).await;
    let before=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    assert!(before.reasoning_diagnostics.iter().all(|d|d.contradictory_supports.is_empty()));
    let relation=epistemic_workbench::create_relation(&f.service,f.workspace,sit,f.principal,Uuid::new_v4(),RelationFamily::Semantic,RelationKind::Contradicts,left,right,"The two alternatives are incompatible in the same context").await.unwrap();
    let after=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    let diagnostic=after.reasoning_diagnostics.iter().find(|d|d.conclusion_object_id==hypothesis).unwrap();
    assert_eq!(diagnostic.contradictory_supports.len(),1);
    let candidate=&diagnostic.contradictory_supports[0];
    assert_eq!(candidate.relation_id,relation.relation_id);
    assert_eq!(candidate.left_supporting_routes,vec![if relation.left_object_id==left{a}else{b}]);
    assert_eq!(candidate.right_supporting_routes,vec![if relation.right_object_id==right{b}else{a}]);
    assert!(after.reasoning_diagnostics.iter().find(|d|d.conclusion_object_id==unrelated).unwrap().contradictory_supports.is_empty());
    assert!(after.assessments.is_empty()&&after.question_decisions.is_empty());
    let gap=node(&f,sit,ExpertNodeKind::InformationGap,"Whether another collection path was searched").await;
    let gap_route=route(hypothesis,gap,GroundingEffect::Supports,GroundingMemberRole::Premise).await;
    let combined=epistemic_workbench::snapshot(&f.service,f.workspace,sit).await.unwrap();
    let d=combined.reasoning_diagnostics.iter().find(|d|d.conclusion_object_id==hypothesis).unwrap();
    assert_eq!(d.contradictory_supports.len(),1);
    assert_eq!(d.information_gap_supports[0].supporting_routes,vec![gap_route]);
    assert!(combined.assessments.is_empty());
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
