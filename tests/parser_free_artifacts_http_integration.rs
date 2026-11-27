#![cfg(feature = "pg-integration")]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use niccolo_evidence::{analysis_structure::AnalysisScope, api, authz, reasoning_packet};
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

struct Setup {
    router: Router,
    workspace: Uuid,
    situation: Uuid,
    token: String,
    service: PgPool,
}

async fn setup() -> Setup {
    setup_with_focus("Compare explicitly supplied descriptions").await
}

async fn setup_with_focus(focus: &str) -> Setup {
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let service = PgPoolOptions::new()
        .max_connections(8)
        .connect(&std::env::var("NICCOLO_PG_SERVICE_URL").unwrap())
        .await
        .unwrap();
    let workspace = Uuid::new_v4();
    let situation = Uuid::new_v4();
    let principal = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'artifact-http')")
        .bind(workspace)
        .execute(&admin)
        .await
        .unwrap();
    let token = authz::provision_token(&admin, principal, workspace, true, true, 1)
        .await
        .unwrap();
    let router = api::router(service.clone());
    let (status, _) = invoke(
        router.clone(),
        request(
            "POST",
            &format!("/v1/workspaces/{workspace}/situations/{situation}"),
            &token,
            Some(json!({"focus": focus})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    Setup {
        router,
        workspace,
        situation,
        token,
        service,
    }
}

fn request(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));
    let bytes = body.map_or_else(Vec::new, |value| serde_json::to_vec(&value).unwrap());
    if !bytes.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder.body(Body::from(bytes)).unwrap()
}

async fn invoke(router: Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

impl Setup {
    async fn case_request(&self, path: &str, body: Option<Value>) -> Value {
        let (method, expected) = if body.is_some() {
            ("POST", StatusCode::CREATED)
        } else {
            ("GET", StatusCode::OK)
        };
        let uri = format!(
            "/v1/workspaces/{}/situations/{}/{}",
            self.workspace, self.situation, path
        );
        let (status, result) = invoke(
            self.router.clone(),
            request(method, &uri, &self.token, body),
        )
        .await;
        assert_eq!(status, expected, "{method} {path}: {result}");
        result
    }

    async fn case_node(&self, kind: &str, text: &str, rationale: &str) -> Uuid {
        let object = Uuid::new_v4();
        self.case_request(
            &format!("epistemic/objects/{object}"),
            Some(json!({"kind":kind, "text":text, "rationale":rationale})),
        )
        .await;
        object
    }

    async fn case_source(&self, object: Uuid, label: &str, speaker: &str, excerpt: &str) -> Uuid {
        self.case_representation(
            object,
            "TRANSCRIPT_FILE",
            label,
            speaker,
            "HUMAN_TRANSCRIPTION",
            excerpt,
        )
        .await
    }

    async fn case_representation(
        &self,
        object: Uuid,
        artifact_type: &str,
        label: &str,
        scope: &str,
        representation_type: &str,
        excerpt: &str,
    ) -> Uuid {
        let artifact = Uuid::new_v4();
        self.case_request(
            &format!("artifacts/{artifact}"),
            Some(json!({"artifact_type":artifact_type, "label":label})),
        )
        .await;
        let representation = Uuid::new_v4();
        self.case_request(
            &format!("artifacts/{artifact}/representations/{representation}"),
            Some(json!({
                "representation_type":representation_type, "producer_type":"USER",
                "producer_label":"Owen Pike", "production_method":"Manually supplied excerpt",
                "scope_locator":scope, "content":excerpt, "completeness_status":"PARTIAL",
                "epistemic_qualification":"Attributed description", "limitations":"Excerpt only"
            })),
        )
        .await;
        self.case_request(
            &format!("epistemic/objects/{object}/artifact-provenance/{representation}"),
            Some(json!({"rationale":"This excerpt contains the attributed statement."})),
        )
        .await;
        representation
    }

    async fn case_argument(&self, conclusion: Uuid, rationale: &str) -> Uuid {
        let argument = Uuid::new_v4();
        self.case_request(
            &format!("epistemic/arguments/{argument}"),
            Some(json!({"conclusion_object_id":conclusion, "mode":"INDUCTIVE", "rationale":rationale})),
        )
        .await;
        argument
    }

    async fn case_ground(&self, argument: Uuid, premises: &[Uuid], rationale: &str) -> Uuid {
        let grounding = Uuid::new_v4();
        let members: Vec<_> = premises
            .iter()
            .map(|object| {
                json!({
                    "member_id":Uuid::new_v4(), "role":"PREMISE", "ground_object_id":object,
                    "evidence_id":null, "rationale":rationale
                })
            })
            .collect();
        self.case_request(
            &format!("epistemic/arguments/{argument}/grounding/{grounding}"),
            Some(json!({"effect":"SUPPORTS", "adequacy":"PARTIAL", "rationale":rationale, "members":members})),
        )
        .await;
        grounding
    }
}

fn case_row<'a>(snapshot: &'a Value, collection: &str, key: &str, id: Uuid) -> &'a Value {
    let id = json!(id);
    snapshot[collection]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[key] == id)
        .unwrap_or_else(|| panic!("missing {collection} row with {key}={id}"))
}

#[tokio::test]
async fn alder_bend_distinguishes_prominence_from_driver_identification() {
    let case =
        setup_with_focus("Why did Alder Bend collect more reports about the Vale truck?").await;
    let vale = case
        .case_node(
            "FINDING",
            "The Vale truck passed the college at 21:10.",
            "Physical passage only.",
        )
        .await;
    let driver = case
        .case_node(
            "HYPOTHESIS",
            "Frank Vale was driving the Vale truck.",
            "Investigator-authored driver hypothesis.",
        )
        .await;
    let vale_argument = case
        .case_argument(vale, "Assess the passage of the Vale truck.")
        .await;
    let driver_argument = case
        .case_argument(driver, "Assess the driver's identity.")
        .await;

    let mae = case
        .case_node(
            "PROPOSITION",
            "Mae reported seeing the Vale logo at 21:10.",
            "Separately collected observation of the truck.",
        )
        .await;
    let jules = case
        .case_node(
            "PROPOSITION",
            "Jules reported seeing the Vale logo at 21:10.",
            "Separately collected observation of the truck.",
        )
        .await;
    let mae_source = case
        .case_source(
            mae,
            "Mae interview",
            "Mae Dorsey",
            "I saw the logo. I guessed Frank because it is his company.",
        )
        .await;
    let jules_source = case
        .case_source(
            jules,
            "Jules interview",
            "Jules Navarro",
            "The sign said Vale. The first person I thought of was Frank.",
        )
        .await;
    assert_ne!(mae_source, jules_source);
    let mae_route = case
        .case_ground(vale_argument, &[mae], "Mae reported the logo.")
        .await;
    let jules_route = case
        .case_ground(vale_argument, &[jules], "Jules reported the logo.")
        .await;

    let prominence = case
        .case_node(
            "PROPOSITION",
            "Mae and Jules know Frank as the owner of Vale Salvage.",
            "Scope: these two people when initially interviewed.",
        )
        .await;
    for (speaker, source) in [("Mae", mae_source), ("Jules", jules_source)] {
        let inference = case
            .case_node(
                "PROPOSITION",
                &format!("{speaker} guessed Frank was driving."),
                "Attributed inference from the familiar owner, without seeing the driver.",
            )
            .await;
        case.case_request(
            &format!("epistemic/objects/{inference}/artifact-provenance/{source}"),
            Some(
                json!({"rationale":"The same excerpt separately records the speaker's inference."}),
            ),
        )
        .await;
        case.case_request(&format!("epistemic/relations/{}", Uuid::new_v4()), Some(json!({
            "family":"CONTEXTUAL", "kind":"RELEVANT_TO", "left_object_id":prominence,
            "right_object_id":inference, "rationale":"Shared association explains the inference, not visual identification."
        }))).await;
    }

    let leon = case
        .case_node(
            "PROPOSITION",
            "Leon claimed he saw Frank at the wheel.",
            "Initially presented as an identification.",
        )
        .await;
    let leon_source = case
        .case_source(
            leon,
            "Leon initial interview",
            "Leon Pratt",
            "I saw Frank Vale at the wheel.",
        )
        .await;
    let leon_route = case
        .case_ground(
            driver_argument,
            &[leon],
            "Only claimed visual identification.",
        )
        .await;
    let request_scope = case
        .case_node(
            "PROPOSITION",
            "Owen's first appeal requested reports about the Vale truck.",
            "Scope of the initial collection.",
        )
        .await;
    let appeal_source = case.case_source(request_scope, "Initial police appeal", "Text of the appeal",
        "Anyone who saw a Vale Salvage tow truck near the college should contact the investigator.").await;
    let before = case.case_request("epistemic/workbench", None).await;
    let initial_driver = case_row(
        &before,
        "reasoning_diagnostics",
        "conclusion_object_id",
        driver,
    );
    assert_eq!(initial_driver["routes"].as_array().unwrap().len(), 1);
    assert_eq!(
        initial_driver["routes"][0]["grounding_set_id"],
        json!(leon_route)
    );
    assert!(
        before["reassessment_impacts"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let correction = case
        .case_node(
            "PROPOSITION",
            "Leon now says he did not see the cab.",
            "Withdraws the identification in his own earlier statement.",
        )
        .await;
    let correction_source = case
        .case_source(
            correction,
            "Leon follow-up",
            "Leon Pratt",
            "I never saw the cab. The table went quiet when I mentioned Frank.",
        )
        .await;
    let reward = case
        .case_node(
            "PROPOSITION",
            "Leon says people at the cafe stopped to listen when he named Frank.",
            "An attributed report about the audience's reaction.",
        )
        .await;
    case.case_request(
        &format!("epistemic/objects/{reward}/artifact-provenance/{correction_source}"),
        Some(
            json!({"rationale":"Leon describes the audience reaction in the follow-up interview."}),
        ),
    )
    .await;
    let motive = case.case_node("HYPOTHESIS", "Leon may have overstated his account to attract attention.",
        "Investigator-authored possible explanation; audience reaction does not establish motive.").await;
    let motive_argument = case
        .case_argument(motive, "Assess the possible incentive.")
        .await;
    case.case_ground(
        motive_argument,
        &[reward],
        "The claimed social reaction is suggestive only.",
    )
    .await;
    let retraction = Uuid::new_v4();
    case.case_request(
        &format!("epistemic/relations/{retraction}"),
        Some(json!({
            "family":"GENEALOGY", "kind":"RETRACTS", "left_object_id":correction,
            "right_object_id":leon, "rationale":"Leon retracts the purported identification."
        })),
    )
    .await;

    let pine = case
        .case_node(
            "FINDING",
            "A Pine Towing truck passed the college at 21:13.",
            "Physical passage only.",
        )
        .await;
    let mara = case
        .case_node(
            "PROPOSITION",
            "Mara reported seeing the Pine logo at 21:13.",
            "Collected after Owen broadened the appeal.",
        )
        .await;
    let mara_source = case
        .case_source(
            mara,
            "Mara interview after broadened appeal",
            "Mara Ellis",
            "I saw Pine Towing at 21:13. I thought the police only wanted the Vale truck.",
        )
        .await;
    let pine_argument = case
        .case_argument(pine, "Assess the passage of the Pine truck.")
        .await;
    let pine_route = case
        .case_ground(pine_argument, &[mara], "Mara reported the Pine logo.")
        .await;
    let camera = case.case_node("PROPOSITION", "Owen reports that the college camera shows Vale at 21:10 and Pine at 21:13, without a view of either cab.",
        "The investigator supplied a description of the external record.").await;
    let camera_source = case
        .case_representation(
            camera,
            "VIDEO",
            "College exterior camera",
            "Tuesday 21:10–21:13",
            "HUMAN_DESCRIPTION",
            "Vale at 21:10 and Pine at 21:13. The framing does not show either cab.",
        )
        .await;
    let vale_camera_route = case
        .case_ground(
            vale_argument,
            &[camera],
            "Description supports Vale's passage.",
        )
        .await;
    let pine_camera_route = case
        .case_ground(
            pine_argument,
            &[camera],
            "Description supports Pine's passage.",
        )
        .await;
    let selection = case
        .case_node(
            "HYPOTHESIS",
            "The initial appeal contributed to reports concentrating on Vale.",
            "Attributable explanation of the collection gap.",
        )
        .await;
    let selection_argument = case
        .case_argument(selection, "Assess the effect of the initial appeal.")
        .await;
    let selection_route = case
        .case_ground(
            selection_argument,
            &[request_scope, mara],
            "Scope of request and Mara's attributed reason for waiting.",
        )
        .await;

    let after = case.case_request("epistemic/workbench", None).await;
    let impacts = after["reassessment_impacts"].as_array().unwrap();
    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0]["trigger_relation_id"], json!(retraction));
    assert_eq!(impacts[0]["grounding_set_id"], json!(leon_route));
    assert_eq!(impacts[0]["conclusion_object_id"], json!(driver));
    assert_eq!(impacts[0]["prior_object_id"], json!(leon));
    assert_eq!(impacts[0]["revision_object_id"], json!(correction));
    let revised_driver = case_row(&after, "reasoning_diagnostics", "conclusion_object_id", driver);
    let route = case_row(revised_driver, "routes", "grounding_set_id", leon_route);
    assert_eq!(route["reassessment"]["unreviewed"], 1);
    for conclusion in [vale, pine] {
        let passage = case_row(&after, "reasoning_diagnostics", "conclusion_object_id", conclusion);
        assert_eq!(passage["has_supporting_routes"], true);
        for route in passage["routes"].as_array().unwrap() {
            assert_eq!(route["reassessment"]["impacted"], 0);
        }
    }
    for (object, source) in [
        (mae, mae_source),
        (jules, jules_source),
        (leon, leon_source),
        (correction, correction_source),
        (mara, mara_source),
        (request_scope, appeal_source),
        (camera, camera_source),
    ] {
        let link = case_row(&after, "artifact_provenance", "object_id", object);
        assert_eq!(link["representation_id"], json!(source));
    }
    for (route, premise) in [
        (mae_route, mae),
        (jules_route, jules),
        (pine_route, mara),
        (vale_camera_route, camera),
        (pine_camera_route, camera),
    ] {
        let stored = case_row(&after, "grounding_sets", "grounding_set_id", route);
        assert_eq!(stored["members"][0]["ground_object_id"], json!(premise));
    }
    let stored = case_row(
        &after,
        "grounding_sets",
        "grounding_set_id",
        selection_route,
    );
    let members = stored["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    for premise in [request_scope, mara] {
        assert!(
            members
                .iter()
                .any(|member| member["ground_object_id"] == json!(premise))
        );
    }
    assert_eq!(
        case_row(&after, "nodes", "object_id", selection)["kind"],
        "HYPOTHESIS"
    );
    assert_eq!(
        case_row(&after, "nodes", "object_id", motive)["kind"],
        "HYPOTHESIS"
    );
    assert!(
        !before["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["object_id"] == json!(pine))
    );
}

#[tokio::test]
async fn http_flow_records_human_material_without_accepting_or_reading_artifact_bytes() {
    let setup = setup().await;
    let video = Uuid::new_v4();
    let transcript = Uuid::new_v4();
    let artifact_url = format!(
        "/v1/workspaces/{}/situations/{}/artifacts/{video}",
        setup.workspace, setup.situation
    );
    let artifact_body = json!({
        "artifact_type":"VIDEO",
        "label":"meeting.mp4",
        "declared_mime_type":"video/mp4",
        "declared_byte_length":18000000000_i64,
        "declared_sha256":null
    });
    let (status, created) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &artifact_url,
            &setup.token,
            Some(artifact_body.clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["source_bytes_stored"], false);
    assert_eq!(created["artifact_was_inspected"], false);
    assert_eq!(created["parsers_invoked"], false);
    let (status, replay) = invoke(
        setup.router.clone(),
        request("POST", &artifact_url, &setup.token, Some(artifact_body)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["replayed"], true);

    let transcript_url = format!(
        "/v1/workspaces/{}/situations/{}/artifacts/{transcript}",
        setup.workspace, setup.situation
    );
    let (status, _) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &transcript_url,
            &setup.token,
            Some(json!({
                "artifact_type":"TRANSCRIPT_FILE",
                "label":"meeting-transcript.md",
                "declared_mime_type":"text/markdown",
                "declared_byte_length":null,
                "declared_sha256":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let reference = Uuid::new_v4();
    let storage_url = format!("{artifact_url}/storage-references/{reference}");
    let (status, storage) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &storage_url,
            &setup.token,
            Some(json!({
                "access_event_id":Uuid::new_v4(),
                "storage_type":"S3",
                "locator":"s3://client-video/meeting.mp4",
                "provider":"AWS",
                "environment_label":"client production",
                "resolver_profile":null,
                "notes":"declared by the user",
                "access_status":"NONE",
                "access_note":"Niccolò has no access"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(storage["locator_was_dereferenced"], false);
    assert_eq!(storage["access_status_is_human_declared"], true);

    let representation = Uuid::new_v4();
    let (status, represented) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &format!("{artifact_url}/representations/{representation}"),
            &setup.token,
            Some(json!({
                "representation_type":"HUMAN_DESCRIPTION",
                "producer_type":"USER",
                "producer_label":"Renato",
                "production_method":"described while watching externally",
                "scope_locator":"00:13:20-00:13:45",
                "content":"João appears to leave the room.",
                "completeness_status":"PARTIAL",
                "epistemic_qualification":"human report",
                "limitations":"bounded interval"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(represented["content_was_explicitly_supplied"], true);
    assert_eq!(represented["artifact_was_parsed"], false);
    assert_eq!(represented["representation_establishes_truth"], false);

    let epistemic = format!(
        "/v1/workspaces/{}/situations/{}/epistemic",
        setup.workspace, setup.situation,
    );
    let object = Uuid::new_v4();
    let object_path = format!("{epistemic}/objects/{object}");
    let (status, _) = invoke(
        setup.router.clone(),
        request("POST", &object_path, &setup.token, Some(json!({
            "kind":"PROPOSITION", "text":"A description suggests that João left the room.",
            "rationale":"Record the attributed statement."
        }))),
    ).await;
    assert_eq!(status, StatusCode::CREATED);

    let link = format!("{object_path}/artifact-provenance/{representation}");
    let rationale = "Scope contains the described departure.";
    for (uri, note, expected) in [
        (link.clone(), rationale, StatusCode::CREATED),
        (link.clone(), rationale, StatusCode::OK),
        (link, "A different attribution rationale", StatusCode::CONFLICT),
        (format!("{object_path}/artifact-provenance/{}", Uuid::new_v4()), rationale, StatusCode::NOT_FOUND),
    ] {
        let (status, _) = invoke(
            setup.router.clone(),
            request("POST", &uri, &setup.token, Some(json!({"rationale":note}))),
        ).await;
        assert_eq!(status, expected);
    }
    let (status, workbench) = invoke(
        setup.router.clone(),
        request("GET", &format!("{epistemic}/workbench"), &setup.token, None),
    ).await;
    assert_eq!(status, StatusCode::OK);
    let provenance = workbench["artifact_provenance"].as_array().unwrap();
    assert_eq!(provenance.len(), 1);
    assert_eq!(provenance[0]["object_id"], object.to_string());
    assert_eq!(provenance[0]["artifact_id"], video.to_string());
    assert_eq!(provenance[0]["representation_id"], representation.to_string());
    assert_eq!(provenance[0]["scope_locator"], "00:13:20-00:13:45");
    assert_eq!(provenance[0]["qualification"], "human report");
    assert_eq!(provenance[0]["rationale"], rationale);

    let relationship = Uuid::new_v4();
    let (status, related) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &format!(
                "/v1/workspaces/{}/situations/{}/artifact-relationships/{relationship}",
                setup.workspace, setup.situation
            ),
            &setup.token,
            Some(json!({
                "subject_artifact_id":transcript,
                "object_artifact_id":video,
                "relationship_type":"CLAIMED_TO_BE_TRANSCRIPTION_OF",
                "qualification":"declared by the user"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(related["relationship_is_verified"], false);

    let (status, list) = invoke(
        setup.router.clone(),
        request(
            "GET",
            &format!(
                "/v1/workspaces/{}/situations/{}/artifacts",
                setup.workspace, setup.situation
            ),
            &setup.token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(list["parser_free_mvp"], true);
    assert_eq!(list["source_bytes_accepted"], false);
    assert_eq!(list["locators_dereferenced"], false);
    assert_eq!(list["storage_references"].as_array().unwrap().len(), 1);
    assert_eq!(list["storage_references"][0]["access_status"], "NONE");
    assert_eq!(
        list["storage_references"][0]["locator_was_dereferenced"],
        false
    );
    assert_eq!(list["representations"].as_array().unwrap().len(), 1);
    assert_eq!(
        list["representations"][0]["content_was_explicitly_supplied"],
        true
    );
    assert_eq!(list["representations"][0]["artifact_was_parsed"], false);
    assert_eq!(list["relationships"].as_array().unwrap().len(), 1);
    assert_eq!(list["relationships"][0]["relationship_is_verified"], false);

    let analysis = Uuid::new_v4();
    let (status, frozen) = invoke(
        setup.router,
        request(
            "POST",
            &format!(
                "/v1/workspaces/{}/situations/{}/analysis-snapshots/{analysis}",
                setup.workspace, setup.situation
            ),
            &setup.token,
            Some(json!({"goal":"Compare the human-supplied material"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let digest = authz::credential_digest(&setup.token).unwrap();
    let packet = authz::with_auth_digest(digest, async {
        reasoning_packet::build(
            &setup.service,
            AnalysisScope {
                workspace: setup.workspace,
                situation: setup.situation,
                analysis,
            },
        )
        .await
        .unwrap()
    })
    .await;
    assert_eq!(packet.version, 2);
    assert_eq!(
        packet.frozen_input.revision,
        frozen["input_revision"].as_i64().unwrap()
    );
    assert_eq!(packet.frozen_input.artifacts.len(), 2);
    assert_eq!(packet.frozen_input.artifact_storage_references.len(), 1);
    assert_eq!(packet.frozen_input.artifact_representations.len(), 1);
    assert_eq!(packet.frozen_input.artifact_relationships.len(), 1);
    assert_eq!(packet.frozen_input.artifact_original_bytes_included, 0);
    assert!(!packet.frozen_input.artifact_packet_truncated);
    assert!(
        packet
            .frozen_input
            .artifacts
            .iter()
            .all(|artifact| !artifact.source_bytes_included && !artifact.artifact_was_inspected)
    );
    assert!(
        packet
            .frozen_input
            .artifact_representations
            .iter()
            .all(
                |representation| representation.content_was_explicitly_supplied
                    && !representation.artifact_was_parsed
                    && !representation.representation_establishes_truth
            )
    );
}

#[tokio::test]
async fn durable_presigned_credentials_are_rejected_at_the_http_boundary() {
    let setup = setup().await;
    let artifact = Uuid::new_v4();
    let artifact_url = format!(
        "/v1/workspaces/{}/situations/{}/artifacts/{artifact}",
        setup.workspace, setup.situation
    );
    let (status, _) = invoke(
        setup.router.clone(),
        request(
            "POST",
            &artifact_url,
            &setup.token,
            Some(json!({
                "artifact_type":"DOCUMENT",
                "label":"private.pdf",
                "declared_mime_type":null,
                "declared_byte_length":null,
                "declared_sha256":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, error) = invoke(
        setup.router,
        request(
            "POST",
            &format!("{artifact_url}/storage-references/{}", Uuid::new_v4()),
            &setup.token,
            Some(json!({
                "access_event_id":Uuid::new_v4(),
                "storage_type":"HTTPS",
                "locator":"https://example.invalid/private.pdf?X-Amz-Signature=secret",
                "provider":null,
                "environment_label":null,
                "resolver_profile":null,
                "notes":null,
                "access_status":"UNVERIFIED",
                "access_note":null
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "invalid artifact metadata");
}

#[tokio::test]
async fn mvp_runtime_has_no_upload_attachment_download_or_parser_routes() {
    let setup = setup().await;
    let document = Uuid::new_v4();
    let workspace_sources = format!("/v1/workspaces/{}/sources", setup.workspace);
    let retired = [
        ("POST", format!("{workspace_sources}/{document}")),
        ("POST", format!("{workspace_sources}/pdf-preview")),
        ("GET", format!("{workspace_sources}/{document}/pdf-preview")),
        ("GET", format!("{workspace_sources}/{document}/original")),
        (
            "POST",
            format!(
                "/v1/workspaces/{}/situations/{}/sources/{document}",
                setup.workspace, setup.situation
            ),
        ),
    ];

    for (method, uri) in retired {
        let request = Request::builder()
            .method(method)
            .uri(&uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", setup.token))
            .header(header::CONTENT_TYPE, "application/pdf")
            .body(Body::from("%PDF bytes that the MVP must never accept"))
            .unwrap();
        let (status, _) = invoke(setup.router.clone(), request).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "retired route remained: {uri}");
    }
}
