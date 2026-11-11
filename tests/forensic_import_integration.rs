#![cfg(feature = "pg-integration")]

use niccolo_evidence::{
    authz,
    forensic_import::{self, ForensicImportError, ImportRequest},
    situation_forensic_export,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
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

    Fixture {
        admin,
        secure,
        source_workspace,
        source_situation,
        source_token,
        destination_workspace,
        destination_principal,
        destination_token,
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

    let native_situations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situations WHERE workspace_id=$1",
    )
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
