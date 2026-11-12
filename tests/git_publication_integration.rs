#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz, git_publication};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    router: Router,
    admin: sqlx::PgPool,
    secure: sqlx::PgPool,
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
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'git-publication')")
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
        router: api::router(secure.clone()),
        admin,
        secure,
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

fn request(method: &str, uri: String, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}

async fn published_analysis(f: &Fixture) -> (Uuid, Uuid) {
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let base = format!("/v1/workspaces/{}/situations/{situation}", f.workspace);
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                base.clone(),
                &f.writer,
                Some(json!({"focus":"Plan portable publication without claiming it happened"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let analysis_path = format!("{base}/analysis-snapshots/{analysis}");
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                analysis_path.clone(),
                &f.writer,
                Some(json!({"goal":"Produce a portable published analysis"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("{analysis_path}/output"),
                &f.writer,
                Some(json!({"answer_text":"Recorded answer","coverage_note":"Bounded fixture"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    (situation, analysis)
}

#[tokio::test]
async fn publication_planning_is_explicit_idempotent_and_never_claims_remote_success() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();

    let target_uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    let target_body = json!({
        "repository_owner":"example-owner",
        "repository_name":"niccolo-history",
        "ref_name":"main",
        "repository_visibility":"PRIVATE",
        "sensitive_exports_allowed":false
    });
    let (status, registered) = call(
        f.router.clone(),
        request(
            "POST",
            target_uri.clone(),
            &f.writer,
            Some(target_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(registered["provider_connection_verified"], false);
    assert_eq!(registered["provider_secret_stored"], false);
    assert_eq!(registered["registration_publishes_nothing"], true);

    let (status, replay) = call(
        f.router.clone(),
        request("POST", target_uri, &f.writer, Some(target_body)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let analysis_path = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    let (status, preview) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{analysis_path}/git-publication-preview?target_id={target}"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["scope_kind"], "STANDARD_PRESENTATION");
    assert_eq!(preview["includes_examined_source_text"], false);
    assert_eq!(preview["includes_original_uploads"], false);
    assert_eq!(preview["remote_publish_performed"], false);
    assert_eq!(preview["requires_transient_provider_authorization"], true);
    assert_eq!(preview["ready_for_remote_publish"], true);
    assert_eq!(preview["package_sha256"].as_str().unwrap().len(), 64);
    assert!(preview["file_count"].as_u64().unwrap() > 0);
    assert!(preview["total_text_bytes"].as_u64().unwrap() > 0);

    let intent_uri = format!("{analysis_path}/git-publication-intents/{publication}");
    let (status, recorded) = call(
        f.router.clone(),
        request(
            "POST",
            intent_uri.clone(),
            &f.writer,
            Some(json!({"target_id":target})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recorded["remote_publish_performed"], false);
    assert_eq!(
        recorded["requires_provider_authorization_before_publish"],
        true
    );
    assert_eq!(
        recorded["intent"]["publication_state"],
        "AWAITING_REMOTE_AUTHORIZATION"
    );
    assert_eq!(
        recorded["intent"]["package_sha256"],
        preview["package_sha256"]
    );

    let (status, replayed) = call(
        f.router.clone(),
        request(
            "POST",
            intent_uri,
            &f.writer,
            Some(json!({"target_id":target})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);

    let (status, listed) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{analysis_path}/git-publication-intents?limit=25"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["remote_publish_performed"], false);
    assert_eq!(listed["intents"].as_array().unwrap().len(), 1);
    assert_eq!(
        listed["intents"][0]["publication_id"],
        publication.to_string()
    );
}

#[tokio::test]
async fn remote_event_history_is_scoped_bounded_and_secret_free() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace),
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"event-ledger",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                }))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let base = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("{base}/git-publication-intents/{publication}"),
                &f.writer,
                Some(json!({"target_id":target})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let digest = authz::credential_digest(&f.writer).unwrap();
    for class in ["TRANSPORT", "PROVIDER_REJECTED"] {
        let event = git_publication::RemoteEventInput {
            event_id: Uuid::new_v4(),
            publication_id: publication,
            kind: git_publication::RemoteEventKind::PublishFailed,
            remote_commit_sha: None,
            observed_ref: None,
            failure_class: Some(class.into()),
        };
        authz::with_auth_digest(digest.clone(), async {
            git_publication::record_remote_event(
                &f.secure,
                f.workspace,
                f.writer_principal,
                &event,
            )
            .await
            .unwrap()
        })
        .await;
    }

    let (status, body) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{base}/git-publication-intents/{publication}/remote-events?limit=1"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["events"].as_array().unwrap().len(), 1);
    assert_eq!(body["provider_secret_stored"], false);
    assert_eq!(body["event_payloads_include_provider_response_bodies"], false);
    let event = &body["events"][0];
    assert!(event.get("token").is_none());
    assert!(event.get("provider_response").is_none());

    let wrong_analysis = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "GET",
                format!(
                    "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{wrong_analysis}/git-publication-intents/{publication}/remote-events?limit=25",
                    f.workspace
                ),
                &f.reader,
                None,
            ),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn remote_status_changes_only_after_append_only_verified_observation() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();
    let target_uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                target_uri,
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"history",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                }))
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let analysis_path = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("{analysis_path}/git-publication-intents/{publication}"),
                &f.writer,
                Some(json!({"target_id":target})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let digest = authz::credential_digest(&f.writer).unwrap();
    let push_event = Uuid::new_v4();
    let pushed = git_publication::RemoteEventInput {
        event_id: push_event,
        publication_id: publication,
        kind: git_publication::RemoteEventKind::PushAccepted,
        remote_commit_sha: Some("a".repeat(40)),
        observed_ref: None,
        failure_class: None,
    };
    let first = authz::with_auth_digest(digest.clone(), async {
        git_publication::record_remote_event(&f.secure, f.workspace, f.writer_principal, &pushed)
            .await
            .unwrap()
    })
    .await;
    assert!(!first.1);
    let replay = authz::with_auth_digest(digest.clone(), async {
        git_publication::record_remote_event(&f.secure, f.workspace, f.writer_principal, &pushed)
            .await
            .unwrap()
    })
    .await;
    assert!(replay.1);

    let status_uri = format!("{analysis_path}/git-publication-intents/{publication}/remote-status");
    let (status, body) = call(
        f.router.clone(),
        request("GET", status_uri.clone(), &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "PUSHED_UNVERIFIED");
    assert_eq!(body["remote_publish_verified"], false);
    assert_eq!(body["last_event_kind"], "PUSH_ACCEPTED");
    assert!(body["last_event_at"].as_str().is_some());
    assert!(body["last_failure_class"].is_null());

    let verified = git_publication::RemoteEventInput {
        event_id: Uuid::new_v4(),
        publication_id: publication,
        kind: git_publication::RemoteEventKind::RefVerified,
        remote_commit_sha: Some("a".repeat(40)),
        observed_ref: Some("main".into()),
        failure_class: None,
    };
    authz::with_auth_digest(digest, async {
        git_publication::record_remote_event(&f.secure, f.workspace, f.writer_principal, &verified)
            .await
            .unwrap()
    })
    .await;

    let (status, body) = call(
        f.router.clone(),
        request("GET", status_uri, &f.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "VERIFIED_PUBLISHED");
    assert_eq!(body["remote_publish_verified"], true);
    assert_eq!(body["verified_commit_sha"], "a".repeat(40));
    assert_eq!(body["verified_ref"], "main");
    assert_eq!(body["event_count"], 2);
    assert_eq!(body["last_event_kind"], "REF_VERIFIED");
    assert!(body["last_failure_class"].is_null());

    let wrong_analysis = Uuid::new_v4();
    let wrong = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{wrong_analysis}/git-publication-intents/{publication}/remote-status",
        f.workspace
    );
    assert_eq!(
        call(f.router.clone(), request("GET", wrong, &f.reader, None))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn publish_endpoint_requires_explicit_transient_authorization_and_replays_verified_state_without_it()
 {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();
    let target_uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                target_uri,
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"history",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                }))
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let base = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("{base}/git-publication-intents/{publication}"),
                &f.writer,
                Some(json!({"target_id":target})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let publish_uri = format!("{base}/git-publication-intents/{publication}/publish");
    let bad_confirmation = request(
        "POST",
        publish_uri.clone(),
        &f.writer,
        Some(json!({"attempt_id":Uuid::new_v4(),"confirmation":"NO"})),
    );
    assert_eq!(
        call(f.router.clone(), bad_confirmation).await.0,
        StatusCode::BAD_REQUEST
    );

    let missing_provider_auth = request(
        "POST",
        publish_uri.clone(),
        &f.writer,
        Some(json!({
            "attempt_id":Uuid::new_v4(),
            "confirmation":"PUBLISH_STANDARD_PRESENTATION_TO_GITHUB"
        })),
    );
    assert_eq!(
        call(f.router.clone(), missing_provider_auth).await.0,
        StatusCode::UNAUTHORIZED
    );

    let digest = authz::credential_digest(&f.writer).unwrap();
    let attempt = Uuid::new_v4();
    for (kind, observed_ref) in [
        (git_publication::RemoteEventKind::PushAccepted, None),
        (
            git_publication::RemoteEventKind::RefVerified,
            Some("main".to_string()),
        ),
    ] {
        authz::with_auth_digest(digest.clone(), async {
            git_publication::record_remote_event(
                &f.secure,
                f.workspace,
                f.writer_principal,
                &git_publication::RemoteEventInput {
                    event_id: attempt,
                    publication_id: publication,
                    kind,
                    remote_commit_sha: Some("b".repeat(40)),
                    observed_ref,
                    failure_class: None,
                },
            )
            .await
            .unwrap()
        })
        .await;
    }

    // A verified publication is replay-safe and needs no provider credential:
    // no second network side effect is attempted.
    let verified_replay = request(
        "POST",
        publish_uri,
        &f.writer,
        Some(json!({
            "attempt_id":Uuid::new_v4(),
            "confirmation":"PUBLISH_STANDARD_PRESENTATION_TO_GITHUB"
        })),
    );
    let (status, body) = call(f.router.clone(), verified_replay).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["replayed_verified_publication"], true);
    assert_eq!(body["provider_secret_stored"], false);
    assert_eq!(body["status"]["remote_publish_verified"], true);
    assert_eq!(body["status"]["verified_commit_sha"], "b".repeat(40));
}

#[tokio::test]
async fn remote_status_counts_beyond_old_preview_bound_and_reports_last_failure_class() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("/v1/workspaces/{}/git-publication-targets/{target}", f.workspace),
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"event-history",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                }))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let base = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("{base}/git-publication-intents/{publication}"),
                &f.writer,
                Some(json!({"target_id":target})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let digest = authz::credential_digest(&f.writer).unwrap();
    for _ in 0..105 {
        let event = git_publication::RemoteEventInput {
            event_id: Uuid::new_v4(),
            publication_id: publication,
            kind: git_publication::RemoteEventKind::PublishFailed,
            remote_commit_sha: None,
            observed_ref: None,
            failure_class: Some("TRANSPORT".into()),
        };
        authz::with_auth_digest(digest.clone(), async {
            git_publication::record_remote_event(
                &f.secure,
                f.workspace,
                f.writer_principal,
                &event,
            )
            .await
            .unwrap()
        })
        .await;
    }
    let (status, body) = call(
        f.router.clone(),
        request(
            "GET",
            format!("{base}/git-publication-intents/{publication}/remote-status"),
            &f.reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "REMOTE_ATTEMPT_FAILED");
    assert_eq!(body["event_count"], 105);
    assert_eq!(body["last_event_kind"], "PUBLISH_FAILED");
    assert_eq!(body["last_failure_class"], "TRANSPORT");
}

#[tokio::test]
async fn failed_publish_attempt_event_is_retry_idempotent() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let publication = Uuid::new_v4();
    let target_uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                target_uri,
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"history",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                }))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let base = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                format!("{base}/git-publication-intents/{publication}"),
                &f.writer,
                Some(json!({"target_id":target})),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let attempt = Uuid::new_v4();
    let digest = authz::credential_digest(&f.writer).unwrap();
    let failed = git_publication::RemoteEventInput {
        event_id: attempt,
        publication_id: publication,
        kind: git_publication::RemoteEventKind::PublishFailed,
        remote_commit_sha: None,
        observed_ref: None,
        failure_class: Some("TRANSPORT".into()),
    };
    let first = authz::with_auth_digest(digest.clone(), async {
        git_publication::record_remote_event(
            &f.secure,
            f.workspace,
            f.writer_principal,
            &failed,
        )
        .await
        .unwrap()
    })
    .await;
    assert!(!first.1);
    let replay = authz::with_auth_digest(digest, async {
        git_publication::record_remote_event(
            &f.secure,
            f.workspace,
            f.writer_principal,
            &failed,
        )
        .await
        .unwrap()
    })
    .await;
    assert!(replay.1);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.git_publication_events
          WHERE workspace_id=$1 AND publication_id=$2 AND event_id=$3
            AND event_kind='PUBLISH_FAILED'",
    )
    .bind(f.workspace)
    .bind(publication)
    .bind(attempt)
    .fetch_one(&f.admin)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn target_registration_rejects_urls_and_public_sensitive_scope() {
    let f = setup().await;
    let target = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    for body in [
        json!({"repository_owner":"https://example.com","repository_name":"repo","ref_name":"main","repository_visibility":"PRIVATE","sensitive_exports_allowed":false}),
        json!({"repository_owner":"owner","repository_name":"repo","ref_name":"main","repository_visibility":"PUBLIC","sensitive_exports_allowed":true}),
        json!({"repository_owner":"owner","repository_name":"../repo","ref_name":"main","repository_visibility":"PRIVATE","sensitive_exports_allowed":false}),
    ] {
        assert_eq!(
            call(
                f.router.clone(),
                request("POST", uri.clone(), &f.writer, Some(body))
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn reader_cannot_create_targets_or_publication_intents() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                uri,
                &f.reader,
                Some(json!({
                    "repository_owner":"owner","repository_name":"repo","ref_name":"main",
                    "repository_visibility":"PRIVATE","sensitive_exports_allowed":false
                }))
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let intent = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{intent}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request("POST", uri, &f.reader, Some(json!({"target_id":target})))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn write_only_credential_cannot_publish_an_analysis_it_cannot_read() {
    let f = setup().await;
    let (situation, analysis) = published_analysis(&f).await;
    let target = Uuid::new_v4();
    let target_uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                target_uri,
                &f.writer,
                Some(json!({
                    "repository_owner":"owner",
                    "repository_name":"history",
                    "ref_name":"main",
                    "repository_visibility":"PRIVATE",
                    "sensitive_exports_allowed":false
                })),
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let write_only = authz::provision_token(&f.admin, Uuid::new_v4(), f.workspace, false, true, 1)
        .await
        .unwrap();
    let publication = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{publication}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request("POST", uri, &write_only, Some(json!({"target_id":target}))),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn publication_metadata_is_workspace_isolated() {
    let f = setup().await;
    let target = Uuid::new_v4();
    let uri = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        f.workspace
    );
    assert_eq!(
        call(
            f.router.clone(),
            request(
                "POST",
                uri,
                &f.writer,
                Some(json!({
                    "repository_owner":"owner","repository_name":"repo","ref_name":"main",
                    "repository_visibility":"PRIVATE","sensitive_exports_allowed":false
                }))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let other = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'other-git-workspace')",
    )
    .bind(other)
    .execute(&f.admin)
    .await
    .unwrap();
    let other_reader = authz::provision_token(&f.admin, Uuid::new_v4(), other, true, false, 1)
        .await
        .unwrap();
    let (status, body) = call(
        f.router.clone(),
        request(
            "GET",
            format!("/v1/workspaces/{other}/git-publication-targets"),
            &other_reader,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["targets"].as_array().unwrap().is_empty());
}
