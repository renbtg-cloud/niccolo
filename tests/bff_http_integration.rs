#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{
    api, authz, checkpoint_git_publication, git_publication, model_adapter::DeterministicModel,
};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

struct Seed {
    admin: PgPool,
    secure: PgPool,
    router: Router,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    principal: Uuid,
    token: String,
}
async fn setup() -> Seed {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let secure = PgPoolOptions::new()
        .max_connections(16)
        .connect(&std::env::var("NICCOLO_PG_SECURE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'bff-http')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus',$3,1)").bind(workspace).bind(situation).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'focus',$3,$4,'CREATED')").bind(workspace).bind(situation).bind("a".repeat(64)).bind(principal).execute(&admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)").bind(workspace).bind(situation).bind(analysis).bind("a".repeat(64)).bind(principal).execute(&admin).await.unwrap();
    Seed {
        admin,
        secure: secure.clone(),
        router: api::router_with_model(
            secure,
            Arc::new(DeterministicModel::new("test", "deterministic-v1").unwrap()),
        ),
        workspace,
        situation,
        analysis,
        principal,
        token,
    }
}
fn path(s: &Seed, id: Uuid, suffix: &str) -> String {
    format!(
        "/v1/workspaces/{}/situations/{}/bff/operations/{}{}",
        s.workspace, s.situation, id, suffix
    )
}
fn intent_path(s: &Seed, id: Uuid, suffix: &str) -> String {
    format!(
        "/v1/workspaces/{}/situations/{}/bff/intents/{}{}",
        s.workspace, s.situation, id, suffix
    )
}
fn req(
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
    if_none_match: Option<&str>,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::ACCEPT, "application/json");
    if let Some(tag) = if_none_match {
        b = b.header(header::IF_NONE_MATCH, tag);
    }
    let data = body
        .map(|v| serde_json::to_vec(&v).unwrap())
        .unwrap_or_default();
    if !data.is_empty() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    b.body(Body::from(data)).unwrap()
}
async fn invoke(router: Router, r: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let resp = router.oneshot(r).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, headers, v)
}
fn body(s: &Seed, kind: &str) -> Value {
    json!({"kind":kind,"target_analysis_id":s.analysis,"idempotency_key":format!("{}-{kind}-{}",s.analysis,Uuid::new_v4()),"input":{},"step_budget":12})
}

#[tokio::test]
async fn v201_v204_sync_and_async_http_surfaces_share_operation_shape() {
    let s = setup().await;
    let sync = Uuid::new_v4();
    let (status, h, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, sync, "/sync"),
            &s.token,
            Some(body(&s, "BUILD_BRIEFING")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], "SUCCEEDED");
    assert!(h.get(header::ETAG).is_some());
    assert_eq!(v["operation_id"], sync.to_string());
    let async_id = Uuid::new_v4();
    let (status, h, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, async_id, "/async"),
            &s.token,
            Some(body(&s, "BUILD_HANDOFF")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(v["operation_id"], async_id.to_string());
    assert!(h.get(header::RETRY_AFTER).is_some());
    // A worker can be delayed by database scheduling on shared CI runners.
    // Yielding 30 times is not a meaningful asynchronous completion deadline.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let (_, _, now) = invoke(
            s.router.clone(),
            req("GET", &path(&s, async_id, ""), &s.token, None, None),
        )
        .await;
        if now["state"] == "SUCCEEDED" {
            return;
        }
        if matches!(now["state"].as_str(), Some("FAILED" | "CANCELLED")) {
            panic!("async deterministic operation entered terminal failure: {now}");
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "async deterministic operation did not finish by deadline: {now}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn v205_v208_pull_etag_poll_hint_events_and_cancel_are_first_class() {
    let s = setup().await;

    // Prove conditional reads on a terminal operation, where the representation
    // cannot race a background worker between the response and the follow-up GET.
    let etag_id = Uuid::new_v4();
    let (status, h, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, etag_id, "/sync"),
            &s.token,
            Some(body(&s, "BUILD_BRIEFING")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tag = h[header::ETAG].to_str().unwrap().to_string();
    let (status, _, _) = invoke(
        s.router.clone(),
        req("GET", &path(&s, etag_id, ""), &s.token, None, Some(&tag)),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);

    // Use an explicit human boundary so the async operation remains cancellable
    // after its worker has had a chance to advance it.
    let id = Uuid::new_v4();
    let mut async_body = body(&s, "AGENTIC_ANALYSIS");
    async_body["input"] = json!({"human_intent":"ABOUT_TO_ACT_OR_SAY"});
    let (status, h, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/async"),
            &s.token,
            Some(async_body),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(h["x-poll-after-ms"], "750");
    let (status, _, events) = invoke(
        s.router.clone(),
        req(
            "GET",
            &format!("{}?after=0&limit=1", path(&s, id, "/events")),
            &s.token,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events["events"].as_array().unwrap().len(), 1);

    let latest = loop {
        let (_, _, now) = invoke(
            s.router.clone(),
            req("GET", &path(&s, id, ""), &s.token, None, None),
        )
        .await;
        if now["state"] == "WAITING_FOR_HUMAN" {
            break now;
        }
        assert_ne!(
            now["terminal"], true,
            "operation finished before cancellation boundary"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    let version = latest["state_version"].as_i64().unwrap();
    let (status, _, cancelled) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/cancel"),
            &s.token,
            Some(json!({"expected_version":version,"reason":"human says stop"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled["state"], "CANCELLED");
    assert_eq!(cancelled["terminal"], true);
}

#[tokio::test]
async fn v209_v210_read_token_cannot_submit_or_cancel() {
    let s = setup().await;
    let read_principal = Uuid::new_v4();
    let read = authz::provision_token(&s.admin, read_principal, s.workspace, true, false, 1)
        .await
        .unwrap();
    let id = Uuid::new_v4();
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/async"),
            &read,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn v301_v315_agentic_sync_async_and_pull_share_one_operation_semantics() {
    let s = setup().await;
    let sync = Uuid::new_v4();
    let (status, _, v) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, sync, "/sync"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["state"], "SUCCEEDED");
    let async_id = Uuid::new_v4();
    let (status, _, queued) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, async_id, "/async"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let version = queued["state_version"].as_i64().unwrap();
    let wait_path = format!(
        "{}?after_version={version}&timeout_ms=3000",
        path(&s, async_id, "/wait")
    );
    let (status, _, changed) = invoke(
        s.router.clone(),
        req("GET", &wait_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(changed["state_version"].as_i64().unwrap() > version || changed["terminal"] == true);
    for _ in 0..200 {
        let (_, _, now) = invoke(
            s.router.clone(),
            req("GET", &path(&s, async_id, ""), &s.token, None, None),
        )
        .await;
        if now["terminal"] == true {
            assert_eq!(now["state"], "SUCCEEDED");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("async agentic operation did not finish");
}

#[tokio::test]
async fn agentic_model_budget_boundary_returns_provisional_success_and_checkpoints() {
    let s = setup().await;
    let id = Uuid::new_v4();
    let mut bounded = body(&s, "AGENTIC_ANALYSIS");
    bounded["input"] = json!({
        "model_budget": {
            "max_invocations": 1,
            "max_total_tokens": 200000
        }
    });
    let (status, _, operation) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/sync"),
            &s.token,
            Some(bounded),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(operation["state"], "SUCCEEDED");
    assert_eq!(operation["terminal"], true);
    assert_eq!(
        operation["result"]["completion_status"],
        "PARTIAL_BUDGET_BOUNDARY"
    );
    assert_eq!(operation["result"]["provisional"], true);
    assert_eq!(operation["result"]["stop_reason"], "INVOCATION_LIMIT");
    assert_eq!(
        operation["result"]["partial_result_establishes_truth"],
        false
    );
    assert_eq!(
        operation["result"]["partial_result_proves_completeness"],
        false
    );
    assert_eq!(
        operation["result"]["human_owns_consequential_decisions"],
        true
    );

    let (status, _, page) = invoke(
        s.router.clone(),
        req(
            "GET",
            &format!("{}?after=0&limit=50", path(&s, id, "/checkpoints")),
            &s.token,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let checkpoints = page["checkpoints"].as_array().unwrap();
    assert!(
        checkpoints
            .iter()
            .any(|checkpoint| checkpoint["checkpoint_kind"] == "STEP_COMPLETED")
    );
    let boundary = checkpoints
        .iter()
        .find(|checkpoint| checkpoint["checkpoint_kind"] == "BUDGET_BOUNDARY")
        .expect("budget boundary checkpoint");
    assert_eq!(boundary["provisional"], true);
    assert_eq!(boundary["terminal"], true);
    assert_eq!(boundary["establishes_truth"], false);
    assert_eq!(boundary["proves_completeness"], false);
    assert_eq!(boundary["authorizes_real_world_action"], false);
    assert_eq!(boundary["summary"]["stop_reason"], "INVOCATION_LIMIT");
    assert_eq!(boundary["summary"]["provisional"], true);

    let checkpoint_id = boundary["checkpoint_id"].as_str().expect("checkpoint UUID");
    let checkpoint_digest = boundary["summary_sha256"]
        .as_str()
        .expect("checkpoint summary digest");
    let package_path = format!("{}/checkpoints/{checkpoint_id}/package", path(&s, id, ""));
    let (status, _, package) = invoke(
        s.router.clone(),
        req("GET", &package_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(package["format"], "niccolo.operation-checkpoint-package");
    assert_eq!(package["scope_kind"], "OPERATION_CHECKPOINT");
    assert_eq!(package["checkpoint_id"], checkpoint_id);
    assert_eq!(package["checkpoint_summary_sha256"], checkpoint_digest);
    assert_eq!(package["provisional"], true);
    assert_eq!(package["terminal"], true);
    assert_eq!(package["published_to_git"], false);
    assert_eq!(package["includes_original_uploads"], false);
    assert_eq!(package["includes_examined_source_text"], false);
    assert_eq!(package["establishes_truth"], false);
    assert_eq!(package["proves_completeness"], false);
    assert_eq!(package["authorizes_real_world_action"], false);
    assert!(
        package["files"]["README.md"]
            .as_str()
            .unwrap()
            .contains("Remote publication performed: no")
    );
    assert!(package["files"]["checkpoint.json"].as_str().is_some());
    assert!(package["files"]["SHA256SUMS"].as_str().is_some());

    // A human may explicitly declare this durable checkpoint meaningful without
    // changing its provisional epistemic status or claiming remote publication.
    let checkpoint_base = format!("{}/checkpoints/{checkpoint_id}", path(&s, id, ""));
    let milestone = Uuid::new_v4();
    let milestone_path = format!("{checkpoint_base}/milestones/{milestone}");
    let milestone_body = json!({
        "milestone_kind":"TIMED_PARTIAL",
        "reason":"Invocation budget ended; preserve this bounded partial result for review"
    });
    let (status, _, declared) = invoke(
        s.router.clone(),
        req(
            "POST",
            &milestone_path,
            &s.token,
            Some(milestone_body.clone()),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(declared["replayed"], false);
    assert_eq!(declared["remote_publish_performed"], false);
    assert_eq!(declared["checkpoint_epistemic_status_changed"], false);
    assert_eq!(declared["milestone"]["milestone_id"], milestone.to_string());
    assert_eq!(declared["milestone"]["checkpoint_id"], checkpoint_id);
    assert_eq!(declared["milestone"]["checkpoint_provisional"], true);
    assert_eq!(declared["milestone"]["checkpoint_terminal"], true);
    assert_eq!(declared["milestone"]["remote_publish_performed"], false);
    assert_eq!(declared["milestone"]["establishes_truth"], false);
    assert_eq!(declared["milestone"]["proves_completeness"], false);
    assert_eq!(declared["milestone"]["authorizes_real_world_action"], false);

    let (status, _, replayed_milestone) = invoke(
        s.router.clone(),
        req(
            "POST",
            &milestone_path,
            &s.token,
            Some(milestone_body),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed_milestone["replayed"], true);

    let (status, _, fetched_milestone) = invoke(
        s.router.clone(),
        req(
            "GET",
            &format!("{checkpoint_base}/milestone"),
            &s.token,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_milestone["milestone_id"], milestone.to_string());
    assert_eq!(
        fetched_milestone["checkpoint_summary_sha256"],
        checkpoint_digest
    );

    let conflicting_milestone = Uuid::new_v4();
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &format!("{checkpoint_base}/milestones/{conflicting_milestone}"),
            &s.token,
            Some(json!({
                "milestone_kind":"OTHER",
                "reason":"Conflicting redeclaration must not overwrite history"
            })),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let target = Uuid::new_v4();
    let target_path = format!(
        "/v1/workspaces/{}/git-publication-targets/{target}",
        s.workspace
    );
    let (status, _, target_response) = invoke(
        s.router.clone(),
        req(
            "POST",
            &target_path,
            &s.token,
            Some(json!({
                "repository_owner":"checkpoint-owner",
                "repository_name":"checkpoint-history",
                "ref_name":"work",
                "repository_visibility":"PRIVATE",
                "sensitive_exports_allowed":false
            })),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(target_response["registration_publishes_nothing"], true);

    let preview_path = format!("{checkpoint_base}/git-publication-preview?target_id={target}");
    let (status, _, preview) = invoke(
        s.router.clone(),
        req("GET", &preview_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["scope_kind"], "OPERATION_CHECKPOINT");
    assert_eq!(preview["checkpoint_id"], checkpoint_id);
    assert_eq!(preview["checkpoint_summary_sha256"], checkpoint_digest);
    assert_eq!(preview["checkpoint_kind"], "BUDGET_BOUNDARY");
    assert_eq!(preview["provisional"], true);
    assert_eq!(preview["terminal"], true);
    assert_eq!(preview["requested_ref"], "work");
    assert_eq!(preview["remote_publish_performed"], false);
    assert_eq!(preview["ready_for_remote_publish"], false);
    assert_eq!(preview["requires_transient_provider_authorization"], true);
    assert_eq!(preview["establishes_truth"], false);
    assert_eq!(preview["proves_completeness"], false);
    assert_eq!(preview["authorizes_real_world_action"], false);
    assert_eq!(preview["package_sha256"].as_str().unwrap().len(), 64);

    let publication = Uuid::new_v4();
    let intent_path = format!("{checkpoint_base}/git-publication-intents/{publication}");
    let intent_body = json!({"target_id":target});
    let (status, _, recorded) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path,
            &s.token,
            Some(intent_body.clone()),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recorded["replayed"], false);
    assert_eq!(recorded["remote_publish_performed"], false);
    assert_eq!(recorded["checkpoint_status_preserved"], true);
    assert_eq!(recorded["intent"]["scope_kind"], "OPERATION_CHECKPOINT");
    assert_eq!(recorded["intent"]["checkpoint_id"], checkpoint_id);
    assert_eq!(
        recorded["intent"]["package_sha256"],
        preview["package_sha256"]
    );
    assert_eq!(recorded["intent"]["provisional"], true);
    assert_eq!(recorded["intent"]["terminal"], true);
    assert_eq!(recorded["intent"]["remote_publish_performed"], false);

    let (status, _, replayed) = invoke(
        s.router.clone(),
        req("POST", &intent_path, &s.token, Some(intent_body), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(
        replayed["intent"]["package_sha256"],
        preview["package_sha256"]
    );

    let publish_path = format!("{intent_path}/publish");
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &publish_path,
            &s.token,
            Some(json!({
                "attempt_id":Uuid::new_v4(),
                "confirmation":"PUBLISH_OPERATION_CHECKPOINT_TO_GITHUB"
            })),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let scope = niccolo_evidence::bff_operations::OperationScope {
        workspace: s.workspace,
        situation: s.situation,
        operation: id,
    };
    let remote_attempt = Uuid::new_v4();
    let remote_sha = "c".repeat(40);
    let digest = authz::credential_digest(&s.token).unwrap();
    authz::with_auth_digest(digest.clone(), async {
        checkpoint_git_publication::record_remote_event(
            &s.secure,
            s.workspace,
            s.principal,
            &git_publication::RemoteEventInput {
                event_id: remote_attempt,
                publication_id: publication,
                kind: git_publication::RemoteEventKind::PushAccepted,
                remote_commit_sha: Some(remote_sha.clone()),
                observed_ref: None,
                failure_class: None,
            },
        )
        .await
        .unwrap();
    })
    .await;

    let status_path = format!("{intent_path}/remote-status");
    let (status, _, pushed) = invoke(
        s.router.clone(),
        req("GET", &status_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pushed["state"], "PUSHED_UNVERIFIED");
    assert_eq!(pushed["remote_publish_verified"], false);

    authz::with_auth_digest(digest, async {
        checkpoint_git_publication::record_remote_event(
            &s.secure,
            s.workspace,
            s.principal,
            &git_publication::RemoteEventInput {
                event_id: remote_attempt,
                publication_id: publication,
                kind: git_publication::RemoteEventKind::RefVerified,
                remote_commit_sha: Some(remote_sha.clone()),
                observed_ref: Some("work".into()),
                failure_class: None,
            },
        )
        .await
        .unwrap();
    })
    .await;
    let (status, _, verified) = invoke(
        s.router.clone(),
        req("GET", &status_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(verified["state"], "VERIFIED_PUBLISHED");
    assert_eq!(verified["remote_publish_verified"], true);
    assert_eq!(verified["verified_commit_sha"], remote_sha);
    assert_eq!(verified["verified_ref"], "work");
    assert_eq!(verified["event_count"], 2);

    let (status, _, events) = invoke(
        s.router.clone(),
        req(
            "GET",
            &format!("{intent_path}/remote-events?limit=10"),
            &s.token,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events["events"].as_array().unwrap().len(), 2);
    assert_eq!(events["provider_secret_stored"], false);
    assert_eq!(
        events["event_payloads_include_provider_response_bodies"],
        false
    );

    let missing_checkpoint = Uuid::new_v4();
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "GET",
            &format!(
                "{}/checkpoints/{missing_checkpoint}/package",
                path(&s, id, "")
            ),
            &s.token,
            None,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn v316_v325_agentic_plan_is_inspectable_through_bff() {
    let s = setup().await;
    let id = Uuid::new_v4();
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, id, "/async"),
            &s.token,
            Some(body(&s, "AGENTIC_ANALYSIS")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let (status, _, plan) = invoke(
            s.router.clone(),
            req("GET", &path(&s, id, "/agentic"), &s.token, None, None),
        )
        .await;
        if status == StatusCode::OK {
            assert!(plan["steps"].as_array().unwrap().len() >= 6);
            assert!(plan["plan_sha256"].as_str().unwrap().len() == 64);
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("agentic plan was not visible within 3 seconds");
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn v391_v430_human_intent_bff_creates_frozen_analysis_and_uses_same_agentic_runtime() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = json!({
        "analysis_id": analysis,
        "intent": "UNDERSTAND",
        "goal": null,
        "reanalysis_of": null,
        "idempotency_key": format!("intent-{operation}"),
        "step_budget": 32,
        "context": {"surface":"http-integration","human_owns_project":true}
    });
    let (status, headers, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(request),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["analysis_id"], analysis.to_string());
    assert_eq!(value["intent"], "UNDERSTAND");
    assert_eq!(value["operation"]["operation_id"], operation.to_string());
    assert_eq!(value["operation"]["state"], "SUCCEEDED");
    assert!(headers.get(header::ETAG).is_some());

    let analysis_path = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}",
        s.workspace, s.situation, analysis
    );
    let (status, _, frozen) = invoke(
        s.router.clone(),
        req("GET", &analysis_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(frozen["analysis_id"], analysis.to_string());
    assert_eq!(frozen["frozen_input"], true);
    assert_eq!(frozen["incorporates_future_context"], false);
    assert!(frozen["goal"].as_str().unwrap().contains("evidence"));
}

#[tokio::test]
async fn human_intent_exposes_explicit_operation_wide_model_budget() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = json!({
        "analysis_id": analysis,
        "intent": "UNDERSTAND",
        "goal": "Use a deliberately bounded reasoning budget.",
        "reanalysis_of": null,
        "idempotency_key": format!("intent-budget-{operation}"),
        "step_budget": 32,
        "model_budget": {
            "max_invocations": 17,
            "max_total_tokens": 54321,
            "finalization_reserve_ms": 1234
        },
        "context": {"surface":"http-budget-integration"}
    });
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(request),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, budget) = invoke(
        s.router.clone(),
        req("GET", &path(&s, operation, "/budget"), &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(budget["budget"]["max_invocations"], 17);
    assert_eq!(budget["budget"]["max_total_tokens"], 54321);
    assert_eq!(budget["budget"]["finalization_reserve_ms"], 1234);
    assert_eq!(budget["budget"]["budget_is_quality_score"], false);
}

#[tokio::test]
async fn malformed_human_intent_model_budget_leaves_no_analysis_or_operation() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = json!({
        "analysis_id": analysis,
        "intent": "UNDERSTAND",
        "goal": null,
        "reanalysis_of": null,
        "idempotency_key": format!("intent-bad-budget-{operation}"),
        "step_budget": 32,
        "model_budget": {"max_invocations": 0},
        "context": {}
    });
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(request),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let operation_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.bff_operations
          WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(operation)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let analysis_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM niccolo.situation_analysis_snapshots
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    assert_eq!(operation_count, 0);
    assert_eq!(analysis_count, 0);
}

fn intent_request(operation: Uuid, analysis: Uuid, intent: &str) -> Value {
    json!({
        "analysis_id": analysis,
        "intent": intent,
        "goal": null,
        "reanalysis_of": null,
        "idempotency_key": format!("intent-aware-{operation}"),
        "step_budget": 32,
        "context": {"surface":"intent-aware-planning-test","human_owns_project":true}
    })
}

async fn agentic_plan(s: &Seed, operation: Uuid) -> Value {
    let (status, _, plan) = invoke(
        s.router.clone(),
        req("GET", &path(s, operation, "/agentic"), &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    plan
}

fn step<'a>(plan: &'a Value, kind: &str) -> Option<&'a Value> {
    plan["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["kind"] == kind)
}

#[tokio::test]
async fn v431_v440_about_to_act_or_say_forces_explicit_human_boundary() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let (status, _, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(intent_request(operation, analysis, "ABOUT_TO_ACT_OR_SAY")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["operation"]["state"], "WAITING_FOR_HUMAN");
    assert_eq!(value["operation"]["terminal"], false);

    let plan = agentic_plan(&s, operation).await;
    let synthesis = step(&plan, "SYNTHESIZE").unwrap();
    assert!(
        synthesis["label"]
            .as_str()
            .unwrap()
            .contains("before the human acts")
    );
    let human = step(&plan, "REQUEST_HUMAN_DECISION").unwrap();
    assert_eq!(human["requires_human"], true);
    assert_eq!(human["state"], "WAITING_FOR_HUMAN");
    assert!(
        human["label"]
            .as_str()
            .unwrap()
            .contains("human must choose")
    );
    assert_eq!(step(&plan, "PREPARE_HANDOFF").unwrap()["state"], "PENDING");
}

#[tokio::test]
async fn v441_v450_understand_changes_synthesis_without_forcing_human_choice() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let (status, _, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(intent_request(operation, analysis, "UNDERSTAND")),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["operation"]["state"], "SUCCEEDED");

    let plan = agentic_plan(&s, operation).await;
    assert!(step(&plan, "REQUEST_HUMAN_DECISION").is_none());
    assert!(
        step(&plan, "SYNTHESIZE").unwrap()["label"]
            .as_str()
            .unwrap()
            .contains("provisional understanding")
    );
    assert_eq!(plan["complete"], true);
}

#[tokio::test]
async fn v451_v460_figure_out_what_matters_keeps_attention_distinct_from_truth() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let (status, _, value) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(intent_request(
                operation,
                analysis,
                "FIGURE_OUT_WHAT_MATTERS",
            )),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["operation"]["state"], "SUCCEEDED");

    let plan = agentic_plan(&s, operation).await;
    let synthesis = step(&plan, "SYNTHESIZE").unwrap()["label"]
        .as_str()
        .unwrap();
    assert!(synthesis.contains("what appears material"));
    assert!(synthesis.contains("attention-worthy unresolved questions"));
    assert!(step(&plan, "REQUEST_HUMAN_DECISION").is_none());
}

#[tokio::test]
async fn v461_v470_human_intent_is_durable_input_with_non_execution_boundary() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = intent_request(operation, analysis, "ABOUT_TO_ACT_OR_SAY");
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &intent_path(&s, operation, "/sync"),
            &s.token,
            Some(request),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let raw: String = sqlx::query_scalar(
        "SELECT input_json FROM niccolo.bff_operations WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(operation)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let input: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(input["human_intent"], "ABOUT_TO_ACT_OR_SAY");
    assert_eq!(input["human_owns_project"], true);
    assert_eq!(input["automatic_external_action_allowed"], false);
}

#[tokio::test]
async fn v471_v480_exact_intent_replay_reuses_operation_and_plan() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let request = intent_request(operation, analysis, "ABOUT_TO_ACT_OR_SAY");

    for expected_replay in [false, true] {
        let (status, _, value) = invoke(
            s.router.clone(),
            req(
                "POST",
                &intent_path(&s, operation, "/sync"),
                &s.token,
                Some(request.clone()),
                None,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["analysis_replayed"], expected_replay);
        assert_eq!(value["operation"]["operation_id"], operation.to_string());
        assert_eq!(value["operation"]["state"], "WAITING_FOR_HUMAN");
    }

    let plan = agentic_plan(&s, operation).await;
    assert_eq!(plan["plan_revision"], 1);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.agentic_plans WHERE workspace_id=$1 AND situation_id=$2 AND operation_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(operation)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn v561_v575_epistemic_read_model_groups_reasoning_without_promoting_it_to_truth() {
    let s = setup().await;
    let principal: Uuid = sqlx::query_scalar(
        "SELECT requested_by_principal_id FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let claim = Uuid::new_v4();
    let assumption = Uuid::new_v4();
    let standalone_assumption = Uuid::new_v4();
    let alternative = Uuid::new_v4();
    let gap = Uuid::new_v4();
    let standalone_gap = Uuid::new_v4();

    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'Provisional answer','Fixture output for epistemic view',$4)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,qualification,created_by_principal_id) VALUES($1,$2,$3,$4,1,'INFERENCE','A provisional interpretation','Needs corroboration',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(claim).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_assumptions(workspace_id,situation_id,analysis_id,assumption_id,claim_id,statement,basis_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'Timing reflects intent','Temporal proximity only',$6),($1,$2,$3,$7,NULL,'The source chronology is complete','Not yet verified',$6)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(assumption).bind(claim).bind(principal).bind(standalone_assumption).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_claim_alternatives(workspace_id,situation_id,analysis_id,claim_id,alternative_id,alternative_text,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'Coincidence rather than intent','Same observations admit this reading',$6)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(claim).bind(alternative).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,claim_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'Need the original message','MESSAGE_EXPORT',$6),($1,$2,$3,$7,NULL,'Need an independent chronology',NULL,$6)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(gap).bind(claim).bind(principal).bind(standalone_gap).execute(&s.admin).await.unwrap();

    let route = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/bff/epistemic",
        s.workspace, s.situation, s.analysis
    );
    let (status, headers, view) =
        invoke(s.router.clone(), req("GET", &route, &s.token, None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(view["claims"].as_array().unwrap().len(), 1);
    assert_eq!(view["claims"][0]["claim_id"], claim.to_string());
    assert_eq!(
        view["claims"][0]["assumptions"][0]["assumption_id"],
        assumption.to_string()
    );
    assert_eq!(
        view["claims"][0]["alternatives"][0]["alternative_id"],
        alternative.to_string()
    );
    assert_eq!(
        view["claims"][0]["evidence_gaps"][0]["gap_id"],
        gap.to_string()
    );
    assert_eq!(
        view["unlinked_assumptions"][0]["assumption_id"],
        standalone_assumption.to_string()
    );
    assert_eq!(
        view["unlinked_evidence_gaps"][0]["gap_id"],
        standalone_gap.to_string()
    );
    assert_eq!(view["claims_are_verified_facts"], false);
    assert_eq!(view["assumptions_are_facts"], false);
    assert_eq!(view["alternatives_are_automatically_ranked"], false);
    assert_eq!(view["satisfied_gap_proves_claim"], false);
    assert_eq!(view["browser_is_domain_authority"], false);
}

#[tokio::test]
async fn v601_v620_claim_provenance_returns_exact_source_and_separates_human_context() {
    let s = setup().await;
    let (principal, input_revision): (Uuid, i64) = sqlx::query_as(
        "SELECT requested_by_principal_id,input_revision FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let document = Uuid::new_v4();
    let evidence = Uuid::new_v4();
    let contribution = Uuid::new_v4();
    let claim = Uuid::new_v4();

    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'chat.txt#1','Hello from Ana\n','REPORTED_SPEECH')")
        .bind(s.workspace).bind(evidence).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'WHATSAPP_EXPORT','chat.txt','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',15)")
        .bind(s.workspace).bind(document).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$3,0,15,'Ana')")
        .bind(s.workspace).bind(document).bind(evidence).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_sources(workspace_id,situation_id,document_id,added_by_principal_id) VALUES($1,$2,$3,$4)")
        .bind(s.workspace).bind(s.situation).bind(document).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id) VALUES($1,$2,$3,$4)")
        .bind(s.workspace).bind(s.situation).bind(input_revision).bind(document).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(evidence).bind(principal).execute(&s.admin).await.unwrap();

    sqlx::query("INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text) VALUES($1,$2,$3,$4,'USER_REPORTED','I remember the tone as sarcastic')")
        .bind(s.workspace).bind(s.situation).bind(contribution).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id) VALUES($1,$2,$3,$4)")
        .bind(s.workspace).bind(s.situation).bind(input_revision).bind(contribution).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,input_revision,contribution_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(input_revision).bind(contribution).bind(principal).execute(&s.admin).await.unwrap();

    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'Provisional answer','Fixture output for provenance',$4)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,created_by_principal_id) VALUES($1,$2,$3,$4,1,'INFERENCE','The message may be sarcastic',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(claim).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_claim_fragments(workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'SUPPORTS_AS_USED',$6)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(claim).bind(evidence).bind(principal).execute(&s.admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_claim_contributions(workspace_id,situation_id,analysis_id,claim_id,contribution_id,relation,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,'QUALIFIES',$6)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(claim).bind(contribution).bind(principal).execute(&s.admin).await.unwrap();

    let route = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/bff/epistemic",
        s.workspace, s.situation, s.analysis
    );
    let (status, _, view) =
        invoke(s.router.clone(), req("GET", &route, &s.token, None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        view["claims"][0]["source_provenance"][0]["evidence_id"],
        evidence.to_string()
    );
    assert_eq!(
        view["claims"][0]["source_provenance"][0]["relation"],
        "SUPPORTS_AS_USED"
    );
    assert_eq!(
        view["claims"][0]["source_provenance"][0]["content"],
        "Hello from Ana\n"
    );
    assert_eq!(
        view["claims"][0]["source_provenance"][0]["locator"],
        "chat.txt"
    );
    assert_eq!(
        view["claims"][0]["human_context"][0]["contribution_id"],
        contribution.to_string()
    );
    assert_eq!(
        view["claims"][0]["human_context"][0]["relation"],
        "QUALIFIES"
    );
    assert_eq!(view["claim_fragment_relations_establish_truth"], false);
    assert_eq!(
        view["claim_fragment_link_count_is_independent_evidence_count"],
        false
    );
    assert_eq!(view["human_context_is_evidence"], false);
}

#[tokio::test]
async fn v761_v780_human_requested_reanalysis_preserves_prior_and_creates_linked_snapshot() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let new_analysis = Uuid::new_v4();
    let route = path(&s, operation, "/sync");
    let request = json!({
        "kind": "REANALYZE",
        "target_analysis_id": s.analysis,
        "parent_operation_id": null,
        "idempotency_key": format!("reanalysis-{operation}"),
        "input": {
            "new_analysis_id": new_analysis,
            "goal": "Revisit the prior analysis using the current Situation"
        },
        "deadline_at": null,
        "step_budget": 32
    });

    let (status, _, value) = invoke(
        s.router.clone(),
        req("POST", &route, &s.token, Some(request), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["kind"], "REANALYZE");
    assert_eq!(value["state"], "SUCCEEDED");
    assert_eq!(value["target_analysis_id"], s.analysis.to_string());
    assert_eq!(value["result"]["prior_analysis_id"], s.analysis.to_string());
    assert_eq!(value["result"]["new_analysis_id"], new_analysis.to_string());
    assert_eq!(value["result"]["historical_output_preserved"], true);
    assert_eq!(value["result"]["human_requested_operation"], true);
    assert_eq!(value["result"]["agentic_execution_started"], false);

    let lineage: (Option<Uuid>, String) = sqlx::query_as(
        "SELECT reanalysis_of,goal FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(new_analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    assert_eq!(lineage.0, Some(s.analysis));
    assert_eq!(
        lineage.1,
        "Revisit the prior analysis using the current Situation"
    );

    let prior_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    assert_eq!(prior_count, 1);
}

#[tokio::test]
async fn v921_v940_reanalysis_revision_endpoint_reports_recorded_change_without_causality() {
    let s = setup().await;
    let operation = Uuid::new_v4();
    let new_analysis = Uuid::new_v4();

    let (status, _, created) = invoke(
        s.router.clone(),
        req(
            "POST",
            &path(&s, operation, "/sync"),
            &s.token,
            Some(json!({
                "kind":"REANALYZE",
                "target_analysis_id":s.analysis,
                "parent_operation_id":null,
                "idempotency_key":format!("revision-view-{operation}"),
                "input":{
                    "new_analysis_id":new_analysis,
                    "goal":"Explain recorded change without inferring causality"
                },
                "deadline_at":null,
                "step_budget":32
            })),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        created["result"]["new_analysis_id"],
        new_analysis.to_string()
    );

    let route = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/reanalysis-revision",
        s.workspace, s.situation, new_analysis
    );
    let (status, _, revision) =
        invoke(s.router.clone(), req("GET", &route, &s.token, None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revision["prior_analysis_id"], s.analysis.to_string());
    assert_eq!(revision["new_analysis_id"], new_analysis.to_string());
    assert_eq!(revision["unlinked_claims_are_inferred_change"], false);
    assert_eq!(
        revision["input_change_proves_causal_reason_for_output_change"],
        false
    );
}

#[tokio::test]
async fn reasoning_handoff_read_model_survives_operation_and_preserves_non_authority_boundaries() {
    let s = setup().await;
    let principal: Uuid = sqlx::query_scalar(
        "SELECT requested_by_principal_id FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let candidate = Uuid::new_v4();
    let question = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id,interpretation_text,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,'A competing reading remains possible','The current evidence does not discriminate it',$5)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(candidate)
    .bind(principal)
    .execute(&s.admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,'What happened immediately afterward?')",
    )
    .bind(s.workspace)
    .bind(question)
    .execute(&s.admin)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,'The answer could distinguish the candidate interpretations',$5)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(question)
    .bind(principal)
    .execute(&s.admin)
    .await
    .unwrap();

    let route = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/bff/handoff",
        s.workspace, s.situation, s.analysis
    );
    let (status, headers, view) =
        invoke(s.router.clone(), req("GET", &route, &s.token, None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(view["handoff"]["version"], 3);
    assert_eq!(view["handoff_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        view["handoff"]["candidate_interpretations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        view["handoff"]["candidate_interpretations"][0]["interpretation_id"],
        candidate.to_string()
    );
    assert_eq!(
        view["handoff"]["clarifications"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        view["handoff"]["clarifications"][0]["question_id"],
        question.to_string()
    );
    assert_eq!(view["handoff"]["clarifications"][0]["answered"], false);
    assert_eq!(view["handoff"]["clarifications_are_evidence"], false);
    assert_eq!(
        view["handoff"]["candidate_interpretations_are_facts"],
        false
    );
    assert_eq!(
        view["handoff"]["candidate_interpretations_are_automatically_ranked"],
        false
    );
    assert_eq!(view["handoff"]["handoff_grants_authority"], false);
    assert_eq!(view["handoff"]["handoff_starts_analysis"], false);
    assert_eq!(view["handoff"]["handoff_executes_actions"], false);
    assert_eq!(view["read_model_is_authority"], false);
    assert_eq!(view["read_model_executes_actions"], false);
}

#[tokio::test]
async fn handoff_receipt_is_sha_bound_idempotent_and_never_implies_agreement_or_authority() {
    let s = setup().await;
    let principal: Uuid = sqlx::query_scalar(
        "SELECT requested_by_principal_id FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    let handoff_path = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/bff/handoff",
        s.workspace, s.situation, s.analysis
    );
    let (status, _, handoff) = invoke(
        s.router.clone(),
        req("GET", &handoff_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sha = handoff["handoff_sha256"].as_str().unwrap().to_owned();

    let receipt = Uuid::new_v4();
    let receipt_path = format!("{handoff_path}/receipts/{receipt}");
    let payload = json!({
        "handoff_sha256": sha,
        "kind": "REVIEWED",
        "note": "Reviewed by the authenticated recipient"
    });
    let (status, _, created) = invoke(
        s.router.clone(),
        req("POST", &receipt_path, &s.token, Some(payload.clone()), None),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["recipient_principal_id"], principal.to_string());
    assert_eq!(created["replayed"], false);
    assert_eq!(created["receipt_means_agreement"], false);
    assert_eq!(created["receipt_grants_authority"], false);

    let (status, _, replay) = invoke(
        s.router.clone(),
        req("POST", &receipt_path, &s.token, Some(payload.clone()), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let mut conflicting = payload.clone();
    conflicting["kind"] = json!("DECLINED");
    let (status, _, _) = invoke(
        s.router.clone(),
        req("POST", &receipt_path, &s.token, Some(conflicting), None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let stale_receipt = Uuid::new_v4();
    let stale_path = format!("{handoff_path}/receipts/{stale_receipt}");
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &stale_path,
            &s.token,
            Some(json!({
                "handoff_sha256": "b".repeat(64),
                "kind": "RECEIVED",
                "note": null
            })),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let read_only = authz::provision_token(&s.admin, Uuid::new_v4(), s.workspace, true, false, 1)
        .await
        .unwrap();
    let read_only_receipt = Uuid::new_v4();
    let read_only_path = format!("{handoff_path}/receipts/{read_only_receipt}");
    let (status, _, _) = invoke(
        s.router.clone(),
        req(
            "POST",
            &read_only_path,
            &read_only,
            Some(json!({
                "handoff_sha256": handoff["handoff_sha256"],
                "kind": "RECEIVED",
                "note": null
            })),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let receipts_path = format!("{handoff_path}/receipts");
    let (status, _, list) = invoke(
        s.router.clone(),
        req("GET", &receipts_path, &s.token, None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["receipts"].as_array().unwrap().len(), 1);
    assert_eq!(list["receipts"][0]["receipt_id"], receipt.to_string());
    assert_eq!(list["receipts"][0]["kind"], "REVIEWED");
    assert_eq!(
        list["receipts"][0]["note"],
        "Reviewed by the authenticated recipient"
    );
    assert!(
        !list["receipts"][0]["created_at"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        list["receipts"][0]["recipient_principal_id"],
        principal.to_string()
    );
    assert_eq!(list["current_handoff_sha256"], handoff["handoff_sha256"]);
    assert_eq!(list["receipts"][0]["matches_current_handoff"], true);
    assert_eq!(list["receipt_means_agreement"], false);
    assert_eq!(list["receipt_grants_authority"], false);

    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_candidate_interpretations(workspace_id,situation_id,analysis_id,interpretation_id,interpretation_text,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,'A later competing interpretation','This changes the deterministic handoff hash',$5)",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .bind(Uuid::new_v4())
    .bind(principal)
    .execute(&s.admin)
    .await
    .unwrap();
    let (_, _, changed_handoff) = invoke(
        s.router.clone(),
        req("GET", &handoff_path, &s.token, None, None),
    )
    .await;
    assert_ne!(changed_handoff["handoff_sha256"], handoff["handoff_sha256"]);
    let (_, _, historical_list) = invoke(
        s.router.clone(),
        req("GET", &receipts_path, &s.token, None, None),
    )
    .await;
    assert_eq!(
        historical_list["current_handoff_sha256"],
        changed_handoff["handoff_sha256"]
    );
    assert_eq!(
        historical_list["receipts"][0]["matches_current_handoff"],
        false
    );

    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.situation_reasoning_handoff_receipts WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(s.workspace)
    .bind(s.situation)
    .bind(s.analysis)
    .fetch_one(&s.admin)
    .await
    .unwrap();
    assert_eq!(stored, 1);
}
