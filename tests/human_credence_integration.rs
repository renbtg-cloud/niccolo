#![cfg(feature = "pg-integration")]
//! EPI-006: human confidence is not documentary evidence or a calibrated probability.
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{api, authz};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Harness {
    router: Router,
    admin: PgPool,
    workspace: Uuid,
    writer_principal: Uuid,
    writer: String,
    other_writer: String,
    reader: String,
}

async fn setup() -> Harness {
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
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'human credence test')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let writer_principal = Uuid::new_v4();
    let writer = authz::provision_token(&admin, writer_principal, workspace, true, true, 1)
        .await
        .unwrap();
    let other_writer = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Harness {
        router: api::router(secure),
        admin,
        workspace,
        writer_principal,
        writer,
        other_writer,
        reader,
    }
}

fn req(method: &str, url: &str, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(url)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}

async fn call(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})),
    )
}

#[tokio::test]
async fn human_credence_is_bounded_idempotent_authz_scoped_and_not_truth() {
    let h = setup().await;
    let situation = Uuid::new_v4();
    let analysis = Uuid::new_v4();
    let claim = Uuid::new_v4();
    let judgement = Uuid::new_v4();
    let situation_path = format!("/v1/workspaces/{}/situations/{situation}", h.workspace);
    let (status, _) = call(
        h.router.clone(),
        req(
            "POST",
            &situation_path,
            &h.writer,
            Some(json!({"focus":"Why might André leave?"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let analysis_path = format!("{situation_path}/analysis-snapshots/{analysis}");
    let (status, _) = call(
        h.router.clone(),
        req(
            "POST",
            &analysis_path,
            &h.writer,
            Some(json!({"goal":"Evaluate competing explanations"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Claims are children of a published analysis output, not just a snapshot.
    let (output_status, _) = call(
        h.router.clone(),
        req(
            "POST",
            &format!("{analysis_path}/output"),
            &h.writer,
            Some(json!({
                "answer_text":"Several explanations are possible, pending review.",
                "coverage_note":"Synthetic integration fixture; no documentary evidence."
            })),
        ),
    )
    .await;
    assert_eq!(output_status, StatusCode::CREATED);

    // Admin-only fixture: use the actual writer principal, never an invented FK.
    let principal = h.writer_principal;

    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claims \
         (workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,created_by_principal_id) \
         VALUES ($1,$2,$3,$4,1,'INFERENCE','André may join a competitor',$5)"
    )
    .bind(h.workspace).bind(situation).bind(analysis).bind(claim).bind(principal)
    .execute(&h.admin).await.unwrap();

    let base = format!("{analysis_path}/claims/{claim}/credence");
    let path = format!("{base}/{judgement}");
    let body = json!({
        "confidence_percent":90,
        "usage_policy":"SUBJECTIVE_BELIEF",
        "rationale":"I know context outside this document"
    });
    let (status, created) = call(
        h.router.clone(),
        req("POST", &path, &h.writer, Some(body.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["confidence_percent"], 90);
    assert_eq!(created["is_calibrated_probability"], false);
    assert_eq!(created["is_documentary_evidence"], false);
    assert_eq!(created["makes_claim_a_fact"], false);

    let (status, replayed) = call(
        h.router.clone(),
        req("POST", &path, &h.writer, Some(body.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);

    let mut altered = body.clone();
    altered["confidence_percent"] = json!(95);
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &path, &h.writer, Some(altered))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &path, &h.other_writer, Some(body.clone()))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &path, &h.reader, Some(body.clone()))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let (status, listed) = call(h.router.clone(), req("GET", &base, &h.reader, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["human_credence"][0]["confidence_percent"], 90);
    assert_eq!(
        listed["human_credence"][0]["part_of_frozen_analysis_input"],
        false
    );
    assert_eq!(listed["confidence_establishes_truth"], false);

    let mut invalid = body;
    invalid["confidence_percent"] = json!(101);
    let invalid_path = format!("{base}/{}", Uuid::new_v4());
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &invalid_path, &h.writer, Some(invalid))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let missing_claim_path = format!("{analysis_path}/claims/{}/credence", Uuid::new_v4());
    assert_eq!(
        call(
            h.router.clone(),
            req("GET", &missing_claim_path, &h.reader, None)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn heuristic_preview_is_unverified_read_only_and_deduplicates_roots() {
    let h = setup().await;
    let root = Uuid::new_v4();
    let e1 = Uuid::new_v4();
    let e2 = Uuid::new_v4();
    let path = format!("/v1/workspaces/{}/epistemic/preview", h.workspace);
    let preview = json!({
        "policy": {
            "support_multiplier_pct": 100,
            "counter_multiplier_pct": 100,
            "per_origin_cap": 100
        },
        "human_declared_confidence_percent": 90,
        "observations": [
            {"evidence_id":e1,"documented_origin_id":root,"stance":"SUPPORTS",
             "declared_strength":80,"rationale":"same first-hand report"},
            {"evidence_id":e2,"documented_origin_id":root,"stance":"SUPPORTS",
             "declared_strength":80,"rationale":"copy of the report"}
        ]
    });
    let (status, result) = call(
        h.router.clone(),
        req("POST", &path, &h.reader, Some(preview.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["report"]["net_points"], 80);
    assert_eq!(result["report"]["human_declared_confidence_percent"], 90);
    assert_eq!(result["report"]["independence_established"], false);
    assert_eq!(result["report"]["score_is_probability_of_truth"], false);
    assert_eq!(
        result["report"]["human_confidence_is_documentary_evidence"],
        false
    );
    assert_eq!(result["changes_any_persisted_state"], false);
    assert_eq!(result["inputs_checked_against_database"], false);
    assert_eq!(result["mode"], "UNVERIFIED_PREVIEW");

    let mut invalid = preview;
    invalid["observations"][0]["declared_strength"] = json!(101);
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &path, &h.reader, Some(invalid))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}


#[tokio::test]
async fn epistemic_policy_falls_back_then_situation_overrides_without_mutating_history() {
    let h = setup().await;
    let situation = Uuid::new_v4();
    let base = format!("/v1/workspaces/{}/epistemic", h.workspace);
    let (status, defaults) = call(h.router.clone(), req("GET", &format!("{base}/policy"), &h.reader, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(defaults["effective_policy"]["scope"], "BUILTIN_DEFAULT");
    assert_eq!(defaults["effective_policy"]["policy"]["support_multiplier_pct"], 100);

    let situation_url = format!("/v1/workspaces/{}/situations/{situation}", h.workspace);
    assert_eq!(call(h.router.clone(), req("POST", &situation_url, &h.writer,
        Some(json!({"focus":"Why did the meeting happen?"})))).await.0, StatusCode::CREATED);

    let workspace_policy_id = Uuid::new_v4();
    let path = format!("{base}/policies/{workspace_policy_id}");
    let policy = json!({
        "situation_id":null,
        "policy":{"support_multiplier_pct":90,"counter_multiplier_pct":110,"per_origin_cap":95},
        "rationale":"Workspace conservative baseline"
    });
    let (status, created) = call(h.router.clone(), req("POST", &path, &h.writer, Some(policy.clone()))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["is_probability_model"], false);
    assert_eq!(created["retroactively_changes_frozen_analysis"], false);
    let (status, replay) = call(h.router.clone(), req("POST", &path, &h.writer, Some(policy.clone()))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let (status, inherited) = call(h.router.clone(),
        req("GET", &format!("{base}/policy?situation_id={situation}"), &h.reader, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inherited["effective_policy"]["scope"], "WORKSPACE");
    assert_eq!(inherited["effective_policy"]["policy"]["support_multiplier_pct"], 90);

    let situation_policy_id = Uuid::new_v4();
    let override_path = format!("{base}/policies/{situation_policy_id}");
    let override_policy = json!({
        "situation_id":situation,
        "policy":{"support_multiplier_pct":35,"counter_multiplier_pct":160,"per_origin_cap":75},
        "rationale":"Investigate contrary evidence more heavily"
    });
    assert_eq!(call(h.router.clone(), req("POST", &override_path, &h.writer, Some(override_policy.clone()))).await.0,
        StatusCode::CREATED);
    let (status, overridden) = call(h.router.clone(),
        req("GET", &format!("{base}/policy?situation_id={situation}"), &h.reader, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overridden["effective_policy"]["scope"], "SITUATION");
    assert_eq!(overridden["effective_policy"]["policy"]["counter_multiplier_pct"], 160);
    assert_eq!(overridden["effective_policy"]["retroactively_changes_frozen_analysis"], false);


    // EPI-009: omit trial policy to inherit the persisted Situation override.
    let preview_path = format!("{base}/preview");
    let preview_input = json!({
        "situation_id":situation,
        "human_declared_confidence_percent":90,
        "observations":[{
            "evidence_id":Uuid::new_v4(),
            "documented_origin_id":Uuid::new_v4(),
            "stance":"SUPPORTS",
            "declared_strength":80,
            "rationale":"hypothetical report for preview"
        }]
    });
    let (status, inherited_preview) = call(h.router.clone(),
        req("POST", &preview_path, &h.reader, Some(preview_input.clone()))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inherited_preview["policy_source"], "SITUATION");
    assert_eq!(inherited_preview["report"]["support_points"], 28);
    assert_eq!(inherited_preview["report"]["human_declared_confidence_percent"], 90);
    assert_eq!(inherited_preview["report"]["score_is_probability_of_truth"], false);

    // An explicitly trialled policy can differ without changing storage.
    let mut trial_input = preview_input;
    trial_input["policy"] = json!({
        "support_multiplier_pct":100,
        "counter_multiplier_pct":100,
        "per_origin_cap":100
    });
    let (status, trial) = call(h.router.clone(),
        req("POST", &preview_path, &h.reader, Some(trial_input))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(trial["policy_source"], "EXPLICIT_PREVIEW_OVERRIDE");
    assert_eq!(trial["report"]["support_points"], 80);
    assert_eq!(trial["changes_any_persisted_state"], false);


    let (status, unchanged) = call(h.router.clone(), req("GET", &format!("{base}/policy"), &h.reader, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unchanged["effective_policy"]["scope"], "WORKSPACE");

    let mut different = override_policy.clone();
    different["policy"]["counter_multiplier_pct"] = json!(40);
    assert_eq!(call(h.router.clone(), req("POST", &override_path, &h.writer, Some(different))).await.0,
        StatusCode::CONFLICT);
    assert_eq!(call(h.router.clone(), req("POST", &override_path, &h.reader, Some(override_policy))).await.0,
        StatusCode::FORBIDDEN);
    let mut bad = policy;
    bad["policy"]["support_multiplier_pct"] = json!(201);
    assert_eq!(call(h.router.clone(),
        req("POST", &format!("{base}/policies/{}", Uuid::new_v4()), &h.writer, Some(bad))).await.0,
        StatusCode::BAD_REQUEST);
    let unknown = Uuid::new_v4();
    assert_eq!(call(h.router.clone(),
        req("GET", &format!("{base}/policy?situation_id={unknown}"), &h.reader, None)).await.0,
        StatusCode::NOT_FOUND);
}
