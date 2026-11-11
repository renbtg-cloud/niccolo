#![cfg(feature = "pg-integration")]

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    analysis_snapshot,
    analysis_structure::AnalysisScope,
    api, authz, forensic_context,
    forensic_import::{self, ForensicImportError, ImportRequest},
    reasoning_packet, situation_forensic_export,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    admin: PgPool,
    secure: PgPool,
    source_workspace: Uuid,
    source_situation: Uuid,
    source_token: String,
    destination_workspace: Uuid,
    destination_principal: Uuid,
    destination_token: String,
    destination_read_token: String,
    router: Router,
}

async fn fixture() -> Fixture {
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").expect("admin DB"))
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(32)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").expect("secure DB"))
        .await
        .unwrap();

    let source_workspace = Uuid::new_v4();
    let destination_workspace = Uuid::new_v4();
    let source_situation = Uuid::new_v4();
    let source_principal = Uuid::new_v4();
    let destination_principal = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO niccolo_auth.principals(principal_id) VALUES($1),($2)
         ON CONFLICT DO NOTHING",
    )
    .bind(source_principal)
    .bind(destination_principal)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label)
         VALUES($1,'forensic-import-source'),($2,'forensic-import-destination')",
    )
    .bind(source_workspace)
    .bind(destination_workspace)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situations
         (workspace_id,situation_id,focus,created_by_principal_id,current_revision)
         VALUES($1,$2,'Old Niccolo state under examination',$3,1)",
    )
    .bind(source_workspace)
    .bind(source_situation)
    .bind(source_principal)
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_revisions
         (workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause)
         VALUES($1,$2,1,'Old Niccolo state under examination',$3,$4,'CREATED')",
    )
    .bind(source_workspace)
    .bind(source_situation)
    .bind("a".repeat(64))
    .bind(source_principal)
    .execute(&admin)
    .await
    .unwrap();

    let source_token =
        authz::provision_token(&admin, source_principal, source_workspace, true, true, 1)
            .await
            .unwrap();
    let destination_token = authz::provision_token(
        &admin,
        destination_principal,
        destination_workspace,
        true,
        true,
        1,
    )
    .await
    .unwrap();
    let destination_read_token = authz::provision_token(
        &admin,
        Uuid::new_v4(),
        destination_workspace,
        true,
        false,
        1,
    )
    .await
    .unwrap();
    let router = api::router(secure.clone());

    Fixture {
        admin,
        secure,
        source_workspace,
        source_situation,
        source_token,
        destination_workspace,
        destination_principal,
        destination_token,
        destination_read_token,
        router,
    }
}

async fn exported(f: &Fixture) -> situation_forensic_export::ForensicSituationExport {
    let digest = authz::credential_digest(&f.source_token).unwrap();
    authz::with_auth_digest(digest, async {
        situation_forensic_export::build(&f.secure, f.source_workspace, f.source_situation)
            .await
            .unwrap()
    })
    .await
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

fn import_request(
    f: &Fixture,
    batch: Uuid,
    token: &str,
    origin: &str,
    package: &situation_forensic_export::ForensicSituationExport,
) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}",
            f.destination_workspace
        ))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"declared_origin":origin,"package":package}).to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn http_import_requires_write_and_preserves_quarantine_flags() {
    let f = fixture().await;
    let package = exported(&f).await;
    let batch = Uuid::new_v4();

    let (read_only_status, _) = call(
        f.router.clone(),
        import_request(
            &f,
            batch,
            &f.destination_read_token,
            "fixture://http-import",
            &package,
        ),
    )
    .await;
    assert_eq!(read_only_status, StatusCode::FORBIDDEN);

    let (status, body) = call(
        f.router.clone(),
        import_request(
            &f,
            batch,
            &f.destination_token,
            "fixture://http-import",
            &package,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["import_batch_id"], batch.to_string());
    assert_eq!(body["import_mode"], "UNTRUSTED");
    assert_eq!(body["verification_status"], "STRUCTURALLY_VERIFIED");
    assert_eq!(body["structural_validity_establishes_truth"], false);
    assert_eq!(
        body["current_credentials_upgrade_imported_statements"],
        false
    );
    assert_eq!(body["imported_model_output_is_documentary_evidence"], false);

    let second_batch = Uuid::new_v4();
    let (replay_status, replay) = call(
        f.router.clone(),
        import_request(
            &f,
            second_batch,
            &f.destination_token,
            "fixture://http-import",
            &package,
        ),
    )
    .await;
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["import_batch_id"], batch.to_string());
}

#[tokio::test]
async fn quarantined_import_is_readable_as_bounded_metadata_without_payload_promotion() {
    let f = fixture().await;
    let package = exported(&f).await;
    let batch = Uuid::new_v4();

    let (created_status, created) = call(
        f.router.clone(),
        import_request(
            &f,
            batch,
            &f.destination_token,
            "fixture://inspectable-import",
            &package,
        ),
    )
    .await;
    assert_eq!(created_status, StatusCode::CREATED);
    assert!(created["item_count"].as_u64().unwrap() > 1);

    let batch_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_read_token),
        )
        .body(Body::empty())
        .unwrap();
    let (batch_status, stored) = call(f.router.clone(), batch_request).await;
    assert_eq!(batch_status, StatusCode::OK);
    assert_eq!(stored["import_batch_id"], batch.to_string());
    assert_eq!(stored["structural_validity_establishes_truth"], false);
    assert_eq!(stored["current_credentials_upgrade_imported_statements"], false);
    assert_eq!(stored["imported_model_output_is_documentary_evidence"], false);

    let first_page_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}/items?limit=1",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_read_token),
        )
        .body(Body::empty())
        .unwrap();
    let (first_status, first_page) = call(f.router.clone(), first_page_request).await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(first_page["items"].as_array().unwrap().len(), 1);
    assert_eq!(first_page["items"][0]["current_credentials_establish_truth"], false);
    assert_eq!(first_page["items"][0]["documentary_evidence"], false);
    assert!(first_page["items"][0].get("payload_json").is_none());
    assert!(first_page["items"][0].get("payload_bytes").is_none());

    let first_item = first_page["items"][0]["item_id"]
        .as_str()
        .expect("item id");
    let detail_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}/items/{first_item}",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_read_token),
        )
        .body(Body::empty())
        .unwrap();
    let (detail_status, detail) = call(f.router.clone(), detail_request).await;
    assert_eq!(detail_status, StatusCode::OK);
    assert_eq!(detail["item"]["item_id"], first_item);
    assert_eq!(detail["item"]["current_credentials_establish_truth"], false);
    assert_eq!(detail["item"]["documentary_evidence"], false);
    assert!(
        detail["payload_json"].is_object(),
        "logical imported state should remain reviewable as quarantined JSON"
    );
    assert_eq!(detail["original_bytes_retained"], false);

    let cursor = first_page["next_after_source_path"]
        .as_str()
        .expect("more than one quarantined item");
    let second_page_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}/items?limit=1&after={}",
            f.destination_workspace,
            cursor
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_read_token),
        )
        .body(Body::empty())
        .unwrap();
    let (second_status, second_page) = call(f.router.clone(), second_page_request).await;
    assert_eq!(second_status, StatusCode::OK);
    assert_eq!(second_page["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        first_page["items"][0]["source_path"],
        second_page["items"][0]["source_path"]
    );

    let wrong_workspace_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.source_token),
        )
        .body(Body::empty())
        .unwrap();
    let (wrong_status, _) = call(f.router.clone(), wrong_workspace_request).await;
    assert_eq!(wrong_status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn forensic_import_route_accepts_valid_packages_larger_than_global_json_limit() {
    let f = fixture().await;
    let package = exported(&f).await;
    let batch = Uuid::new_v4();
    let package_json = serde_json::to_string(&package).unwrap();
    let padding = " ".repeat(5 * 1024 * 1024);
    let body = format!(
        "{{\"declared_origin\":\"fixture://large-http\",\"package\":{padding}{package_json}}}"
    );
    assert!(body.len() > 4 * 1024 * 1024);

    let request = Request::builder()
        .method("POST")
        .uri(format!(
            "/v1/workspaces/{}/forensic-imports/{batch}",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap();

    let (status, imported) = call(f.router.clone(), request).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(imported["import_batch_id"], batch.to_string());
    assert_eq!(imported["import_mode"], "UNTRUSTED");
}

#[tokio::test]
async fn untrusted_import_is_quarantined_and_exact_replay_is_idempotent() {
    let f = fixture().await;
    let package = exported(&f).await;
    let first_request = ImportRequest {
        import_batch_id: Uuid::new_v4(),
        declared_origin: "fixture://old-niccolo/export-1".into(),
    };
    let digest = authz::credential_digest(&f.destination_token).unwrap();
    let first = authz::with_auth_digest(digest.clone(), async {
        forensic_import::import_untrusted(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            &first_request,
            &package,
        )
        .await
        .unwrap()
    })
    .await;
    assert!(!first.replayed);
    assert_eq!(first.import_mode, "UNTRUSTED");
    assert_eq!(first.verification_status, "STRUCTURALLY_VERIFIED");
    assert!(!first.structural_validity_establishes_truth);
    assert!(!first.current_credentials_upgrade_imported_statements);
    assert!(!first.imported_model_output_is_documentary_evidence);
    assert!(first.item_count > 0);

    let native_situations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.situations WHERE workspace_id=$1")
            .bind(f.destination_workspace)
            .fetch_one(&f.admin)
            .await
            .unwrap();
    assert_eq!(native_situations, 0);

    let quarantined: (i64, bool, bool) = sqlx::query_as(
        "SELECT count(*)::bigint,
                bool_or(current_credentials_establish_truth),
                bool_or(documentary_evidence)
           FROM niccolo.forensic_import_items
          WHERE workspace_id=$1 AND import_batch_id=$2",
    )
    .bind(f.destination_workspace)
    .bind(first.import_batch_id)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(quarantined.0 as u64, first.item_count);
    assert!(!quarantined.1);
    assert!(!quarantined.2);

    let replay_request = ImportRequest {
        import_batch_id: Uuid::new_v4(),
        declared_origin: first_request.declared_origin.clone(),
    };
    let replay = authz::with_auth_digest(digest, async {
        forensic_import::import_untrusted(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            &replay_request,
            &package,
        )
        .await
        .unwrap()
    })
    .await;
    assert!(replay.replayed);
    assert_eq!(replay.import_batch_id, first.import_batch_id);
    assert_eq!(replay.item_count, first.item_count);

    let batches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.forensic_import_batches
          WHERE workspace_id=$1 AND source_package_sha256=$2",
    )
    .bind(f.destination_workspace)
    .bind(&first.source_package_sha256)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(batches, 1);
}

#[tokio::test]
async fn reused_batch_id_for_a_different_package_fails_as_provenance_conflict() {
    let f = fixture().await;
    let first_package = exported(&f).await;
    let batch = Uuid::new_v4();
    let digest = authz::credential_digest(&f.destination_token).unwrap();
    let first = ImportRequest {
        import_batch_id: batch,
        declared_origin: "fixture://stable-batch".into(),
    };
    authz::with_auth_digest(digest.clone(), async {
        forensic_import::import_untrusted(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            &first,
            &first_package,
        )
        .await
        .unwrap();
    })
    .await;

    let mut different_package = first_package.clone();
    different_package
        .omissions
        .push("different transport package metadata".into());

    let direct = authz::with_auth_digest(digest, async {
        forensic_import::import_untrusted(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            &first,
            &different_package,
        )
        .await
    })
    .await;
    assert!(matches!(direct, Err(ForensicImportError::Conflict)));

    let (status, _) = call(
        f.router.clone(),
        import_request(
            &f,
            batch,
            &f.destination_token,
            "fixture://stable-batch",
            &different_package,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn selected_quarantined_item_reaches_reasoning_as_current_non_evidence_context() {
    let f = fixture().await;
    let package = exported(&f).await;
    let batch = Uuid::new_v4();
    let (created_status, _) = call(
        f.router.clone(),
        import_request(
            &f,
            batch,
            &f.destination_token,
            "fixture://reasoning-context",
            &package,
        ),
    )
    .await;
    assert_eq!(created_status, StatusCode::CREATED);

    let digest = authz::credential_digest(&f.destination_token).unwrap();
    let page = authz::with_auth_digest(digest.clone(), async {
        forensic_import::list_items(&f.secure, f.destination_workspace, batch, None, 50)
            .await
            .unwrap()
    })
    .await;
    let imported_item = page
        .items
        .iter()
        .find(|item| item.source_kind == "LOGICAL_ROW")
        .expect("logical imported item")
        .item_id;

    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.situations
         (workspace_id,situation_id,focus,created_by_principal_id,current_revision)
         VALUES($1,$2,'Reconsider old Niccolo material',$3,1)",
    )
    .bind(f.destination_workspace)
    .bind(situation)
    .bind(f.destination_principal)
    .execute(&f.admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_revisions
         (workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause)
         VALUES($1,$2,1,'Reconsider old Niccolo material',$3,$4,'CREATED')",
    )
    .bind(f.destination_workspace)
    .bind(situation)
    .bind("c".repeat(64))
    .bind(f.destination_principal)
    .execute(&f.admin)
    .await
    .unwrap();
    authz::with_auth_digest(digest.clone(), async {
        analysis_snapshot::begin(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            situation,
            analysis,
            "Compare the old reasoning with a fresh analysis",
            None,
        )
        .await
        .unwrap();
    })
    .await;

    let binding = Uuid::new_v4();
    let bind_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}/forensic-context/{binding}",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "import_batch_id": batch,
                "item_id": imported_item,
                "note": "Compare this imported historical item; do not treat it as fact."
            })
            .to_string(),
        ))
        .unwrap();
    let (bind_status, bound) = call(f.router.clone(), bind_request).await;
    assert_eq!(bind_status, StatusCode::CREATED);
    assert_eq!(bound["item"]["binding_id"], binding.to_string());
    assert_eq!(bound["item"]["quarantine_provenance"], true);
    assert_eq!(bound["item"]["establishes_truth"], false);
    assert_eq!(bound["item"]["documentary_evidence"], false);

    let read_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}/forensic-context",
            f.destination_workspace
        ))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.destination_read_token),
        )
        .body(Body::empty())
        .unwrap();
    let (read_status, context) = call(f.router.clone(), read_request).await;
    assert_eq!(read_status, StatusCode::OK);
    assert_eq!(context["items"].as_array().unwrap().len(), 1);
    assert_eq!(context["items_are_documentary_evidence"], false);
    assert_eq!(context["part_of_frozen_analysis_input"], false);

    let scope = AnalysisScope {
        workspace: f.destination_workspace,
        situation,
        analysis,
    };
    let packet = authz::with_auth_digest(digest.clone(), async {
        reasoning_packet::build(&f.secure, scope).await.unwrap()
    })
    .await;
    assert_eq!(packet.current_human_control.imported_context.len(), 1);
    let imported = &packet.current_human_control.imported_context[0];
    assert_eq!(imported.binding_id, binding);
    assert!(imported.quarantine_provenance);
    assert!(!imported.establishes_truth);
    assert!(!imported.documentary_evidence);
    assert!(!imported.current_credentials_upgrade_imported_statement);
    assert!(
        imported.payload_json.is_some(),
        "bounded logical historical state should be available to fresh reasoning"
    );
    assert!(!packet.current_human_control.imported_context_was_part_of_frozen_input);
    assert!(!packet.current_human_control.imported_context_is_documentary_evidence);

    let native_evidence: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.evidence WHERE workspace_id=$1")
            .bind(f.destination_workspace)
            .fetch_one(&f.admin)
            .await
            .unwrap();
    assert_eq!(native_evidence, 0);

    let replay = authz::with_auth_digest(digest.clone(), async {
        forensic_context::bind(
            &f.secure,
            scope,
            f.destination_principal,
            &forensic_context::BindingInput {
                binding_id: binding,
                import_batch_id: batch,
                item_id: imported_item,
                note: Some(
                    "Compare this imported historical item; do not treat it as fact.".into(),
                ),
            },
        )
        .await
        .unwrap()
    })
    .await;
    assert!(replay.replayed);

    let conflict = authz::with_auth_digest(digest, async {
        forensic_context::bind(
            &f.secure,
            scope,
            f.destination_principal,
            &forensic_context::BindingInput {
                binding_id: binding,
                import_batch_id: batch,
                item_id: imported_item,
                note: Some("Change the recorded rationale retroactively.".into()),
            },
        )
        .await
    })
    .await;
    assert!(matches!(
        conflict,
        Err(forensic_context::ForensicContextError::Conflict)
    ));
}

#[tokio::test]
async fn same_package_with_conflicting_declared_origin_fails_closed() {
    let f = fixture().await;
    let package = exported(&f).await;
    let digest = authz::credential_digest(&f.destination_token).unwrap();
    let first = ImportRequest {
        import_batch_id: Uuid::new_v4(),
        declared_origin: "fixture://origin/a".into(),
    };
    authz::with_auth_digest(digest.clone(), async {
        forensic_import::import_untrusted(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            &first,
            &package,
        )
        .await
        .unwrap();
    })
    .await;

    let conflicting = ImportRequest {
        import_batch_id: Uuid::new_v4(),
        declared_origin: "fixture://origin/b".into(),
    };
    let result = authz::with_auth_digest(digest, async {
        forensic_import::import_untrusted(
            &f.secure,
            f.destination_workspace,
            f.destination_principal,
            &conflicting,
            &package,
        )
        .await
    })
    .await;
    assert!(matches!(result, Err(ForensicImportError::Conflict)));
}
