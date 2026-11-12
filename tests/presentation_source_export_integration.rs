#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz, presentation_package};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    router: Router,
    admin: sqlx::PgPool,
    workspace: Uuid,
    writer_principal: Uuid,
    writer: String,
    reader: String,
}

async fn setup() -> Fixture {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'presentation-source-export')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    let writer_principal = Uuid::new_v4();
    let writer = authz::provision_token(&admin, writer_principal, workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Fixture {
        router: api::router(secure),
        admin,
        workspace,
        writer_principal,
        writer,
        reader,
    }
}

async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}

fn json_request(method: &str, uri: String, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .unwrap()
}

fn analysis_path(f: &Fixture, situation: Uuid, analysis: Uuid) -> String {
    format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    )
}

#[tokio::test]
async fn presentation_exports_recorded_policy_history_and_human_credence_without_truth_claims() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let claim = Uuid::new_v4();
    let policy = Uuid::new_v4();
    let credence = Uuid::new_v4();

    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                format!("/v1/workspaces/{}/situations/{situation}", f.workspace),
                &f.writer,
                Some(json!({"focus":"Test recorded epistemic controls"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                analysis_path(&f, situation, analysis),
                &f.writer,
                Some(json!({"goal":"Publish an answer with separately recorded controls"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                format!("{}/output", analysis_path(&f, situation, analysis)),
                &f.writer,
                Some(json!({
                    "answer_text":"Published before the later control records.",
                    "coverage_note":"No claim that later controls generated this answer."
                })),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claims
         (workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,created_by_principal_id)
         VALUES($1,$2,$3,$4,1,'POSSIBILITY','A bounded possibility',$5)",
    )
    .bind(f.workspace)
    .bind(situation)
    .bind(analysis)
    .bind(claim)
    .bind(f.writer_principal)
    .execute(&f.admin)
    .await
    .unwrap();

    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                format!("/v1/workspaces/{}/epistemic/policies/{policy}", f.workspace),
                &f.writer,
                Some(json!({
                    "situation_id":situation,
                    "policy":{
                        "support_multiplier_pct":80,
                        "counter_multiplier_pct":140,
                        "per_origin_cap":75
                    },
                    "rationale":"Deliberately conservative counter-weighting"
                })),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                format!(
                    "{}/claims/{claim}/credence/{credence}",
                    analysis_path(&f, situation, analysis)
                ),
                &f.writer,
                Some(json!({
                    "confidence_percent":63,
                    "usage_policy":"SUBJECTIVE_BELIEF",
                    "rationale":"Human context unavailable to the model"
                })),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let (status, package) = call(
        f.router.clone(),
        json_request(
            "GET",
            format!("{}/presentation-package", analysis_path(&f, situation, analysis)),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let policies: Value = serde_json::from_str(
        package["files"]["data/epistemic_policies.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(policies.as_array().unwrap().len(), 1);
    assert_eq!(policies[0]["policy_id"], policy.to_string());
    assert_eq!(policies[0]["scope"], "SITUATION");
    assert_eq!(policies[0]["support_multiplier_pct"], 80);
    assert_eq!(policies[0]["counter_multiplier_pct"], 140);
    assert_eq!(policies[0]["per_origin_cap"], 75);
    assert_eq!(policies[0]["is_probability_model"], false);
    assert_eq!(
        policies[0]["proves_policy_was_used_for_published_output"],
        false
    );

    let human: Value = serde_json::from_str(
        package["files"]["data/human_credence.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(human.as_array().unwrap().len(), 1);
    assert_eq!(human[0]["claim_id"], claim.to_string());
    assert_eq!(human[0]["credence_id"], credence.to_string());
    assert_eq!(human[0]["confidence_percent"], 63);
    assert_eq!(human[0]["is_human_declaration"], true);
    assert_eq!(human[0]["is_calibrated_probability"], false);
    assert_eq!(human[0]["is_documentary_evidence"], false);
    assert_eq!(human[0]["makes_claim_a_fact"], false);
    assert_eq!(human[0]["part_of_frozen_analysis_input"], false);

    let scope: Value = serde_json::from_str(
        package["files"]["data/epistemic_export_scope.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(scope["recorded_policy_count"], 1);
    assert_eq!(scope["recorded_human_credence_count"], 1);
    assert_eq!(scope["deterministic_scores_exported"], false);
    assert_eq!(scope["policy_history_proves_policy_used_for_output"], false);
    assert_eq!(scope["human_credence_is_documentary_evidence"], false);
    assert_eq!(scope["human_credence_is_calibrated_probability"], false);
}

#[tokio::test]
async fn examined_source_text_requires_explicit_write_authorized_export() {
    let f = setup().await;
    let situation = Uuid::new_v4();
    let document = Uuid::new_v4();
    let analysis = Uuid::new_v4();

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!("/v1/workspaces/{}/situations/{situation}", f.workspace),
            &f.writer,
            Some(json!({"focus":"Which message was actually examined?"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let source = b"[02/11/2026, 12:00] Ana: examined private text\n[02/11/2026, 12:01] Bob: unexamined private text\n";
    let ingest = Request::builder()
        .method("POST")
        .uri(format!("/v1/workspaces/{}/sources/{document}", f.workspace))
        .header(header::AUTHORIZATION, format!("Bearer {}", f.writer))
        .header(header::CONTENT_TYPE, "text/plain")
        .header("x-source-locator", "fixture://presentation-source-export")
        .body(Body::from(source.as_slice()))
        .unwrap();
    let (status, imported) = call(f.router.clone(), ingest).await;
    assert_eq!(status, StatusCode::CREATED);
    let fragments = imported["fragment_ids"].as_array().unwrap();
    assert_eq!(fragments.len(), 2);
    let examined = fragments[0].as_str().unwrap().parse::<Uuid>().unwrap();
    let unexamined = fragments[1].as_str().unwrap().parse::<Uuid>().unwrap();

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!(
                "/v1/workspaces/{}/situations/{situation}/sources/{document}",
                f.workspace
            ),
            &f.writer,
            None,
        ),
    )
    .await;
    assert!(matches!(status, StatusCode::CREATED | StatusCode::OK));

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            analysis_path(&f, situation, analysis),
            &f.writer,
            Some(json!({"goal":"Explain only what the examined source supports"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!(
                "{}/examined-fragments/{examined}",
                analysis_path(&f, situation, analysis)
            ),
            &f.writer,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!("{}/output", analysis_path(&f, situation, analysis)),
            &f.writer,
            Some(json!({
                "answer_text":"Only Ana's recorded fragment was examined.",
                "coverage_note":"One of two source fragments examined."
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let package_path = format!(
        "{}/presentation-package",
        analysis_path(&f, situation, analysis)
    );
    let (status, standard) = call(
        f.router.clone(),
        json_request("GET", package_path.clone(), &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(standard["includes_examined_source_fragment_contents"], false);
    assert_eq!(standard["includes_original_source_fragment_contents"], false);
    assert!(standard["files"]["data/examined_source_fragments.json"].is_null());

    let preview_path = format!("{package_path}/source-preview");
    let (status, preview) = call(
        f.router.clone(),
        json_request("GET", preview_path, &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["examined_fragment_count"], 1);
    assert!(preview["examined_source_text_bytes"].as_u64().unwrap() > 0);
    assert_eq!(preview["max_examined_fragments"], 128);
    assert_eq!(preview["max_examined_source_text_bytes"], 512 * 1024);
    assert_eq!(preview["within_export_bounds"], true);
    assert_eq!(preview["selection"], "RECORDED_EXAMINED_ONLY");
    assert_eq!(preview["includes_original_uploads"], false);
    assert_eq!(preview["establishes_truth"], false);
    assert_eq!(preview["exposes_source_text"], false);
    assert!(preview.get("content").is_none());

    let export_event = Uuid::new_v4();
    let sensitive_body = json!({
        "include_examined_source_fragments":true,
        "export_event_id":export_event
    });
    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path.clone(),
            &f.reader,
            Some(sensitive_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path.clone(),
            &f.writer,
            Some(json!({
                "include_examined_source_fragments":false,
                "export_event_id":Uuid::new_v4()
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, sensitive) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path.clone(),
            &f.writer,
            Some(sensitive_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sensitive["includes_examined_source_fragment_contents"], true);
    assert_eq!(sensitive["includes_original_source_fragment_contents"], false);
    assert_eq!(sensitive["examined_source_extracts_are_original_uploads"], false);
    assert_eq!(sensitive["examined_source_extracts_establish_truth"], false);
    assert_eq!(sensitive["published_to_git"], false);

    let extracts: Value = serde_json::from_str(
        sensitive["files"]["data/examined_source_fragments.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let rows = extracts.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["evidence_id"], examined.to_string());
    assert_ne!(rows[0]["evidence_id"], unexamined.to_string());
    assert_eq!(
        rows[0]["document_sha256"],
        imported["sha256"].as_str().unwrap()
    );
    assert_eq!(
        rows[0]["fragment_sha256"],
        format!(
            "{:x}",
            Sha256::digest(rows[0]["content"].as_str().unwrap().as_bytes())
        )
    );
    assert!(rows[0]["content"].as_str().unwrap().contains("examined private text"));
    assert!(!rows[0]["content"].as_str().unwrap().contains("unexamined private text"));
    assert!(
        sensitive["files"]["README.md"]
            .as_str()
            .unwrap()
            .contains("do not establish truth")
    );

    let (status, replayed) = call(
        f.router.clone(),
        json_request(
            "POST",
            package_path.clone(),
            &f.writer,
            Some(sensitive_body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        replayed["files"]["data/examined_source_fragments.json"],
        sensitive["files"]["data/examined_source_fragments.json"]
    );

    let (status, events) = call(
        f.router.clone(),
        json_request(
            "GET",
            format!("{package_path}/export-events?limit=25"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let events = events["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["export_event_id"], export_event.to_string());
    assert_eq!(events[0]["analysis_id"], analysis.to_string());
    assert_eq!(events[0]["examined_fragment_count"], 1);
    assert!(events[0]["examined_source_text_bytes"].as_u64().unwrap() > 0);
    assert_eq!(
        events[0]["package_sha256"],
        presentation_package::package_sha256(
            &serde_json::from_value::<presentation_package::PresentationPackage>(sensitive.clone())
                .unwrap()
        )
        .unwrap()
    );
    assert_eq!(events[0]["includes_original_uploads"], false);
    assert_eq!(events[0]["establishes_truth"], false);

    let audit_rows: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.analysis_export_events
          WHERE workspace_id=$1 AND analysis_id=$2 AND export_event_id=$3",
    )
    .bind(f.workspace)
    .bind(analysis)
    .bind(export_event)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(audit_rows, 1);

    let other_analysis = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                analysis_path(&f, situation, other_analysis),
                &f.writer,
                Some(json!({"goal":"A different published analysis"})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                format!(
                    "{}/examined-fragments/{examined}",
                    analysis_path(&f, situation, other_analysis)
                ),
                &f.writer,
                None,
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            json_request(
                "POST",
                format!("{}/output", analysis_path(&f, situation, other_analysis)),
                &f.writer,
                Some(json!({
                    "answer_text":"A second immutable answer.",
                    "coverage_note":"Same examined fragment, different analysis."
                })),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status, _) = call(
        f.router.clone(),
        json_request(
            "POST",
            format!(
                "{}/presentation-package",
                analysis_path(&f, situation, other_analysis)
            ),
            &f.writer,
            Some(json!({
                "include_examined_source_fragments":true,
                "export_event_id":export_event
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let audit_rows_after_conflict: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.analysis_export_events
          WHERE workspace_id=$1 AND export_event_id=$2",
    )
    .bind(f.workspace)
    .bind(export_event)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(audit_rows_after_conflict, 1);
}
