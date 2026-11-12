#![cfg(feature = "pg-integration")]
//! EPI-013 — only authorized, examined, claim-linked evidence can receive
//! heuristic weights; stances and grouping are derived from stored provenance.
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
    situation: Uuid,
    principal: Uuid,
    writer: String,
    reader: String,
}
async fn setup() -> Harness {
    let admin = PgPoolOptions::new()
        .max_connections(5)
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
    let principal = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'grounded-claim-weights')",
    )
    .bind(workspace)
    .execute(&admin)
    .await
    .unwrap();
    let writer = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), workspace, true, false, 1)
        .await
        .unwrap();
    Harness {
        router: api::router(secure),
        admin,
        workspace,
        situation,
        principal,
        writer,
        reader,
    }
}
fn req(method: &str, path: &str, token: &str, body: Option<Value>) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap()
}
fn source_req(path: &str, token: &str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "text/plain")
        .header("x-source-locator", "two-fragments.txt")
        .body(Body::from(body))
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
struct Seed {
    analysis: Uuid,
    claim: Uuid,
    evidence: Vec<Uuid>,
    path: String,
}
async fn seed(h: &Harness) -> Seed {
    let sp = format!("/v1/workspaces/{}/situations/{}", h.workspace, h.situation);
    assert_eq!(
        call(
            h.router.clone(),
            req(
                "POST",
                &sp,
                &h.writer,
                Some(json!({"focus":"Does André plan to move?"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let document = Uuid::new_v4();
    let source_path = format!("/v1/workspaces/{}/sources/{document}", h.workspace);
    let (source_status,source)=call(h.router.clone(),source_req(&source_path,&h.writer,
        "01/01/2026, 10:00 - Alice: André had a meeting\n01/01/2026, 11:00 - Bob: I heard the same story\n")).await;
    assert_eq!(source_status, StatusCode::CREATED);
    let evidence = source["fragment_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().parse::<Uuid>().unwrap())
        .collect::<Vec<_>>();
    assert!(
        evidence.len() >= 2,
        "test fixture requires two parsed source fragments"
    );
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &format!("{sp}/sources/{document}"), &h.writer, None)
        )
        .await
        .0,
        StatusCode::CREATED
    );
    // A distinct source document must be attached BEFORE the analysis freezes
    // its input; this third fragment allows cross-document dependence tests.
    let second_document = Uuid::new_v4();
    let second_source_path = format!("/v1/workspaces/{}/sources/{second_document}", h.workspace);
    let (other_status, other_source) = call(
        h.router.clone(),
        source_req(
            &second_source_path,
            &h.writer,
            "01/02/2026, 09:00 - Carol: Another report about the same meeting\n",
        ),
    )
    .await;
    assert_eq!(other_status, StatusCode::CREATED);
    let other_evidence = other_source["fragment_ids"].as_array().unwrap();
    assert_eq!(other_evidence.len(), 1);
    let other_id = other_evidence[0].as_str().unwrap().parse::<Uuid>().unwrap();
    assert_eq!(
        call(
            h.router.clone(),
            req(
                "POST",
                &format!("{sp}/sources/{second_document}"),
                &h.writer,
                None,
            ),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let mut evidence = evidence;
    evidence.push(other_id);
    let analysis = Uuid::new_v4();
    let analysis_path = format!("{sp}/analysis-snapshots/{analysis}");
    assert_eq!(
        call(
            h.router.clone(),
            req(
                "POST",
                &analysis_path,
                &h.writer,
                Some(json!({"goal":"Compare interpretations"}))
            )
        )
        .await
        .0,
        StatusCode::CREATED
    );
    for evidence_id in &evidence {
        let examined = format!("{analysis_path}/examined-fragments/{evidence_id}");
        let (status, _) = call(h.router.clone(), req("POST", &examined, &h.writer, None)).await;
        assert_eq!(status, StatusCode::CREATED);
    }
    assert_eq!(call(h.router.clone(),req("POST",&format!("{analysis_path}/output"),&h.writer,
        Some(json!({"answer_text":"Several possibilities","coverage_note":"These two fragments examined"})))).await.0,
        StatusCode::CREATED);
    let claim = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claims \
        (workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,created_by_principal_id) \
        VALUES($1,$2,$3,$4,1,'INFERENCE','André might join a competitor',$5)"
    ).bind(h.workspace).bind(h.situation).bind(analysis).bind(claim).bind(h.principal)
    .execute(&h.admin).await.unwrap();
    for (i, evidence_id) in evidence.iter().take(2).enumerate() {
        let relation = if i == 0 {
            "SUPPORTS_AS_USED"
        } else {
            "CONTRADICTS_AS_USED"
        };
        sqlx::query(
            "INSERT INTO niccolo.situation_analysis_claim_fragments \
             (workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation,recorded_by_principal_id) \
             VALUES($1,$2,$3,$4,$5,$6,$7)"
        ).bind(h.workspace).bind(h.situation).bind(analysis).bind(claim).bind(evidence_id)
        .bind(relation).bind(h.principal).execute(&h.admin).await.unwrap();
    }
    Seed {
        analysis,
        claim,
        evidence,
        path: format!("{analysis_path}/claims/{claim}/epistemic/evaluate"),
    }
}
fn body(first: Uuid, second: Uuid) -> Value {
    json!({"human_declared_confidence_percent":90,
        "evidence":[
            {"evidence_id":first,"declared_strength":80,"rationale":"Primary document fragment"},
            {"evidence_id":second,"declared_strength":60,"rationale":"Contrary indication"}
        ]
    })
}

#[tokio::test]
async fn weighting_uses_only_real_claim_links_and_stored_relation() {
    let h = setup().await;
    let s = seed(&h).await;
    let b = body(s.evidence[0], s.evidence[1]);
    let (status, v) = call(
        h.router.clone(),
        req("POST", &s.path, &h.reader, Some(b.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["mode"], "GROUNDED_CLAIM_HEURISTIC");
    assert_eq!(v["assessment"]["claim_id"], s.claim.to_string());
    assert_eq!(v["assessment"]["inputs_checked_against_database"], true);
    assert_eq!(v["assessment"]["stance_derived_from_analysis_links"], true);
    assert_eq!(v["assessment"]["origins_grouped_by_document_id"], false);
    assert_eq!(v["assessment"]["derivation_graph_traversed"], true);
    assert_eq!(
        v["assessment"]["grouping_mode"],
        "RECORDED_DEPENDENCE_COMPONENT"
    );
    assert_eq!(
        v["assessment"]["recorded_dependence"]["clusters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        v["assessment"]["recorded_dependence"]["different_clusters_establish_independence"],
        false
    );
    assert!(v["assessment"]["source_graph_revision"].as_i64().unwrap() > 0);
    assert_eq!(v["assessment"]["independent_witnesses_established"], false);
    assert_eq!(v["assessment"]["establishes_claim_truth"], false);
    assert_eq!(v["assessment"]["report"]["support_points"], 80);
    assert_eq!(v["assessment"]["report"]["counter_points"], 60);
    assert_eq!(v["assessment"]["report"]["net_points"], 20);
    assert_eq!(
        v["assessment"]["report"]["origins"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        v["assessment"]["report"]["human_declared_confidence_percent"],
        90
    );
    assert_eq!(
        v["assessment"]["report"]["score_is_probability_of_truth"],
        false
    );
    assert_eq!(
        v["assessment"]["resolved_evidence"][0]["recorded_relation"],
        "SUPPORTS_AS_USED"
    );
    assert_eq!(
        v["assessment"]["resolved_evidence"][1]["recorded_relation"],
        "CONTRADICTS_AS_USED"
    );
    let (_, repeat) = call(
        h.router.clone(),
        req("POST", &s.path, &h.reader, Some(b.clone())),
    )
    .await;
    assert_eq!(repeat, v);

    let mut forged = b.clone();
    forged["evidence"][0]["evidence_id"] = json!(Uuid::new_v4());
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &s.path, &h.reader, Some(forged))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut spoof = b.clone();
    spoof["evidence"][0]["documented_origin_id"] = json!(Uuid::new_v4());
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &s.path, &h.reader, Some(spoof))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut invalid = b.clone();
    invalid["evidence"][0]["declared_strength"] = json!(101);
    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &s.path, &h.reader, Some(invalid))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let missing = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/claims/{}/epistemic/evaluate",
        h.workspace,
        h.situation,
        s.analysis,
        Uuid::new_v4()
    );
    assert_eq!(
        call(h.router.clone(), req("POST", &missing, &h.reader, Some(b)))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn explicit_grounded_score_record_is_immutable_retry_safe_and_readable() {
    let h = setup().await;
    let s = seed(&h).await;
    let score = Uuid::new_v4();
    let path = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/claims/{}/epistemic/records/{score}",
        h.workspace, h.situation, s.analysis, s.claim
    );
    let request_body = body(s.evidence[0], s.evidence[1]);

    assert_eq!(
        call(
            h.router.clone(),
            req("POST", &path, &h.reader, Some(request_body.clone()))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let (status, created) = call(
        h.router.clone(),
        req("POST", &path, &h.writer, Some(request_body.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["replayed"], false);
    assert_eq!(created["is_immutable_record"], true);
    assert_eq!(created["score_is_probability_of_truth"], false);
    assert_eq!(created["establishes_claim_truth"], false);
    assert_eq!(created["modifies_published_analysis"], false);
    assert_eq!(created["record"]["score_id"], score.to_string());
    assert_eq!(created["record"]["claim_id"], s.claim.to_string());
    assert_eq!(created["record"]["policy_source"], "BUILTIN_DEFAULT");
    assert_eq!(
        created["record"]["human_declared_confidence_percent"],
        90
    );
    assert_eq!(created["record"]["immutable_record"], true);
    assert_eq!(created["record"]["score_is_probability_of_truth"], false);
    assert_eq!(created["record"]["establishes_claim_truth"], false);
    assert_eq!(created["record"]["is_documentary_evidence"], false);
    assert_eq!(created["record"]["modifies_published_analysis"], false);
    assert_eq!(
        created["record"]["result"]["report"]["support_points"],
        80
    );
    assert_eq!(
        created["record"]["result"]["report"]["counter_points"],
        60
    );
    let digest = created["record"]["result_sha256"].as_str().unwrap();
    assert_eq!(digest.len(), 64);

    let (status, replay) = call(
        h.router.clone(),
        req("POST", &path, &h.writer, Some(request_body.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["record"]["result_sha256"], digest);

    let mut changed = request_body.clone();
    changed["evidence"][0]["declared_strength"] = json!(79);
    let (status, conflict) = call(
        h.router.clone(),
        req("POST", &path, &h.writer, Some(changed)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"], "grounded score ID conflict");

    let list_path = format!(
        "/v1/workspaces/{}/situations/{}/analysis-snapshots/{}/epistemic/records?limit=100",
        h.workspace, h.situation, s.analysis
    );
    let (status, page) = call(
        h.router.clone(),
        req("GET", &list_path, &h.reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let records = page["records"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["score_id"], score.to_string());
    assert_eq!(records[0]["result_sha256"], digest);

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
           FROM niccolo.situation_analysis_grounded_score_records
          WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3
            AND claim_id=$4 AND score_id=$5",
    )
    .bind(h.workspace)
    .bind(h.situation)
    .bind(s.analysis)
    .bind(s.claim)
    .bind(score)
    .fetch_one(&h.admin)
    .await
    .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn ambiguous_claim_evidence_links_fail_closed() {
    let h = setup().await;
    let s = seed(&h).await;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_fragments \
         (workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation,recorded_by_principal_id) \
         VALUES ($1,$2,$3,$4,$5,'CONTRADICTS_AS_USED',$6)"
    ).bind(h.workspace).bind(h.situation).bind(s.analysis).bind(s.claim).bind(s.evidence[0])
    .bind(h.principal).execute(&h.admin).await.unwrap();
    let (_, v) = call(
        h.router.clone(),
        req(
            "POST",
            &s.path,
            &h.reader,
            Some(body(s.evidence[0], s.evidence[1])),
        ),
    )
    .await;
    assert_eq!(v["error"], "conflicting claim-to-evidence use");
}

#[tokio::test]
async fn recorded_multi_parent_lineage_is_used_and_revocation_fails_closed() {
    let h = setup().await;
    let s = seed(&h).await;
    let common_ancestor = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) \
         VALUES($1,$2,'shared-parent','Recorded source antecedent','DIRECT_OBSERVATION')"
    ).bind(h.workspace).bind(common_ancestor).execute(&h.admin).await.unwrap();
    for selected in s.evidence.iter().take(2) {
        sqlx::query(
            "INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) \
             VALUES($1,$2,$3,'DERIVED_FROM')",
        )
        .bind(h.workspace)
        .bind(selected)
        .bind(common_ancestor)
        .execute(&h.admin)
        .await
        .unwrap();
    }
    let (status, v) = call(
        h.router.clone(),
        req(
            "POST",
            &s.path,
            &h.reader,
            Some(body(s.evidence[0], s.evidence[1])),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        v["assessment"]["recorded_dependence"]["clusters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        v["assessment"]["recorded_dependence"]["clusters"][0]["documented_roots"][0],
        common_ancestor.to_string()
    );
    assert_eq!(
        v["assessment"]["recorded_dependence"]["known_lineage_node_count"],
        3
    );
    assert_eq!(
        v["assessment"]["report"]["origins"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(v["assessment"]["derivation_graph_traversed"], true);
    assert_eq!(
        v["assessment"]["recorded_dependence"]["establishes_truth"],
        false
    );
    // This transaction's graph revision is visible for the future evaluation audit.
    assert!(v["assessment"]["source_graph_revision"].as_i64().unwrap() > 0);

    sqlx::query(
        "UPDATE niccolo.evidence SET admissible=false WHERE workspace_id=$1 AND evidence_id=$2",
    )
    .bind(h.workspace)
    .bind(common_ancestor)
    .execute(&h.admin)
    .await
    .unwrap();
    let (status, rejected) = call(
        h.router.clone(),
        req(
            "POST",
            &s.path,
            &h.reader,
            Some(body(s.evidence[0], s.evidence[1])),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(rejected["error"], "withdrawn recorded provenance");
}

#[tokio::test]
async fn cycles_in_recorded_provenance_are_rejected() {
    let h = setup().await;
    let s = seed(&h).await;
    sqlx::query(
        "INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) \
         VALUES($1,$2,$3,'DERIVED_FROM'),($1,$3,$2,'DERIVED_FROM')",
    )
    .bind(h.workspace)
    .bind(s.evidence[0])
    .bind(s.evidence[1])
    .execute(&h.admin)
    .await
    .unwrap();
    let (status, v) = call(
        h.router.clone(),
        req(
            "POST",
            &s.path,
            &h.reader,
            Some(body(s.evidence[0], s.evidence[1])),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["error"], "cyclic recorded provenance");
}

#[tokio::test]
async fn cross_document_common_ancestor_prevents_double_counting() {
    let h = setup().await;
    let s = seed(&h).await;
    assert!(s.evidence.len() >= 3);
    let other = s.evidence[2];

    // This second document's examined fragment supports the claim as used.
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_claim_fragments \
         (workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,'SUPPORTS_AS_USED',$6)"
    )
    .bind(h.workspace)
    .bind(h.situation)
    .bind(s.analysis)
    .bind(s.claim)
    .bind(other)
    .bind(h.principal)
    .execute(&h.admin)
    .await
    .unwrap();

    let two_sources = body(s.evidence[0], other);
    let (status, separate) = call(
        h.router.clone(),
        req("POST", &s.path, &h.reader, Some(two_sources.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        separate["assessment"]["recorded_dependence"]["clusters"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(separate["assessment"]["report"]["support_points"], 140);
    assert_eq!(separate["assessment"]["report"]["net_points"], 140);
    assert_eq!(
        separate["assessment"]["recorded_dependence"]["different_clusters_establish_independence"],
        false
    );

    // Even though the report came through another document, its recorded
    // derivation reveals a common ancestor and requires ONE support group.
    let parent = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) \
         VALUES($1,$2,'common-report','Recorded common ancestor','REPORTED_SPEECH')",
    )
    .bind(h.workspace)
    .bind(parent)
    .execute(&h.admin)
    .await
    .unwrap();
    for evidence in [s.evidence[0], other] {
        sqlx::query(
            "INSERT INTO niccolo.evidence_derivation(workspace_id,child_id,parent_id,relation) \
             VALUES($1,$2,$3,'DERIVED_FROM')",
        )
        .bind(h.workspace)
        .bind(evidence)
        .bind(parent)
        .execute(&h.admin)
        .await
        .unwrap();
    }
    let (status, merged) = call(
        h.router.clone(),
        req("POST", &s.path, &h.reader, Some(two_sources)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        merged["assessment"]["recorded_dependence"]["clusters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(merged["assessment"]["report"]["support_points"], 80);
    assert_eq!(merged["assessment"]["report"]["net_points"], 80);
    assert_eq!(
        merged["assessment"]["recorded_dependence"]["clusters"][0]["documented_roots"][0],
        parent.to_string()
    );
    assert_eq!(
        merged["assessment"]["recorded_dependence"]["different_clusters_establish_independence"],
        false
    );
    assert!(
        merged["assessment"]["source_graph_revision"]
            .as_i64()
            .unwrap()
            > separate["assessment"]["source_graph_revision"]
                .as_i64()
                .unwrap()
    );
}
