use niccolo_evidence::{authz, situation_store};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Populate the stored representation of material imported before the parser-free MVP.
pub async fn seed(
    admin: &PgPool,
    workspace: Uuid,
    document: Uuid,
    locator: &str,
    fragments: &[(&str, &str)],
) -> Vec<Uuid> {
    let content = if fragments.is_empty() {
        "Historical record with no catalogued fragments".to_owned()
    } else {
        fragments
            .iter()
            .map(|(_, line)| *line)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut tx = admin.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length)
         VALUES($1,$2,'WHATSAPP_EXPORT',$3,$4,$5)",
    )
    .bind(workspace).bind(document).bind(locator)
    .bind(format!("{:x}", Sha256::digest(content.as_bytes())))
    .bind(content.len() as i64)
    .execute(&mut *tx).await.unwrap();
    let mut offset = 0;
    let mut evidence_ids = Vec::with_capacity(fragments.len());
    for (ordinal, (speaker, line)) in fragments.iter().enumerate() {
        let evidence = Uuid::new_v4();
        let end = offset + line.len() as i32;
        sqlx::query(
            "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class)
             VALUES($1,$2,$3,$4,'REPORTED_SPEECH')",
        )
        .bind(workspace).bind(evidence).bind(format!("{locator}#{ordinal}")).bind(line)
        .execute(&mut *tx).await.unwrap();
        sqlx::query(
            "INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label)
             VALUES($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(workspace).bind(document).bind(ordinal as i32).bind(evidence).bind(offset).bind(end).bind(speaker)
        .execute(&mut *tx).await.unwrap();
        evidence_ids.push(evidence);
        offset = end + 1;
    }
    tx.commit().await.unwrap();
    evidence_ids
}

#[allow(dead_code)]
pub async fn attach(
    secure: &PgPool,
    token: &str,
    workspace: Uuid,
    situation: Uuid,
    document: Uuid,
) {
    let principal = authz::authorized_principal(secure, token, workspace, authz::Capability::Write)
        .await
        .unwrap()
        .expect("historical fixture writer");
    let result = authz::with_auth_digest(
        authz::credential_digest(token).unwrap(),
        situation_store::add_source(secure, workspace, principal, situation, document),
    )
    .await
    .unwrap();
    assert!(!result.replayed, "historical fixture source attached twice");
}
