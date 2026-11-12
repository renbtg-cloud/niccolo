#![cfg(feature = "pg-integration")]
use niccolo_evidence::{
    postgres_ingest::{ImportError, ingest_source},
    postgres_repository::{RepositoryError, load_workspace_graph},
    source_import::SourceFormat,
    source_catalog,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::collections::BTreeSet;
use uuid::Uuid;

async fn pools() -> (PgPool, PgPool) {
    let admin = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").expect("admin DB"))
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .max_connections(3)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").expect("service DB"))
        .await
        .unwrap();
    (admin, service)
}
async fn workspace(pool: &PgPool) -> Uuid {
    let w = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES ($1,'source-fixture')")
        .bind(w)
        .execute(pool)
        .await
        .unwrap();
    w
}
const TWO_MESSAGES: &[u8] = b"[02/11/2026, 12:00] Ana: text 1\n[02/11/2026, 12:01] Bob: text 2\n";

#[tokio::test]
async fn attributed_import_and_workspace_graph_are_connected() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    let r = ingest_source(
        &service,
        w,
        id,
        SourceFormat::WhatsappExport,
        "fixture://chat",
        TWO_MESSAGES,
    )
    .await
    .unwrap();
    assert_eq!(r.fragment_ids.len(), 2);
    assert!(!r.replayed);
    assert!(r.graph_revision >= 2);
    let snapshot = load_workspace_graph(&service, w).await.unwrap();
    assert_eq!(r.graph_revision, snapshot.revision());
    let assessed = snapshot.assess(&r.fragment_ids).unwrap();
    assert_eq!(
        assessed.traceable_roots,
        BTreeSet::from_iter(r.fragment_ids.clone())
    );
    assert!(!assessed.independence_established);
    let fragments: Vec<(i32, i32, i32, String, String)> = sqlx::query_as(
        "SELECT ordinal,byte_start,byte_end,speaker_label,e.content \
         FROM niccolo.source_fragments f JOIN niccolo.evidence e USING(workspace_id,evidence_id) \
         WHERE f.workspace_id=$1 AND f.document_id=$2 ORDER BY ordinal",
    )
    .bind(w)
    .bind(id)
    .fetch_all(&admin)
    .await
    .unwrap();
    for (ordinal, start, end, speaker, content) in fragments {
        assert_eq!(ordinal as usize, if speaker == "Ana" { 0 } else { 1 });
        assert_eq!(
            content.as_bytes(),
            &TWO_MESSAGES[start as usize..end as usize]
        );
    }
}
#[tokio::test]
async fn canonical_email_thread_stores_each_sender_as_a_separate_attributed_fragment() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    let raw = include_bytes!("../fixtures/law-firm-mvp/sources/02_full_thread.eml");

    let imported = ingest_source(
        &service,
        w,
        id,
        SourceFormat::Eml,
        "fixture://law-firm/full-thread",
        raw,
    )
    .await
    .unwrap();
    assert_eq!(imported.fragment_ids.len(), 3);

    let stored: Vec<(i32, i32, i32, String, String)> = sqlx::query_as(
        "SELECT f.ordinal,f.byte_start,f.byte_end,f.speaker_label,e.content
           FROM niccolo.source_fragments f
           JOIN niccolo.evidence e USING(workspace_id,evidence_id)
          WHERE f.workspace_id=$1 AND f.document_id=$2
          ORDER BY f.ordinal",
    )
    .bind(w)
    .bind(id)
    .fetch_all(&admin)
    .await
    .unwrap();

    assert_eq!(
        stored
            .iter()
            .map(|(_, _, _, speaker, _)| speaker.as_str())
            .collect::<Vec<_>>(),
        vec![
            "julian.mercer@aurelius.example",
            "adrian.kessler@aurelius.example",
            "adrian.kessler@aurelius.example",
        ]
    );
    for (_, start, end, _, content) in stored {
        assert_eq!(
            content.as_bytes(),
            &raw[start as usize..end as usize]
        );
    }
}

#[tokio::test]
async fn source_bytes_are_retained_exactly_in_the_authoritative_database() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    let raw = b"From: Example <example@example.org>\r\n\r\nOl\xc3\xa1!\r\n";
    let imported = ingest_source(
        &service, w, id, SourceFormat::Eml, "fixture://original", raw
    ).await.unwrap();
    assert!(!imported.replayed);
    let stored: Vec<u8> = sqlx::query_scalar(
        "SELECT original_bytes FROM niccolo.source_payloads WHERE workspace_id=$1 AND document_id=$2",
    ).bind(w).bind(id).fetch_one(&admin).await.unwrap();
    assert_eq!(stored, raw);
    let page = source_catalog::list_sources(&service, w, None, 25).await.unwrap();
    assert_eq!(page.documents.len(), 1);
    assert!(page.documents[0].original_bytes_available);
    let authorized = source_catalog::source_original(&service, w, id).await.unwrap();
    assert_eq!(authorized, Some(("EML".to_owned(), raw.to_vec())));
    let replay = ingest_source(
        &service, w, id, SourceFormat::Eml, "fixture://original", raw
    ).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.graph_revision, imported.graph_revision);
}

#[tokio::test]
async fn original_bytes_are_not_visible_outside_the_authorized_workspace() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    ingest_source(&service, w, id, SourceFormat::Eml, "fixture://isolated", b"From: A\n\none")
        .await.unwrap();
    // No workspace GUC nor bearer digest is bound to this connection.
    let visible: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.source_payloads WHERE workspace_id=$1",
    ).bind(w).fetch_one(&service).await.unwrap();
    assert_eq!(visible, 0);
    let wrong_workspace = workspace(&admin).await;
    let isolated = source_catalog::source_original(&service, wrong_workspace, id)
        .await.unwrap();
    assert!(isolated.is_none());
}

#[tokio::test]
async fn original_source_bytes_cannot_be_modified_after_import() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    ingest_source(&service, w, id, SourceFormat::Eml, "fixture://immutable", b"From: A\n\none")
        .await.unwrap();
    let attempt = sqlx::query(
        "UPDATE niccolo.source_payloads SET original_bytes=$3 WHERE workspace_id=$1 AND document_id=$2",
    ).bind(w).bind(id).bind(&b"tampered"[..]).execute(&admin).await;
    assert_eq!(
        attempt.unwrap_err().as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514")
    );
}

#[tokio::test]
async fn retry_is_idempotent_and_does_not_advance_revision() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    let first = ingest_source(
        &service,
        w,
        id,
        SourceFormat::WhatsappExport,
        "fixture://chat",
        TWO_MESSAGES,
    )
    .await
    .unwrap();
    let again = ingest_source(
        &service,
        w,
        id,
        SourceFormat::WhatsappExport,
        "fixture://chat",
        TWO_MESSAGES,
    )
    .await
    .unwrap();
    assert!(again.replayed);
    assert_eq!(again.fragment_ids, first.fragment_ids);
    assert_eq!(again.graph_revision, first.graph_revision);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_fragments WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(count, 2);
}
#[tokio::test]
async fn changed_same_id_is_rejected_without_writes() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    let original = ingest_source(
        &service,
        w,
        id,
        SourceFormat::Eml,
        "fixture://mail",
        b"From: A\n\none",
    )
    .await
    .unwrap();
    assert!(matches!(
        ingest_source(
            &service,
            w,
            id,
            SourceFormat::Eml,
            "fixture://mail",
            b"From: A\n\ntwo"
        )
        .await,
        Err(ImportError::DocumentConflict)
    ));
    let after = load_workspace_graph(&service, w).await.unwrap();
    assert_eq!(after.revision(), original.graph_revision);
    assert_eq!(
        after.assess(&original.fragment_ids).unwrap().report_count,
        1
    );
}
#[tokio::test]
async fn parse_errors_and_invalid_locator_cause_no_persistent_writes() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    assert!(matches!(
        ingest_source(
            &service,
            w,
            id,
            SourceFormat::Eml,
            "fixture://mail",
            b"From: A\nContent-Transfer-Encoding: base64\n\nabc"
        )
        .await,
        Err(ImportError::Parse(_))
    ));
    assert!(matches!(
        ingest_source(
            &service,
            w,
            id,
            SourceFormat::Eml,
            "bad\nlocator",
            b"From: A\n\ntext"
        )
        .await,
        Err(ImportError::Locator)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_documents WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn same_document_identifier_is_isolated_per_workspace() {
    let (admin, service) = pools().await;
    let (a, b) = (workspace(&admin).await, workspace(&admin).await);
    let shared = Uuid::new_v4();
    let first = ingest_source(
        &service,
        a,
        shared,
        SourceFormat::Eml,
        "fixture://1",
        b"From: A\n\none",
    )
    .await
    .unwrap();
    let second = ingest_source(
        &service,
        b,
        shared,
        SourceFormat::Eml,
        "fixture://2",
        b"From: B\n\ntwo",
    )
    .await
    .unwrap();
    let ga = load_workspace_graph(&service, a).await.unwrap();
    let gb = load_workspace_graph(&service, b).await.unwrap();
    assert!(matches!(
        ga.assess(&second.fragment_ids),
        Err(RepositoryError::InvisibleReport(_))
    ));
    assert!(matches!(
        gb.assess(&first.fragment_ids),
        Err(RepositoryError::InvisibleReport(_))
    ));
}
#[tokio::test]
async fn concurrent_replays_produce_exactly_one_import() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let id = Uuid::new_v4();
    let mut tasks = Vec::new();
    for _ in 0..24 {
        let db = service.clone();
        tasks.push(tokio::spawn(async move {
            ingest_source(
                &db,
                w,
                id,
                SourceFormat::WhatsappExport,
                "fixture://collision",
                TWO_MESSAGES,
            )
            .await
            .unwrap()
        }));
    }
    let mut first = 0;
    let mut ids = None;
    for task in tasks {
        let r = task.await.unwrap();
        if !r.replayed {
            first += 1;
        }
        if let Some(previous) = &ids {
            assert_eq!(previous, &r.fragment_ids);
        }
        ids = Some(r.fragment_ids);
    }
    assert_eq!(first, 1);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM niccolo.source_fragments WHERE workspace_id=$1")
            .bind(w)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(count, 2);
}
#[tokio::test]
async fn rejects_privileged_connection_and_does_not_leak_scope() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    assert!(matches!(
        ingest_source(
            &admin,
            w,
            Uuid::new_v4(),
            SourceFormat::Eml,
            "fixture://mail",
            b"From: X\n\ntext"
        )
        .await,
        Err(ImportError::PrivilegedRole)
    ));
    let id = Uuid::new_v4();
    let _ = ingest_source(
        &service,
        w,
        id,
        SourceFormat::Eml,
        "fixture://mail",
        b"From: X\n\ntext",
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.source_documents")
        .fetch_one(&service)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn replay_detects_tampered_fragment_and_fails_closed() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let doc = Uuid::new_v4();
    let first = ingest_source(
        &service,
        w,
        doc,
        SourceFormat::Eml,
        "fixture://mail",
        b"From: X\n\ntext",
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE niccolo.evidence SET content='TAMPERED' WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(w)
    .bind(first.fragment_ids[0])
    .execute(&admin)
    .await
    .unwrap();
    assert!(matches!(
        ingest_source(
            &service,
            w,
            doc,
            SourceFormat::Eml,
            "fixture://mail",
            b"From: X\n\ntext"
        )
        .await,
        Err(ImportError::DocumentConflict)
    ));
}

#[tokio::test]
async fn two_messages_in_one_document_are_flagged_as_shared_source_context() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let doc = Uuid::new_v4();
    let imported = ingest_source(
        &service,
        w,
        doc,
        SourceFormat::WhatsappExport,
        "fixture://one-chat",
        TWO_MESSAGES,
    )
    .await
    .unwrap();
    let s = load_workspace_graph(&service, w).await.unwrap();
    let a = s.assess(&imported.fragment_ids).unwrap();
    // Distinct message IDs are still distinct provenance roots, but the
    // underlying uploaded artifact is shared and must be reported separately.
    assert!(a.shared_origins.is_empty());
    assert_eq!(
        a.document_ids_per_report,
        vec![BTreeSet::from([doc]), BTreeSet::from([doc])]
    );
    assert_eq!(a.shared_source_documents.len(), 1);
    assert_eq!(
        a.shared_source_documents[0].documents,
        BTreeSet::from([doc])
    );
    assert!(!a.independence_established);
}

#[tokio::test]
async fn separate_documents_have_no_shared_document_marker() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let first = ingest_source(
        &service,
        w,
        Uuid::new_v4(),
        SourceFormat::Eml,
        "fixture://one",
        b"From: A\n\none",
    )
    .await
    .unwrap();
    let second = ingest_source(
        &service,
        w,
        Uuid::new_v4(),
        SourceFormat::Eml,
        "fixture://two",
        b"From: B\n\ntwo",
    )
    .await
    .unwrap();
    let s = load_workspace_graph(&service, w).await.unwrap();
    let a = s
        .assess(&[first.fragment_ids[0], second.fragment_ids[0]])
        .unwrap();
    assert!(a.shared_source_documents.is_empty());
    assert!(!a.independence_established);
}

#[tokio::test]
async fn derived_copy_inherits_source_document_context() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let doc = Uuid::new_v4();
    let imported = ingest_source(
        &service,
        w,
        doc,
        SourceFormat::WhatsappExport,
        "fixture://one-chat",
        TWO_MESSAGES,
    )
    .await
    .unwrap();
    let copy = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES ($1,$2,'fixture','synthetic','COPY')")
       .bind(w).bind(copy).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) VALUES ($1,$2,$3,'COPIED_FROM')")
       .bind(w).bind(copy).bind(imported.fragment_ids[0]).execute(&admin).await.unwrap();
    let s = load_workspace_graph(&service, w).await.unwrap();
    let a = s.assess(&[copy, imported.fragment_ids[1]]).unwrap();
    assert_eq!(a.shared_source_documents.len(), 1);
    assert_eq!(
        a.shared_source_documents[0].documents,
        BTreeSet::from([doc])
    );
}

#[tokio::test]
async fn direct_sql_cannot_forge_out_of_bounds_or_overlapping_source_spans() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let doc = Uuid::new_v4();
    let r = ingest_source(
        &service,
        w,
        doc,
        SourceFormat::WhatsappExport,
        "fixture://integrity",
        TWO_MESSAGES,
    )
    .await
    .unwrap();
    let forged = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES ($1,$2,'fixture','forged','COPY')")
      .bind(w).bind(forged).execute(&admin).await.unwrap();
    let too_long=sqlx::query(
      "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES ($1,$2,100,$3,0,4000000,'Forged')"
    ).bind(w).bind(doc).bind(forged).execute(&admin).await;
    assert_eq!(
        too_long
            .unwrap_err()
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    let overlapping=sqlx::query(
      "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES ($1,$2,101,$3,0,10,'Forged')"
    ).bind(w).bind(doc).bind(forged).execute(&admin).await;
    assert_eq!(
        overlapping
            .unwrap_err()
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    let revision_before = load_workspace_graph(&service, w).await.unwrap().revision();
    let changed=sqlx::query("UPDATE niccolo.source_documents SET byte_length=byte_length+1 WHERE workspace_id=$1 AND document_id=$2")
       .bind(w).bind(doc).execute(&admin).await;
    assert_eq!(
        changed
            .unwrap_err()
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    assert_eq!(
        load_workspace_graph(&service, w).await.unwrap().revision(),
        revision_before
    );
    assert_eq!(r.fragment_ids.len(), 2);
}

#[tokio::test]
async fn restricted_role_cannot_mutate_original_document_metadata_even_with_row_lock_grant() {
    let (admin, service) = pools().await;
    let w = workspace(&admin).await;
    let doc = Uuid::new_v4();
    ingest_source(
        &service,
        w,
        doc,
        SourceFormat::Eml,
        "fixture://meta",
        b"From: A\n\ntext",
    )
    .await
    .unwrap();
    let mut tx = service.begin().await.unwrap();
    sqlx::query_scalar::<_, String>("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let err=sqlx::query("UPDATE niccolo.source_documents SET byte_length=3 WHERE workspace_id=$1 AND document_id=$2")
       .bind(w).bind(doc).execute(&mut *tx).await.unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn ninety_six_parallel_distinct_imports_preserve_workspace_isolation() {
    let (admin, service) = pools().await;
    let mut workspaces = Vec::new();
    for _ in 0..8 {
        workspaces.push(workspace(&admin).await);
    }
    let mut tasks = Vec::new();
    for n in 0..96 {
        let pool = service.clone();
        let w = workspaces[n % workspaces.len()];
        tasks.push(tokio::spawn(async move {
            let r = ingest_source(
                &pool,
                w,
                Uuid::new_v4(),
                SourceFormat::Eml,
                "fixture://concurrent",
                b"From: Test\n\nsynthetic evidence",
            )
            .await
            .unwrap();
            assert!(!r.replayed);
            (w, r.fragment_ids[0])
        }));
    }
    let mut by_workspace = std::collections::BTreeMap::<Uuid, Vec<Uuid>>::new();
    for t in tasks {
        let (w, id) = t.await.unwrap();
        by_workspace.entry(w).or_default().push(id);
    }
    for (w, ids) in by_workspace {
        assert_eq!(ids.len(), 12);
        let snapshot = load_workspace_graph(&service, w).await.unwrap();
        assert_eq!(snapshot.assess(&ids).unwrap().roots_per_report.len(), 12);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM niccolo.source_fragments WHERE workspace_id=$1",
        )
        .bind(w)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(count, 12);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM niccolo.source_fragments")
            .fetch_one(&service)
            .await
            .unwrap(),
        0
    );
}
