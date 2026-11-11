#![cfg(feature = "pg-integration")]
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz, situation_forensic_export};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

async fn pools() -> (PgPool, PgPool) {
    let admin = PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    (admin, secure)
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[tokio::test]
async fn forensic_export_is_deterministic_scoped_and_preserves_original_bytes() {
    let (admin, secure) = pools().await;
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    let doc = Uuid::new_v4();
    let missing_doc = Uuid::new_v4();
    let referenced_doc = Uuid::new_v4();
    let collection_doc = Uuid::new_v4();
    let document_only_doc = Uuid::new_v4();
    let ancestor_doc = Uuid::new_v4();
    let question_doc = Uuid::new_v4();
    let evidence = Uuid::new_v4();
    let missing_evidence = Uuid::new_v4();
    let referenced_evidence = Uuid::new_v4();
    let collection_evidence = Uuid::new_v4();
    let ancestor_evidence = Uuid::new_v4();
    let question_evidence = Uuid::new_v4();
    let question = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let person = Uuid::new_v4();
    let original = b"Alice: exact retained forensic bytes\n".to_vec();
    let missing_original_declared = b"Bob: metadata exists but retained bytes are unavailable\n";
    let referenced_original =
        b"Carol: source used by analysis but not directly attached\n".to_vec();
    let collection_original =
        b"Dana: source referenced only through collection provenance\n".to_vec();
    let document_only_original =
        b"Eve: document referenced by collection provenance without a fragment\n".to_vec();
    let ancestor_original =
        b"Frank: original source behind derived analysis evidence\n".to_vec();
    let question_original =
        b"Grace: source linked only through an investigative question\n".to_vec();

    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'forensic-export')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(principal)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'Investigate the records without upgrading assertions to facts',$3,1)")
        .bind(workspace).bind(situation).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'Investigate the records without upgrading assertions to facts',$3,$4,'CREATED')")
        .bind(workspace).bind(situation).bind("a".repeat(64)).bind(principal).execute(&admin).await.unwrap();

    for (document, locator, bytes, evidence_id) in [
        (doc, "fixture://retained", original.as_slice(), evidence),
        (
            missing_doc,
            "fixture://missing-original",
            missing_original_declared.as_slice(),
            missing_evidence,
        ),
    ] {
        sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT',$3,$4,$5)")
            .bind(workspace).bind(document).bind(locator).bind(sha(bytes)).bind(bytes.len() as i64).execute(&admin).await.unwrap();
        sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class,admissible) VALUES($1,$2,$3,$4,'DIRECT_OBSERVATION',true)")
            .bind(workspace).bind(evidence_id).bind(locator).bind(String::from_utf8_lossy(bytes).to_string()).execute(&admin).await.unwrap();
        sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,$4,$5)")
            .bind(workspace).bind(document).bind(evidence_id).bind(bytes.len() as i32).bind(if document==doc {"Alice"} else {"Bob"}).execute(&admin).await.unwrap();
        sqlx::query("INSERT INTO niccolo.situation_sources(workspace_id,situation_id,document_id,added_by_principal_id) VALUES($1,$2,$3,$4)")
            .bind(workspace).bind(situation).bind(document).bind(principal).execute(&admin).await.unwrap();
        sqlx::query("INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id) VALUES($1,$2,1,$3)")
            .bind(workspace).bind(situation).bind(document).execute(&admin).await.unwrap();
    }
    sqlx::query("INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)")
        .bind(workspace).bind(doc).bind(&original).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','fixture://analysis-referenced',$3,$4)")
        .bind(workspace).bind(referenced_doc).bind(sha(&referenced_original)).bind(referenced_original.len() as i64).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class,admissible) VALUES($1,$2,'fixture://analysis-referenced',$3,'DIRECT_OBSERVATION',true)")
        .bind(workspace).bind(referenced_evidence).bind(String::from_utf8_lossy(&referenced_original).to_string()).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,$4,'Carol')")
        .bind(workspace).bind(referenced_doc).bind(referenced_evidence).bind(referenced_original.len() as i32).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)")
        .bind(workspace).bind(referenced_doc).bind(&referenced_original).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'Forensic export completeness',$5)")
        .bind(workspace).bind(situation).bind(analysis).bind("a".repeat(64)).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5)")
        .bind(workspace).bind(situation).bind(analysis).bind(referenced_evidence).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','fixture://derived-ancestor',$3,$4)")
        .bind(workspace).bind(ancestor_doc).bind(sha(&ancestor_original)).bind(ancestor_original.len() as i64).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class,admissible) VALUES($1,$2,'fixture://derived-ancestor',$3,'DIRECT_OBSERVATION',true)")
        .bind(workspace).bind(ancestor_evidence).bind(String::from_utf8_lossy(&ancestor_original).to_string()).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,$4,'Frank')")
        .bind(workspace).bind(ancestor_doc).bind(ancestor_evidence).bind(ancestor_original.len() as i32).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)")
        .bind(workspace).bind(ancestor_doc).bind(&ancestor_original).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) VALUES($1,$2,$3,'DERIVED_FROM')")
        .bind(workspace).bind(referenced_evidence).bind(ancestor_evidence).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','fixture://question-evidence-only',$3,$4)")
        .bind(workspace).bind(question_doc).bind(sha(&question_original)).bind(question_original.len() as i64).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class,admissible) VALUES($1,$2,'fixture://question-evidence-only',$3,'DIRECT_OBSERVATION',true)")
        .bind(workspace).bind(question_evidence).bind(String::from_utf8_lossy(&question_original).to_string()).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,$4,'Grace')")
        .bind(workspace).bind(question_doc).bind(question_evidence).bind(question_original.len() as i32).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)")
        .bind(workspace).bind(question_doc).bind(&question_original).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question,status) VALUES($1,$2,'What does the question-only source change?','INVESTIGATING')")
        .bind(workspace).bind(question).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,'Question is material to the analysis',$5)")
        .bind(workspace).bind(situation).bind(analysis).bind(question).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.question_evidence_references(workspace_id,reference_id,question_id,evidence_id,relation,human_reason,actor_principal_id) VALUES($1,$2,$3,$4,'REFERENCES','Human linked the source to this question',$5)")
        .bind(workspace).bind(Uuid::new_v4()).bind(question).bind(question_evidence).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.question_feedback_events(workspace_id,event_id,question_id,response,human_reason,actor_principal_id) VALUES($1,$2,$3,'UNKNOWN','Still unresolved after reviewing the linked source',$4)")
        .bind(workspace).bind(Uuid::new_v4()).bind(question).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','fixture://collection-provenance-only',$3,$4)")
        .bind(workspace).bind(collection_doc).bind(sha(&collection_original)).bind(collection_original.len() as i64).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class,admissible) VALUES($1,$2,'fixture://collection-provenance-only',$3,'DIRECT_OBSERVATION',true)")
        .bind(workspace).bind(collection_evidence).bind(String::from_utf8_lossy(&collection_original).to_string()).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,$4,'Dana')")
        .bind(workspace).bind(collection_doc).bind(collection_evidence).bind(collection_original.len() as i32).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)")
        .bind(workspace).bind(collection_doc).bind(&collection_original).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_collection_events(workspace_id,situation_id,analysis_id,event_id,actor_label,actor_role,operation,input_evidence_id,method,assertion_kind,qualification,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,'CI collector','COLLECTOR','Referenced evidence during collection',$5,'fixture collection event','DOCUMENTARY_OBSERVATION','OBSERVED','Exercises noncanonical evidence-id column discovery',$6)")
        .bind(workspace).bind(situation).bind(analysis).bind(Uuid::new_v4()).bind(collection_evidence).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','fixture://collection-document-only',$3,$4)")
        .bind(workspace).bind(document_only_doc).bind(sha(&document_only_original)).bind(document_only_original.len() as i64).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_payloads(workspace_id,document_id,original_bytes) VALUES($1,$2,$3)")
        .bind(workspace).bind(document_only_doc).bind(&document_only_original).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_collection_events(workspace_id,situation_id,analysis_id,event_id,actor_label,actor_role,operation,input_document_id,method,assertion_kind,qualification,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,'CI collector','COLLECTOR','Referenced document during collection',$5,'fixture document collection event','DOCUMENTARY_OBSERVATION','OBSERVED','Exercises noncanonical document-id column discovery',$6)")
        .bind(workspace).bind(situation).bind(analysis).bind(Uuid::new_v4()).bind(document_only_doc).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.identity_subjects(workspace_id,person_id,display_label) VALUES($1,$2,'Alice Example')")
        .bind(workspace).bind(person).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.identity_events(workspace_id,event_id,document_id,speaker_label,operation,person_id,human_reason,actor_principal_id) VALUES($1,$2,$3,'Alice','ASSIGN',$4,'Human explicitly mapped this label',$5)")
        .bind(workspace).bind(Uuid::new_v4()).bind(doc).bind(person).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text) VALUES($1,$2,$3,$4,'USER_REPORTED','Human assertion; not documentary evidence')")
        .bind(workspace).bind(situation).bind(Uuid::new_v4()).bind(principal).execute(&admin).await.unwrap();

    let other_workspace = Uuid::new_v4();
    let other_situation = Uuid::new_v4();
    let other_principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'must-not-leak')")
        .bind(other_workspace)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(other_principal)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'CROSS_WORKSPACE_SECRET_SENTINEL',$3,0)")
        .bind(other_workspace).bind(other_situation).bind(other_principal).execute(&admin).await.unwrap();

    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    let digest = authz::credential_digest(&token).unwrap();
    let first = authz::with_auth_digest(
        digest.clone(),
        situation_forensic_export::build(&secure, workspace, situation),
    )
    .await
    .unwrap();
    let second = authz::with_auth_digest(
        digest.clone(),
        situation_forensic_export::build(&secure, workspace, situation),
    )
    .await
    .unwrap();

    assert_eq!(first, second);

    let response = api::router(secure.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/workspaces/{workspace}/situations/{situation}/forensic-export"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let http_export: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        http_export["manifest"]["root_sha256"],
        first.manifest.root_sha256
    );
    assert_eq!(http_export["structural_validity_establishes_truth"], false);
    assert_eq!(first.format, "niccolo.situation-forensic-export");
    assert_eq!(first.format_version, 1);
    assert!(!first.structural_validity_establishes_truth);
    assert!(!first.included_model_output_is_documentary_evidence);
    assert!(!first.imported_state_should_be_trusted_automatically);
    assert_eq!(first.original_source_payloads.len(), 6);
    let retained = first
        .original_source_payloads
        .iter()
        .find(|p| p.document_id == doc)
        .unwrap();
    assert_eq!(retained.declared_sha256, sha(&original));
    assert_eq!(
        retained.bytes_hex,
        original
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert!(
        first
            .original_source_payloads
            .iter()
            .any(|p| p.document_id == referenced_doc
                && p.declared_sha256 == sha(&referenced_original))
    );
    assert!(
        first
            .original_source_payloads
            .iter()
            .any(|p| p.document_id == collection_doc
                && p.declared_sha256 == sha(&collection_original))
    );
    assert!(
        first
            .original_source_payloads
            .iter()
            .any(|p| p.document_id == document_only_doc
                && p.declared_sha256 == sha(&document_only_original))
    );
    assert!(
        first
            .original_source_payloads
            .iter()
            .any(|p| p.document_id == ancestor_doc
                && p.declared_sha256 == sha(&ancestor_original))
    );
    assert!(
        first
            .original_source_payloads
            .iter()
            .any(|p| p.document_id == question_doc
                && p.declared_sha256 == sha(&question_original))
    );
    assert_eq!(first.missing_original_document_ids, vec![missing_doc]);
    assert!(
        first
            .logical_tables
            .get("identity_subjects")
            .is_some_and(|rows| rows.len() == 1)
    );
    assert!(
        first
            .logical_tables
            .get("identity_events")
            .is_some_and(|rows| rows.len() == 1)
    );
    assert!(
        first
            .logical_tables
            .get("evidence")
            .is_some_and(|rows| rows.len() == 6)
    );
    assert!(
        first
            .logical_tables
            .get("investigative_questions")
            .is_some_and(|rows| rows.len() == 1)
    );
    assert!(
        first
            .logical_tables
            .get("question_evidence_references")
            .is_some_and(|rows| rows.len() == 1)
    );
    assert!(
        first
            .logical_tables
            .get("question_feedback_events")
            .is_some_and(|rows| rows.len() == 1)
    );
    assert!(
        first
            .manifest
            .entries
            .iter()
            .any(|entry| entry.path == format!("sources/{doc}/original")
                && entry.sha256 == sha(&original))
    );
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(serialized.contains("fixture://analysis-referenced"));
    assert!(serialized.contains("fixture://collection-provenance-only"));
    assert!(serialized.contains("fixture://collection-document-only"));
    assert!(serialized.contains("fixture://derived-ancestor"));
    assert!(serialized.contains("fixture://question-evidence-only"));
    assert!(!serialized.contains("CROSS_WORKSPACE_SECRET_SENTINEL"));

    let no_digest = situation_forensic_export::build(&secure, workspace, situation).await;
    assert!(matches!(
        no_digest,
        Err(situation_forensic_export::ForensicExportError::Invisible)
    ));
    let privileged = situation_forensic_export::build(&admin, workspace, situation).await;
    assert!(matches!(
        privileged,
        Err(situation_forensic_export::ForensicExportError::PrivilegedRole)
    ));

    let before = first.manifest.root_sha256.clone();
    sqlx::query("INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text) VALUES($1,$2,$3,$4,'INVESTIGATIVE_PREFERENCE','New recorded state must change the forensic manifest')")
        .bind(workspace).bind(situation).bind(Uuid::new_v4()).bind(principal).execute(&admin).await.unwrap();
    let changed = authz::with_auth_digest(
        digest,
        situation_forensic_export::build(&secure, workspace, situation),
    )
    .await
    .unwrap();
    assert_ne!(before, changed.manifest.root_sha256);
}
