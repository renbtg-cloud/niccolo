#![cfg(feature = "pg-integration")]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    analysis_structure::AnalysisScope, api, authz, reasoning_packet, situation_store,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Harness {
    router: Router,
    admin: PgPool,
    secure: PgPool,
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    token: String,
}

async fn setup() -> Harness {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").expect("admin DB"))
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(12)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").expect("secure DB"))
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'reasoning-packet-sources')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    Harness {
        router: api::router(secure.clone()),
        admin,
        secure,
        workspace,
        situation,
        principal,
        token,
    }
}

fn json_request(method: &str, path: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json");
    let bytes = body
        .map(|value| serde_json::to_vec(&value).unwrap())
        .unwrap_or_default();
    if !bytes.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder.body(Body::from(bytes)).unwrap()
}

async fn invoke(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}

// Seed the historical representation directly. The shipping HTTP router no longer accepts source bytes.
async fn attach_historical_source(
    h: &Harness,
    locator: &str,
    speaker: &str,
    lines: &[&str],
) -> Uuid {
    let document = Uuid::new_v4();
    let text = lines.join("\n");
    let mut tx = h.admin.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length)
         VALUES($1,$2,'WHATSAPP_EXPORT',$3,$4,$5)",
    )
    .bind(h.workspace).bind(document).bind(locator)
    .bind(format!("{:x}", Sha256::digest(text.as_bytes())))
    .bind(text.len() as i64)
    .execute(&mut *tx).await.unwrap();
    let mut offset = 0;
    for (ordinal, line) in lines.iter().enumerate() {
        let evidence = Uuid::new_v4();
        let end = offset + line.len() as i32;
        sqlx::query(
            "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class)
             VALUES($1,$2,$3,$4,'REPORTED_SPEECH')",
        )
        .bind(h.workspace).bind(evidence).bind(format!("{locator}#{ordinal}")).bind(line)
        .execute(&mut *tx).await.unwrap();
        sqlx::query(
            "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label)
             VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(h.workspace).bind(document).bind(ordinal as i32).bind(evidence).bind(offset).bind(end).bind(speaker)
        .execute(&mut *tx).await.unwrap();
        offset = end + 1;
    }
    tx.commit().await.unwrap();
    authz::with_auth_digest(
        authz::credential_digest(&h.token).unwrap(),
        situation_store::add_source(&h.secure, h.workspace, h.principal, h.situation, document),
    )
    .await
    .unwrap();
    document
}

#[tokio::test]
async fn v1041_v1060_reasoning_packet_uses_only_fragments_from_frozen_revision() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, created) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &situation_path,
            &h.token,
            Some(json!({"focus":"Understand the sequence"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["revision"], 1);

    let before = attach_historical_source(&h, "before.txt", "Ana", &["BEFORE_FREEZE_SOURCE"]).await;

    let analysis = Uuid::new_v4();
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, frozen) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &analysis_path,
            &h.token,
            Some(json!({"goal":"Use only what was frozen"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let frozen_revision = frozen["input_revision"].as_i64().unwrap();

    let after = attach_historical_source(&h, "after.txt", "Beto", &["AFTER_FREEZE_SOURCE"]).await;
    assert_ne!(before, after);

    let digest = authz::credential_digest(&h.token).unwrap();
    let packet = authz::with_auth_digest(digest, async {
        reasoning_packet::build(
            &h.secure,
            AnalysisScope {
                workspace: h.workspace,
                situation: h.situation,
                analysis,
            },
        )
        .await
        .unwrap()
    })
    .await;

    assert_eq!(packet.frozen_input.revision, frozen_revision);
    assert_eq!(packet.frozen_input.sources, vec![before]);
    assert_eq!(packet.frozen_input.source_fragment_count_total, 1);
    assert_eq!(packet.frozen_input.source_fragment_count_included, 1);
    assert!(!packet.frozen_input.source_packet_truncated);
    assert!(
        packet
            .frozen_input
            .source_fragments
            .iter()
            .any(|fragment| fragment.content.contains("BEFORE_FREEZE_SOURCE"))
    );
    assert!(
        packet
            .frozen_input
            .source_fragments
            .iter()
            .all(|fragment| !fragment.content.contains("AFTER_FREEZE_SOURCE"))
    );
    assert!(
        packet
            .frozen_input
            .source_fragments
            .iter()
            .all(|fragment| fragment.document_id == before)
    );
}

#[tokio::test]
async fn v1061_v1080_reasoning_packet_truncates_deterministically_at_fragment_budget() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &situation_path,
            &h.token,
            Some(json!({"focus":"Bound the frozen source packet"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let many = (0..300)
        .map(|i| format!("FROZEN_FRAGMENT_{i:03}"))
        .collect::<Vec<_>>();
    let lines = many.iter().map(String::as_str).collect::<Vec<_>>();
    let document = attach_historical_source(&h, "many.txt", "Ana", &lines).await;

    let analysis = Uuid::new_v4();
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &analysis_path,
            &h.token,
            Some(json!({"goal":"Bound source material deterministically"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let digest = authz::credential_digest(&h.token).unwrap();
    let build = |digest| async {
        authz::with_auth_digest(digest, async {
            reasoning_packet::build(
                &h.secure,
                AnalysisScope {
                    workspace: h.workspace,
                    situation: h.situation,
                    analysis,
                },
            )
            .await
            .unwrap()
        })
        .await
    };
    let first = build(digest.clone()).await;
    let second = build(digest).await;

    assert_eq!(first.frozen_input.sources, vec![document]);
    assert_eq!(first.frozen_input.source_fragment_count_total, 300);
    assert_eq!(first.frozen_input.source_fragment_count_included, 256);
    assert!(first.frozen_input.source_packet_truncated);
    assert_eq!(
        first.frozen_input.source_fragments.first().unwrap().ordinal,
        0
    );
    assert_eq!(
        first.frozen_input.source_fragments.last().unwrap().ordinal,
        255
    );
    assert_eq!(
        first.frozen_input.source_fragments,
        second.frozen_input.source_fragments
    );
    assert_eq!(
        reasoning_packet::sha256(&first).unwrap(),
        reasoning_packet::sha256(&second).unwrap()
    );
    assert!(
        first
            .frozen_input
            .source_fragments
            .iter()
            .all(|fragment| fragment.document_id == document)
    );

    let summary_path =
        format!("{situation_path}/analysis-snapshots/{analysis}/bff/reasoning-input");
    let (status, summary) = invoke(
        h.router.clone(),
        json_request("GET", &summary_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary["source_fragment_count_total"], 300);
    assert_eq!(summary["source_fragment_count_included"], 256);
    assert_eq!(summary["source_packet_truncated"], true);
    assert_eq!(
        summary["reasoning_packet_sha256"],
        reasoning_packet::sha256(&first).unwrap()
    );
    assert_eq!(summary["packet_asserts_truth"], false);
    assert_eq!(summary["summary_is_provider_input_observability"], true);
}

#[tokio::test]
async fn v1161_v1180_reasoning_packet_freezes_human_identity_assignment_at_analysis_horizon() {
    let h = setup().await;
    let situation_path = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &situation_path,
            &h.token,
            Some(json!({"focus":"Keep identity mapping historical"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let document = attach_historical_source(
        &h,
        "identity-freeze.txt",
        "Ana",
        &["identity freeze source"],
    )
    .await;

    let first_person = Uuid::new_v4();
    let first_identity_path = format!("/v1/workspaces/{}/identities/{first_person}", h.workspace);
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &first_identity_path,
            &h.token,
            Some(json!({"display_label":"Ana — first human mapping"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let first_event = Uuid::new_v4();
    let first_event_path = format!(
        "/v1/workspaces/{}/identity-events/{first_event}",
        h.workspace
    );
    let (status, first_assignment) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &first_event_path,
            &h.token,
            Some(json!({
                "document_id":document,
                "speaker_label":"Ana",
                "operation":"assign",
                "person_id":first_person,
                "human_reason":"first explicit human mapping"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_event_no = first_assignment["event_no"].as_i64().unwrap();

    let analysis = Uuid::new_v4();
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, frozen) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &analysis_path,
            &h.token,
            Some(json!({"goal":"Use the human-confirmed identity available at freeze time"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(frozen["identity_event_no_max"], first_event_no);

    let second_person = Uuid::new_v4();
    let second_identity_path = format!("/v1/workspaces/{}/identities/{second_person}", h.workspace);
    let (status, _) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &second_identity_path,
            &h.token,
            Some(json!({"display_label":"Ana — later corrected mapping"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let second_event = Uuid::new_v4();
    let second_event_path = format!(
        "/v1/workspaces/{}/identity-events/{second_event}",
        h.workspace
    );
    let (status, later_assignment) = invoke(
        h.router.clone(),
        json_request(
            "POST",
            &second_event_path,
            &h.token,
            Some(json!({
                "document_id":document,
                "speaker_label":"Ana",
                "operation":"assign",
                "person_id":second_person,
                "human_reason":"later explicit correction"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(later_assignment["event_no"].as_i64().unwrap() > first_event_no);

    let digest = authz::credential_digest(&h.token).unwrap();
    let packet = authz::with_auth_digest(digest, async {
        reasoning_packet::build(
            &h.secure,
            AnalysisScope {
                workspace: h.workspace,
                situation: h.situation,
                analysis,
            },
        )
        .await
        .unwrap()
    })
    .await;

    assert_eq!(packet.frozen_input.identity_event_no_max, first_event_no);
    assert_eq!(packet.frozen_input.identity_bindings.len(), 1);
    let binding = &packet.frozen_input.identity_bindings[0];
    assert_eq!(binding.document_id, document);
    assert_eq!(binding.speaker_label, "Ana");
    assert_eq!(binding.person_id, Some(first_person));
    assert_eq!(
        binding.display_label.as_deref(),
        Some("Ana — first human mapping")
    );
    assert_eq!(binding.event_id, first_event);
    assert_eq!(binding.event_no, first_event_no);
    assert!(packet.frozen_input.identity_bindings_are_human_confirmed);
    assert!(
        !packet
            .frozen_input
            .matching_speaker_labels_imply_same_person
    );

    let summary_path = format!("{analysis_path}/bff/reasoning-input");
    let (status, summary) = invoke(
        h.router.clone(),
        json_request("GET", &summary_path, &h.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary["identity_event_no_max"], first_event_no);
    assert_eq!(summary["identity_binding_count"], 1);
    assert_eq!(summary["identity_bindings_are_human_confirmed"], true);
    assert_eq!(summary["matching_speaker_labels_imply_same_person"], false);
}
