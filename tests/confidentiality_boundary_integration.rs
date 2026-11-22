#![cfg(feature = "pg-integration")]
//! Composed AT-06/AT-21 witnesses at the supported workspace boundary.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    api, authz,
    epistemic_workbench::{
        self as workbench, ArgumentMode, ExpertNodeKind, GroundingAdequacy, GroundingEffect,
        GroundingMemberRole, NewGroundingMember, WorkbenchError, assessment::*,
    },
    postgres_ingest::ingest_source,
    situation_store,
    source_import::SourceFormat,
};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Compartment {
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    token: String,
}

impl Compartment {
    fn path(&self) -> String {
        format!(
            "/v1/workspaces/{}/situations/{}/epistemic/workbench",
            self.workspace, self.situation
        )
    }
}

async fn compartment(admin: &PgPool, service: &PgPool, focus: &str) -> Compartment {
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'compartment adversarial fixture')").bind(workspace).execute(admin).await.unwrap();
    let token = authz::provision_token(admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    situation_store::create(service, workspace, principal, situation, focus)
        .await
        .unwrap();
    Compartment {
        workspace,
        situation,
        principal,
        token,
    }
}

async fn get(router: &Router, path: &str, token: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn node(service: &PgPool, c: &Compartment, kind: ExpertNodeKind, text: &str) -> Uuid {
    let id = Uuid::new_v4();
    workbench::create_node(
        service,
        c.workspace,
        c.situation,
        c.principal,
        id,
        kind,
        text,
        "Attributed within the permitted informational universe",
    )
    .await
    .unwrap();
    id
}

async fn source(service: &PgPool, c: &Compartment, content: &[u8]) -> Uuid {
    let doc = Uuid::new_v4();
    let result = ingest_source(
        service,
        c.workspace,
        doc,
        SourceFormat::WhatsappExport,
        "fixture://officer-report",
        content,
    )
    .await
    .unwrap();
    situation_store::add_source(service, c.workspace, c.principal, c.situation, doc)
        .await
        .unwrap();
    result.fragment_ids[0]
}

async fn route(service: &PgPool, c: &Compartment, premise: Uuid, conclusion: Uuid) -> (Uuid, Uuid) {
    let argument = Uuid::new_v4();
    let set = Uuid::new_v4();
    workbench::create_argument(
        service,
        c.workspace,
        c.situation,
        c.principal,
        argument,
        conclusion,
        ArgumentMode::Abductive,
        "Locally authored reasoning",
    )
    .await
    .unwrap();
    workbench::create_grounding_set(
        service,
        c.workspace,
        c.situation,
        c.principal,
        argument,
        set,
        GroundingEffect::Supports,
        GroundingAdequacy::Partial,
        "One scoped premise",
        &[NewGroundingMember {
            member_id: Uuid::new_v4(),
            role: GroundingMemberRole::Premise,
            ground_object_id: Some(premise),
            evidence_id: None,
            rationale: "Explicit reliance".into(),
        }],
    )
    .await
    .unwrap();
    (argument, set)
}

#[tokio::test]
async fn confidential_findings_and_revision_signals_do_not_cross_workspace_boundary() {
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let router = api::router(secure);
    let public = compartment(
        &admin,
        &service,
        "Antônio: apparent unauthorized cooperation",
    )
    .await;
    let public_evidence = source(
        &service,
        &public,
        b"[02/11/2026, 12:00] Witness: Officer Roberto met the organization\n",
    )
    .await;
    let public_premise = node(
        &service,
        &public,
        ExpertNodeKind::Proposition,
        "Officer Roberto met the organization",
    )
    .await;
    let public_finding = node(
        &service,
        &public,
        ExpertNodeKind::Finding,
        "Possible unauthorized cooperation by Roberto",
    )
    .await;
    let public_route = route(&service, &public, public_premise, public_finding).await;
    let assessment = AssessmentInput {
        target: AssessmentTarget::Evidence {
            evidence_id: public_evidence,
        },
        dimension: AssessmentDimension::Reliability,
        result: AssessmentResult::Reliable,
        confidence: AssessmentConfidence::Moderate,
        criterion: "Record continuity".into(),
        context: "Available local information".into(),
        rationale: "No alteration known to this investigator".into(),
        grounds: vec![],
        revises_event_id: None,
    };
    let public_assessment = Uuid::new_v4();
    record(
        &service,
        public.workspace,
        public.situation,
        public.principal,
        public_assessment,
        assessment.clone(),
    )
    .await
    .unwrap();
    let public_dependency = DependencyInput {
        argument_id: public_route.0,
        grounding_set_id: public_route.1,
        assessment_event_id: public_assessment,
        rationale: "Dependent on source reliability".into(),
    };
    record_dependency(
        &service,
        public.workspace,
        public.situation,
        public.principal,
        Uuid::new_v4(),
        public_dependency.clone(),
    )
    .await
    .unwrap();
    let (status, before) = get(&router, &public.path(), &public.token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        before["assessment_dependencies"][0]["requires_review"],
        false
    );
    let search_path = format!("{}/evidence/search?q=undercover", public.path());
    let search_before = get(&router, &search_path, &public.token).await;
    assert_eq!(
        search_before,
        (StatusCode::OK, serde_json::json!({"results":[]}))
    );
    let graph_path = format!("/v1/workspaces/{}/graph", public.workspace);
    let graph_before = get(&router, &graph_path, &public.token).await;
    assert_eq!(graph_before.0, StatusCode::OK);
    let absent_path = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/workbench",
        Uuid::new_v4(),
        Uuid::new_v4()
    );
    let absent = get(&router, &absent_path, &public.token).await;
    assert_eq!(absent.0, StatusCode::FORBIDDEN);

    // The protected operation now exists. A separate source and exact local pins
    // preserve the alternative explanation without silently merging shared names.
    let hidden = compartment(&admin, &service, "Bernardo: protected operation").await;
    let secret = source(
        &service,
        &hidden,
        b"[02/11/2026, 12:00] Controller: Roberto has authorization for undercover operation\n",
    )
    .await;
    let hidden_premise = node(
        &service,
        &hidden,
        ExpertNodeKind::Proposition,
        "Roberto has an undercover authorization",
    )
    .await;
    let hidden_finding = node(
        &service,
        &hidden,
        ExpertNodeKind::Finding,
        "Meeting may have been authorized undercover activity",
    )
    .await;
    let hidden_route = route(&service, &hidden, hidden_premise, hidden_finding).await;
    let hidden_assessment = Uuid::new_v4();
    let hidden_input = AssessmentInput {
        target: AssessmentTarget::Evidence {
            evidence_id: secret,
        },
        rationale: "Controller report; exact source retained".into(),
        ..assessment.clone()
    };
    record(
        &service,
        hidden.workspace,
        hidden.situation,
        hidden.principal,
        hidden_assessment,
        hidden_input.clone(),
    )
    .await
    .unwrap();
    record_dependency(
        &service,
        hidden.workspace,
        hidden.situation,
        hidden.principal,
        Uuid::new_v4(),
        DependencyInput {
            argument_id: hidden_route.0,
            grounding_set_id: hidden_route.1,
            assessment_event_id: hidden_assessment,
            rationale: "Protected local dependence".into(),
        },
    )
    .await
    .unwrap();
    let (_, hidden_before) = get(&router, &hidden.path(), &hidden.token).await;
    assert_eq!(
        hidden_before["assessment_dependencies"][0]["requires_review"],
        false
    );
    assert_eq!(get(&router, &hidden.path(), &public.token).await, absent);
    assert_eq!(
        get(&router, &public.path(), &public.token).await,
        (StatusCode::OK, before.clone())
    );
    assert_eq!(
        get(&router, &search_path, &public.token).await,
        search_before
    );

    // A protected revision creates authentic local review attention, not a hint
    // or suspension in the other workspace. No cross-boundary dependency exists.
    let revision = Uuid::new_v4();
    record(
        &service,
        hidden.workspace,
        hidden.situation,
        hidden.principal,
        revision,
        AssessmentInput {
            result: AssessmentResult::Unreliable,
            rationale: "Protected investigation identified alteration in source material".into(),
            revises_event_id: Some(hidden_assessment),
            ..hidden_input
        },
    )
    .await
    .unwrap();
    let (_, hidden_after) = get(&router, &hidden.path(), &hidden.token).await;
    assert_eq!(
        hidden_after["assessment_dependencies"][0]["requires_review"],
        true
    );
    assert_eq!(
        hidden_after["assessment_dependencies"][0]["current_assessment_event_id"],
        revision.to_string()
    );
    assert_eq!(
        get(&router, &public.path(), &public.token).await,
        (StatusCode::OK, before.clone())
    );
    assert_eq!(get(&router, &hidden.path(), &public.token).await, absent);
    assert_eq!(
        get(&router, &search_path, &public.token).await,
        search_before
    );
    assert_eq!(get(&router, &graph_path, &public.token).await, graph_before);
    assert!(matches!(
        record_dependency(
            &service,
            public.workspace,
            public.situation,
            public.principal,
            Uuid::new_v4(),
            DependencyInput {
                assessment_event_id: hidden_assessment,
                ..public_dependency
            }
        )
        .await,
        Err(WorkbenchError::Invisible)
    ));
    assert!(
        !before.to_string().contains("undercover")
            && !before.to_string().contains(&hidden.workspace.to_string())
            && !before.to_string().contains(&revision.to_string())
    );

    // Explicit actual authorization permits a reviewer to inspect both views,
    // but neither merges nor discloses them to the other investigator.
    let reviewer = Uuid::new_v4();
    let review_token = authz::provision_token(&admin, reviewer, public.workspace, true, false, 1)
        .await
        .unwrap();
    assert_eq!(get(&router, &hidden.path(), &review_token).await, absent);
    authz::provision_token(&admin, reviewer, hidden.workspace, true, false, 1)
        .await
        .unwrap();
    assert_eq!(
        get(&router, &public.path(), &review_token).await,
        (StatusCode::OK, before.clone())
    );
    assert_eq!(
        get(&router, &hidden.path(), &review_token).await,
        (StatusCode::OK, hidden_after)
    );
    assert_eq!(get(&router, &hidden.path(), &public.token).await, absent);
    sqlx::query("UPDATE niccolo_auth.memberships SET revoked_at=now() WHERE principal_id=$1 AND workspace_id=$2").bind(reviewer).bind(hidden.workspace).execute(&admin).await.unwrap();
    assert_eq!(get(&router, &hidden.path(), &review_token).await, absent);
    assert_eq!(
        get(&router, &public.path(), &review_token).await,
        (StatusCode::OK, before)
    );
}
