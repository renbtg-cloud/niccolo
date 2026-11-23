#![cfg(feature = "pg-integration")]
use niccolo_evidence::artifacts::{
    self, ArtifactError, ArtifactInput, ArtifactRelationshipInput, RepresentationInput,
    StorageReferenceInput,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

struct Seed {
    workspace: Uuid,
    other_workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
}

async fn pools() -> (PgPool, PgPool) {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_NEXT_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let app = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_NEXT_APP_URL").unwrap())
        .await
        .unwrap();
    (admin, app)
}

async fn seed(admin: &PgPool) -> Seed {
    let seed = Seed {
        workspace: Uuid::new_v4(),
        other_workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        principal: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(seed.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label)
         VALUES($1,'parser-free'),($2,'isolated')",
    )
    .bind(seed.workspace)
    .bind(seed.other_workspace)
    .execute(admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situations
         (workspace_id,situation_id,focus,created_by_principal_id)
         VALUES($1,$2,'Understand explicitly supplied accounts',$3)",
    )
    .bind(seed.workspace)
    .bind(seed.situation)
    .bind(seed.principal)
    .execute(admin)
    .await
    .unwrap();
    seed
}

fn artifact(id: Uuid, actor: Uuid, label: &str) -> ArtifactInput {
    ArtifactInput {
        artifact_id: id,
        artifact_type: "VIDEO".into(),
        label: label.into(),
        declared_mime_type: Some("video/mp4".into()),
        declared_byte_length: Some(18_000_000_000),
        declared_sha256: None,
        recorded_by: actor,
    }
}

#[tokio::test]
async fn external_artifacts_remain_useful_without_bytes_or_parser_output() {
    let (admin, app) = pools().await;
    let seed = seed(&admin).await;
    let video = Uuid::new_v4();
    let transcript = Uuid::new_v4();

    assert!(
        !artifacts::record_for_situation(
            &app,
            seed.workspace,
            seed.situation,
            &artifact(video, seed.principal, "meeting.mp4"),
        )
        .await
        .unwrap()
    );
    assert!(
        artifacts::record_for_situation(
            &app,
            seed.workspace,
            seed.situation,
            &artifact(video, seed.principal, "meeting.mp4"),
        )
        .await
        .unwrap()
    );
    artifacts::record_for_situation(
        &app,
        seed.workspace,
        seed.situation,
        &ArtifactInput {
            artifact_id: transcript,
            artifact_type: "TRANSCRIPT_FILE".into(),
            label: "meeting-transcript.md".into(),
            declared_mime_type: Some("text/markdown".into()),
            declared_byte_length: None,
            declared_sha256: None,
            recorded_by: seed.principal,
        },
    )
    .await
    .unwrap();

    artifacts::record_storage_reference(
        &app,
        seed.workspace,
        seed.situation,
        video,
        &StorageReferenceInput {
            storage_reference_id: Uuid::new_v4(),
            access_event_id: Uuid::new_v4(),
            storage_type: "S3".into(),
            locator: "s3://client-video/meeting.mp4".into(),
            provider: Some("AWS".into()),
            environment_label: Some("client production".into()),
            resolver_profile: None,
            notes: Some("Location supplied by the user".into()),
            access_status: "NONE".into(),
            access_note: Some("Niccolò has no access".into()),
            recorded_by: seed.principal,
        },
    )
    .await
    .unwrap();

    artifacts::record_representation(
        &app,
        seed.workspace,
        seed.situation,
        video,
        &RepresentationInput {
            representation_id: Uuid::new_v4(),
            representation_type: "HUMAN_DESCRIPTION".into(),
            producer_type: "USER".into(),
            producer_label: Some("Renato".into()),
            production_method: Some("described while watching externally".into()),
            scope_locator: "00:13:20-00:13:45".into(),
            content: "João appears to leave the room at approximately 00:13:24.".into(),
            completeness_status: "PARTIAL".into(),
            epistemic_qualification: Some("human report, not machine verified".into()),
            limitations: Some("only the relevant interval was described".into()),
            recorded_by: seed.principal,
        },
    )
    .await
    .unwrap();

    artifacts::record_relationship(
        &app,
        seed.workspace,
        seed.situation,
        &ArtifactRelationshipInput {
            relationship_id: Uuid::new_v4(),
            subject_artifact_id: transcript,
            object_artifact_id: video,
            relationship_type: "CLAIMED_TO_BE_TRANSCRIPTION_OF".into(),
            qualification: Some("relationship declared by the user".into()),
            recorded_by: seed.principal,
        },
    )
    .await
    .unwrap();

    let visible = artifacts::list_for_situation(&app, seed.workspace, seed.situation)
        .await
        .unwrap();
    assert_eq!(visible.len(), 2);
    assert!(
        visible
            .iter()
            .all(|item| !item.source_bytes_stored && !item.artifact_was_inspected)
    );
    assert!(
        artifacts::list_for_situation(&app, seed.other_workspace, seed.situation)
            .await
            .unwrap()
            .is_empty()
    );

    let payloads: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.source_payloads
         WHERE workspace_id=$1 AND document_id IN ($2,$3)",
    )
    .bind(seed.workspace)
    .bind(video)
    .bind(transcript)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(payloads, 0);

    let revision: i64 = sqlx::query_scalar(
        "SELECT current_revision FROM niccolo.situations
         WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(seed.workspace)
    .bind(seed.situation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(revision, 5);
    let frozen: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM niccolo.situation_revision_artifacts
             WHERE workspace_id=$1 AND situation_id=$2 AND revision=5),
           (SELECT count(*) FROM niccolo.situation_revision_artifact_storage_references
             WHERE workspace_id=$1 AND situation_id=$2 AND revision=5),
           (SELECT count(*) FROM niccolo.situation_revision_artifact_representations
             WHERE workspace_id=$1 AND situation_id=$2 AND revision=5),
           (SELECT count(*) FROM niccolo.situation_revision_artifact_relationships
             WHERE workspace_id=$1 AND situation_id=$2 AND revision=5)",
    )
    .bind(seed.workspace)
    .bind(seed.situation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(frozen, (2, 1, 1, 1));
    let distinct_fingerprints: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT fingerprint) FROM niccolo.situation_revisions
         WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(seed.workspace)
    .bind(seed.situation)
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(distinct_fingerprints, 5);
}

#[tokio::test]
async fn artifact_identity_replay_cannot_change_declared_metadata() {
    let (admin, app) = pools().await;
    let seed = seed(&admin).await;
    let id = Uuid::new_v4();
    artifacts::record_for_situation(
        &app,
        seed.workspace,
        seed.situation,
        &artifact(id, seed.principal, "first label"),
    )
    .await
    .unwrap();
    let changed = artifacts::record_for_situation(
        &app,
        seed.workspace,
        seed.situation,
        &artifact(id, seed.principal, "changed label"),
    )
    .await;
    assert!(matches!(changed, Err(ArtifactError::Conflict)));
}
