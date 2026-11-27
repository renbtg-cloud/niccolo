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
async fn alder_bend_queries_recognizability_by_audience_and_two_dates() {
    let mut case = setup_with_focus("What could each audience recognize, and when?").await;
    if let Ok(url) = std::env::var("NICCOLO_PG_SECURE_URL") {
        let restricted = PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .unwrap();
        case.router = api::router(restricted);
    }

    let [frank, residents, students] = std::array::from_fn(|_| Uuid::new_v4());
    for (id, label) in [
        (frank, "Frank Vale"),
        (residents, "Long-time residents of Alder Bend"),
        (students, "Night students at the community college"),
    ] {
        let uri = format!("/v1/workspaces/{}/actors/{id}", case.workspace);
        let (status, body) = invoke(
            case.router.clone(),
            request(
                "POST",
                &uri,
                &case.token,
                Some(json!({"display_label":label})),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let neighborhood = case
        .case_node(
            "PROPOSITION",
            "Mae and Theo report recognizing Frank by face.",
            "Their reports explicitly refer to before the disappearance.",
        )
        .await;
    let neighborhood_source = case
        .case_source(
            neighborhood,
            "Neighborhood interviews",
            "Mae and Theo",
            "Before Daniel went missing, each of us knew Frank by sight.",
        )
        .await;
    let roll_call = case
        .case_node(
            "PROPOSITION",
            "Jules reports knowing Frank's name before the disappearance.",
            "Jules did not identify a face in this interview.",
        )
        .await;
    let name_source = case
        .case_source(
            roll_call,
            "Jules interview",
            "Jules Navarro",
            "I already knew the name Frank Vale.",
        )
        .await;
    let photo = case
        .case_node("PROPOSITION", "Mara reports seeing Frank's labeled photo after the disappearance.",
            "This describes the person and picture she recalls encountering, without a recognition claim.")
        .await;
    let photo_source = case
        .case_source(photo, "Mara interview on newspaper coverage", "Mara Ellis",
            "I opened the story on the 19th. The caption identified Frank. I saw the picture before my interview.")
        .await;

    let create = [
        json!({
            "subject_actor_id":frank, "audience_actor_id":residents,
            "cue":"FACE", "observation":"RECOGNITION_REPORTED", "audience_scope":"Mae and Theo only",
            "effective_from":"2026-11-15", "effective_through":"2026-11-17", "available_on":"2026-11-18",
            "claim_object_id":neighborhood, "representation_id":neighborhood_source
        }),
        json!({
            "subject_actor_id":frank, "audience_actor_id":students,
            "cue":"NAME", "observation":"RECOGNITION_REPORTED", "audience_scope":"Jules only",
            "effective_from":"2026-11-16", "effective_through":"2026-11-17", "available_on":"2026-11-18",
            "claim_object_id":roll_call, "representation_id":name_source
        }),
        json!({
            "subject_actor_id":frank, "audience_actor_id":students,
            "cue":"FACE", "observation":"EXPOSURE_REPORTED", "audience_scope":"Mara only",
            "effective_from":"2026-11-19", "effective_through":"2026-11-19", "available_on":"2026-11-20",
            "claim_object_id":photo, "representation_id":photo_source
        }),
    ];
    let ids: [Uuid; 3] = std::array::from_fn(|_| Uuid::new_v4());
    for (id, input) in ids.into_iter().zip(&create) {
        case.case_request(
            &format!("epistemic/public-prominence/{id}"),
            Some(input.clone()),
        )
        .await;
    }

    let lookup = |audience, cue, on, known_through| {
        format!(
            "epistemic/public-prominence?subject_actor_id={frank}&audience_actor_id={audience}&cue={cue}&on={on}&known_through={known_through}"
        )
    };
    for (audience, cue, on, as_of, expected) in [
        (residents, "FACE", "2026-11-17", "2026-11-17", None),
        (residents, "FACE", "2026-11-17", "2026-11-18", Some(0)),
        (students, "NAME", "2026-11-17", "2026-11-18", Some(1)),
        (students, "FACE", "2026-11-17", "2026-11-20", None),
        (students, "FACE", "2026-11-19", "2026-11-19", None),
        (students, "FACE", "2026-11-19", "2026-11-20", Some(2)),
        (residents, "FACE", "2026-11-19", "2026-11-20", None),
    ] {
        let result = case
            .case_request(&lookup(audience, cue, on, as_of), None)
            .await;
        assert_eq!(result["query"]["on"], on);
        assert_eq!(result["query"]["known_through"], as_of);
        assert_eq!(result["truncated"], false);
        let reports = result["records"].as_array().unwrap();
        match expected {
            Some(i) => {
                assert_eq!(result["coverage"], "RECORDED_REPORTS");
                assert_eq!(reports.len(), 1);
                assert_eq!(reports[0]["record_id"], json!(ids[i]));
                for (key, value) in create[i].as_object().unwrap() {
                    assert_eq!(&reports[0][key], value, "returned field {key}");
                }
            }
            None => {
                assert_eq!(result["coverage"], "NO_RECORDED_REPORTS");
                assert!(reports.is_empty());
            }
        }
    }

    let base = format!(
        "/v1/workspaces/{}/situations/{}/",
        case.workspace, case.situation
    );
    let mut foreign_basis = create[0].clone();
    foreign_basis["representation_id"] = json!(name_source);
    let mut changed_scope = create[0].clone();
    changed_scope["audience_scope"] = json!("All residents");
    for (id, payload, expected) in [
        (ids[0], create[0].clone(), StatusCode::OK),
        (ids[0], changed_scope, StatusCode::CONFLICT),
        (Uuid::new_v4(), foreign_basis, StatusCode::NOT_FOUND),
    ] {
        let uri = format!("{base}epistemic/public-prominence/{id}");
        let (status, body) = invoke(
            case.router.clone(),
            request("POST", &uri, &case.token, Some(payload)),
        )
        .await;
        assert_eq!(status, expected, "{body}");
    }
    let invalid_calendar = lookup(students, "FACE", "2026-02-29", "2026-11-20");
    let (status, _) = invoke(
        case.router.clone(),
        request(
            "GET",
            &format!("{base}{invalid_calendar}"),
            &case.token,
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let reader = authz::provision_token(&admin, Uuid::new_v4(), case.workspace, true, false, 1)
        .await
        .unwrap();
    let historical = lookup(residents, "FACE", "2026-11-17", "2026-11-18");
    let (status, body) = invoke(
        case.router.clone(),
        request("GET", &format!("{base}{historical}"), &reader, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["records"][0]["record_id"], json!(ids[0]));
    let (status, _) = invoke(
        case.router.clone(),
        request(
            "POST",
            &format!("{base}epistemic/public-prominence/{}", Uuid::new_v4()),
            &reader,
            Some(create[0].clone()),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn alder_bend_reviews_only_the_detail_reachable_from_prior_exposure() {
    let mut case =
        setup_with_focus("Could the archive photo account for a detail in Mara's account?").await;
    if let Ok(url) = std::env::var("NICCOLO_PG_SECURE_URL") {
        case.router = api::router(PgPoolOptions::new().connect(&url).await.unwrap());
    }
    let frank = Uuid::new_v4();
    let mara = Uuid::new_v4();
    for (actor, label) in [(frank, "Frank Vale"), (mara, "Mara Ellis")] {
        let uri = format!("/v1/workspaces/{}/actors/{actor}", case.workspace);
        let (status, body) = invoke(
            case.router.clone(),
            request(
                "POST",
                &uri,
                &case.token,
                Some(json!({"display_label":label})),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let hat = case
        .case_node(
            "PROPOSITION",
            "Mara reports a red hat in the cab.",
            "One detail of the interview.",
        )
        .await;
    let truck = case
        .case_node(
            "PROPOSITION",
            "Mara reports a truck passing the college.",
            "A separate observation in the same interview.",
        )
        .await;
    let interview = case
        .case_source(
            hat,
            "Mara's initial testimony",
            "Mara Ellis",
            "I saw a truck pass the college. I remember a red hat in the cab.",
        )
        .await;
    case.case_request(
        &format!("epistemic/objects/{truck}/artifact-provenance/{interview}"),
        Some(json!({"rationale":"The same testimony also records a distinct observation."})),
    )
    .await;
    let hat_finding = case
        .case_node(
            "FINDING",
            "The testimony describes a red hat in the cab.",
            "Depends on the detail.",
        )
        .await;
    let passage = case
        .case_node(
            "FINDING",
            "A truck passed the college.",
            "Depends on the truck observation.",
        )
        .await;
    let identity = case
        .case_node(
            "HYPOTHESIS",
            "The driver may have been Frank.",
            "Authored investigative hypothesis.",
        )
        .await;
    let hat_argument = case
        .case_argument(hat_finding, "Assess the reported detail.")
        .await;
    let passage_argument = case
        .case_argument(passage, "Assess the truck observation.")
        .await;
    let identity_argument = case
        .case_argument(identity, "Consider what the hat might indicate.")
        .await;
    let hat_route = case
        .case_ground(hat_argument, &[hat], "Uses the hat detail.")
        .await;
    let passage_route = case
        .case_ground(passage_argument, &[truck], "Uses the truck observation.")
        .await;
    let identity_route = case
        .case_ground(
            identity_argument,
            &[hat_finding],
            "Relies on the reported hat finding.",
        )
        .await;

    let photo = case
        .case_node(
            "PROPOSITION",
            "The newspaper archive photo shows Frank in a red hat.",
            "An attributed description of the candidate source.",
        )
        .await;
    let picture = case
        .case_representation(
            photo,
            "IMAGE",
            "Alder Bend newspaper archive photo",
            "Image accompanying the article",
            "HUMAN_DESCRIPTION",
            "Frank wears a red hat in the archive photograph.",
        )
        .await;
    let workbench_before = case.case_request("epistemic/workbench", None).await;

    let uri = format!(
        "/v1/workspaces/{}/situations/{}/epistemic/detail-exposure-review",
        case.workspace, case.situation
    );
    for (from, through, expected) in [
        ("2026-11-19", "2026-11-19", "REVIEW_CANDIDATE"),
        ("2026-11-22", "2026-11-22", "EXPOSURE_AFTER_TESTIMONY"),
        ("2026-11-19", "2026-11-21", "ORDER_UNCERTAIN"),
        ("2026-11-20", "2026-11-20", "ORDER_UNCERTAIN"),
    ] {
        let exposure_claim = case
            .case_node(
                "PROPOSITION",
                &format!("Mara reports seeing the picture between {from} and {through}."),
                "Alternative interval tested against the same interview.",
            )
            .await;
        let basis = case
            .case_source(
                exposure_claim,
                "Mara follow-up on photo exposure",
                "Mara Ellis",
                &format!("I saw the archive photo between {from} and {through}."),
            )
            .await;
        let exposure_id = Uuid::new_v4();
        case.case_request(
            &format!("epistemic/public-prominence/{exposure_id}"),
            Some(json!({
                "subject_actor_id":frank, "audience_actor_id":mara, "cue":"FACE",
                "observation":"EXPOSURE_REPORTED", "audience_scope":"Mara only",
                "effective_from":from, "effective_through":through, "available_on":"2026-11-23",
                "claim_object_id":exposure_claim, "representation_id":basis
            })),
        )
        .await;
        let payload = json!({
            "witness_actor_id":mara, "testimony_detail":{"object_id":hat, "representation_id":interview},
            "candidate_source":{"object_id":photo, "representation_id":picture},
            "exposure_record_id":exposure_id, "testimony_from":"2026-11-20",
            "testimony_through":"2026-11-20", "known_through":"2026-11-23"
        });
        let (status, body) = invoke(
            case.router.clone(),
            request("POST", &uri, &case.token, Some(payload.clone())),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["disposition"], expected);
        assert_eq!(body["needs_finer_timing"], expected == "ORDER_UNCERTAIN");
        assert_eq!(body["exposure"]["representation_id"], json!(basis));
        assert_eq!(
            body["candidate_source"]["representation_id"],
            json!(picture)
        );
        let routes = body["affected_routes"].as_array().unwrap();
        let needs_review = expected == "REVIEW_CANDIDATE";
        assert_eq!(body["detail_needs_review"], needs_review);
        if needs_review {
            assert_eq!(routes.len(), 2);
            for (route, conclusion) in [(hat_route, hat_finding), (identity_route, identity)] {
                let affected = case_row(&body, "affected_routes", "grounding_set_id", route);
                assert_eq!(affected["conclusion_object_id"], json!(conclusion));
            }
        } else {
            assert!(routes.is_empty());
        }

        let mut earlier = payload.clone();
        earlier["known_through"] = json!("2026-11-20");
        let (status, historical) = invoke(
            case.router.clone(),
            request("POST", &uri, &case.token, Some(earlier)),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{historical}");
        assert_eq!(
            historical["disposition"],
            "EXPOSURE_NOT_AVAILABLE_AT_CUTOFF"
        );
        assert_eq!(historical["exposure"], Value::Null);
        assert_eq!(historical["candidate_source"], Value::Null);
        assert_eq!(historical["affected_routes"], json!([]));

        let mut wrong_witness = payload.clone();
        wrong_witness["witness_actor_id"] = json!(frank);
        let (status, _) = invoke(
            case.router.clone(),
            request("POST", &uri, &case.token, Some(wrong_witness)),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let mut unlinked_source = payload;
        unlinked_source["candidate_source"]["representation_id"] = json!(interview);
        let (status, _) = invoke(
            case.router.clone(),
            request("POST", &uri, &case.token, Some(unlinked_source)),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
    let after = case.case_request("epistemic/workbench", None).await;
    assert_eq!(
        after["reassessment_impacts"],
        workbench_before["reassessment_impacts"]
    );
    assert_eq!(after["grounding_sets"], workbench_before["grounding_sets"]);
    let still_usable = case_row(&after, "grounding_sets", "grounding_set_id", passage_route);
    assert_eq!(still_usable["members"][0]["ground_object_id"], json!(truck));
}

struct AlderSupportModel {
    before: Uuid,
    invalid: Uuid,
    hat: (Uuid, Uuid),
    alternate: (Uuid, Uuid),
    truck: (Uuid, Uuid),
    suspended_by: Uuid,
}

impl niccolo_evidence::model_adapter::ReasoningModel for AlderSupportModel {
    fn provider(&self) -> &str {
        "alder-bend-fixture"
    }

    fn model(&self) -> &str {
        "support-replay"
    }

    fn invoke<'a>(
        &'a self,
        request: &'a niccolo_evidence::model_adapter::ModelRequest,
    ) -> niccolo_evidence::model_adapter::ModelFuture<'a> {
        use niccolo_evidence::model_adapter::{DeterministicModel, ModelResponse};
        Box::pin(async move {
            if request.purpose != "SYNTHESIZE" {
                let fallback = DeterministicModel::new("alder-bend-fixture", "support-replay")?;
                return fallback.invoke(request).await;
            }
            let analysis = &request.input["analysis_id"];
            let initial = analysis == &json!(self.before);
            let invalid = analysis == &json!(self.invalid);
            let frozen = &request.input["reasoning_packet"]["frozen_input"]["workbench_support"];
            let hat = case_row(frozen, "routes", "grounding_set_id", self.hat.1);
            assert_eq!(
                hat["suspended_by_decision_id"],
                if initial {
                    Value::Null
                } else {
                    json!(self.suspended_by)
                }
            );
            assert_eq!(hat["grounds"].as_array().unwrap().is_empty(), !initial);
            for route in [self.alternate, self.truck] {
                let row = case_row(frozen, "routes", "grounding_set_id", route.1);
                assert_eq!(row["argument_id"], json!(route.0));
                assert_eq!(row["suspended_by_decision_id"], Value::Null);
                assert!(!row["grounds"].as_array().unwrap().is_empty());
            }
            let identity_claim = Uuid::new_v4();
            let passage_claim = Uuid::new_v4();
            let mut links = vec![
                json!({"claim_id":identity_claim,"argument_id":self.alternate.0,"grounding_set_id":self.alternate.1}),
                json!({"claim_id":passage_claim,"argument_id":self.truck.0,"grounding_set_id":self.truck.1}),
            ];
            if initial || invalid {
                links.push(json!({"claim_id":identity_claim,"argument_id":self.hat.0,"grounding_set_id":self.hat.1}));
            }
            let answer = if initial {
                "The hat and Jules's account appear on separate support routes."
            } else {
                "Jules's account remains as support. The hat route is suspended."
            };
            Ok(ModelResponse {
                value: json!({"purpose":"SYNTHESIZE","status":"ok","niccolo_artifact":{
                    "answer_text":answer,"coverage_note":"Deterministic Alder Bend replay.",
                    "claims":[
                        {"claim_id":identity_claim,"ordinal":1,"kind":"INFERENCE","text":answer},
                        {"claim_id":passage_claim,"ordinal":2,"kind":"OBSERVATION","text":"Mara also reported a truck passing."}
                    ],
                    "workbench_support_links":links
                }}),
                prompt_tokens: Some(8),
                completion_tokens: Some(5),
                effective_provider: None,
                effective_model: None,
                fallback_used: false,
            })
        })
    }
}

const ALDER_SUPPORT_GOAL: &str = "What supports the hypothesis that Frank drove?";

async fn alder_analysis(
    case: &Setup,
    base: &str,
    analysis: Uuid,
    prior: Option<Uuid>,
) -> (StatusCode, Uuid, Value) {
    let operation = Uuid::new_v4();
    let uri = format!("{base}/bff/intents/{operation}/sync");
    let (status, reply) = invoke(case.router.clone(), request("POST", &uri, &case.token, Some(json!({
        "analysis_id":analysis, "intent":"UNDERSTAND", "goal":ALDER_SUPPORT_GOAL,
        "reanalysis_of":prior, "step_budget":32, "context":{}
    })))).await;
    (status, operation, reply)
}

#[tokio::test]
async fn alder_bend_audits_suspension_and_restoration_of_one_support_route() {
    let mut case = setup_with_focus("Which support should we use after reviewing the hat?").await;
    if let Ok(url) = std::env::var("NICCOLO_PG_SECURE_URL") {
        case.router = api::router(PgPoolOptions::new().connect(&url).await.unwrap());
    }
    let frank = Uuid::new_v4();
    let mara = Uuid::new_v4();
    for (actor, label) in [(frank, "Frank Vale"), (mara, "Mara Ellis")] {
        let uri = format!("/v1/workspaces/{}/actors/{actor}", case.workspace);
        let (status, body) = invoke(
            case.router.clone(),
            request(
                "POST",
                &uri,
                &case.token,
                Some(json!({"display_label":label})),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let hat = case
        .case_node(
            "PROPOSITION",
            "Mara reports a red hat in the cab.",
            "A detail under review.",
        )
        .await;
    let truck = case
        .case_node(
            "PROPOSITION",
            "Mara reports seeing a truck.",
            "A separate detail.",
        )
        .await;
    let interview = case
        .case_source(
            hat,
            "Mara testimony",
            "Mara Ellis",
            "I saw the truck. I remember a red hat in the cab.",
        )
        .await;
    case.case_request(
        &format!("epistemic/objects/{truck}/artifact-provenance/{interview}"),
        Some(json!({"rationale":"Separate observation from the same interview."})),
    )
    .await;
    let alternate = case
        .case_node(
            "PROPOSITION",
            "Jules reports seeing Frank by the driver's door.",
            "Independently collected observation.",
        )
        .await;
    case.case_source(
        alternate,
        "Jules testimony",
        "Jules Navarro",
        "Frank was standing by the driver's door.",
    )
    .await;
    let hypothesis = case
        .case_node(
            "HYPOTHESIS",
            "Frank may have been driving.",
            "Investigator-authored hypothesis.",
        )
        .await;
    let passage = case
        .case_node(
            "FINDING",
            "A truck passed the college.",
            "Separate physical observation.",
        )
        .await;
    let identity_argument = case
        .case_argument(hypothesis, "Compare distinct support routes.")
        .await;
    let passage_argument = case
        .case_argument(passage, "Assess the truck observation.")
        .await;
    let hat_route = case
        .case_ground(
            identity_argument,
            &[hat],
            "The red hat is the only ground on this route.",
        )
        .await;
    let alternate_route = case
        .case_ground(identity_argument, &[alternate], "A different observation.")
        .await;
    let passage_route = case
        .case_ground(passage_argument, &[truck], "Uses the truck observation.")
        .await;

    let photo = case
        .case_node(
            "PROPOSITION",
            "Frank wears a red hat in the archive photo.",
            "Attributed description of the selected source.",
        )
        .await;
    let picture = case
        .case_representation(
            photo,
            "IMAGE",
            "Newspaper archive photo",
            "Illustrating image",
            "HUMAN_DESCRIPTION",
            "The archive photo shows Frank wearing a red hat.",
        )
        .await;
    let exposure_claim = case
        .case_node(
            "PROPOSITION",
            "Mara says she saw the archive photo on November 19.",
            "Before the interview.",
        )
        .await;
    let exposure_basis = case
        .case_source(
            exposure_claim,
            "Mara follow-up",
            "Mara Ellis",
            "I saw that photo on November 19.",
        )
        .await;
    let exposure = Uuid::new_v4();
    case.case_request(&format!("epistemic/public-prominence/{exposure}"), Some(json!({
        "subject_actor_id":frank, "audience_actor_id":mara, "cue":"FACE",
        "observation":"EXPOSURE_REPORTED", "audience_scope":"Mara only",
        "effective_from":"2026-11-19", "effective_through":"2026-11-19", "available_on":"2026-11-23",
        "claim_object_id":exposure_claim, "representation_id":exposure_basis
    }))).await;
    let review = json!({
        "exposure_record_id":exposure, "witness_actor_id":mara,
        "testimony_detail":{"object_id":hat, "representation_id":interview},
        "candidate_source":{"object_id":photo, "representation_id":picture},
        "testimony_from":"2026-11-20", "testimony_through":"2026-11-20", "known_through":"2026-11-23"
    });
    let before = case.case_request("epistemic/workbench", None).await;
    let route_use = "epistemic/detail-exposure-review/route-use";
    let base = format!(
        "/v1/workspaces/{}/situations/{}",
        case.workspace, case.situation
    );
    let preview_uri = format!("{base}/epistemic/detail-exposure-review");
    let (status, preview) = invoke(
        case.router.clone(),
        request("POST", &preview_uri, &case.token, Some(review.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["disposition"], "REVIEW_CANDIDATE");
    assert_eq!(preview["affected_routes"].as_array().unwrap().len(), 1);
    let keep_id = Uuid::new_v4();
    let payload = json!({
        "argument_id":identity_argument, "grounding_set_id":hat_route, "disposition":"USE",
        "rationale":"Keep the candidate route during the preliminary review.", "review":review
    });
    let kept = case
        .case_request(&format!("{route_use}/{keep_id}"), Some(payload.clone()))
        .await;
    assert_eq!(kept["decision"]["basis"]["result"], preview);

    let first_analysis = Uuid::new_v4();
    let second_analysis = Uuid::new_v4();
    let invalid_analysis = Uuid::new_v4();
    let suspend_id = Uuid::new_v4();
    let analysis_pool = match std::env::var("NICCOLO_PG_SECURE_URL") {
        Ok(url) => PgPoolOptions::new().connect(&url).await.unwrap(),
        Err(_) => case.service.clone(),
    };
    case.router = api::router_with_model(
        analysis_pool,
        std::sync::Arc::new(AlderSupportModel {
            before: first_analysis,
            invalid: invalid_analysis,
            hat: (identity_argument, hat_route),
            alternate: (identity_argument, alternate_route),
            truck: (passage_argument, passage_route),
            suspended_by: suspend_id,
        }),
    );
    let (status, _, initial) = alder_analysis(&case, &base, first_analysis, None).await;
    assert_eq!(status, StatusCode::OK, "{initial}");
    assert_eq!(initial["operation"]["state"], "SUCCEEDED", "{initial}");
    let first_output = format!("analysis-snapshots/{first_analysis}/output");
    let first_basis = format!("analysis-snapshots/{first_analysis}/bff/reasoning-input");
    let original_output = case.case_request(&first_output, None).await;
    let original_support = case.case_request(&first_basis, None).await["workbench_support"].clone();
    assert_eq!(
        original_output["workbench_support_links"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        case_row(&original_support, "routes", "grounding_set_id", hat_route)["suspended_by_decision_id"],
        Value::Null
    );

    let mut suspend = payload.clone();
    suspend["disposition"] = json!("SUSPEND");
    suspend["rationale"] = json!("Suspend use of the hat until the origin is clarified.");
    let suspended = case
        .case_request(&format!("{route_use}/{suspend_id}"), Some(suspend.clone()))
        .await;
    let suspended = &suspended["decision"];
    assert_eq!(suspended["basis"]["input"], payload["review"]);
    assert_eq!(suspended["basis"]["result"], preview);
    assert_eq!(suspended["recorded_by"], kept["decision"]["recorded_by"]);
    assert!(!suspended["recorded_at"].as_str().unwrap().is_empty());
    let after = case.case_request("epistemic/workbench", None).await;
    let prior_hypothesis = case_row(
        &before,
        "reasoning_diagnostics",
        "conclusion_object_id",
        hypothesis,
    );
    let current_hypothesis = case_row(
        &after,
        "reasoning_diagnostics",
        "conclusion_object_id",
        hypothesis,
    );
    assert_eq!(
        prior_hypothesis["non_suspended_supporting_routes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        current_hypothesis["non_suspended_supporting_routes"],
        json!([
            {"argument_id":identity_argument, "grounding_set_id":alternate_route}
        ])
    );
    let suspended_route = case_row(current_hypothesis, "routes", "grounding_set_id", hat_route);
    assert_eq!(
        suspended_route["suspended_by_decision_id"],
        json!(suspend_id)
    );
    let current_passage = case_row(
        &after,
        "reasoning_diagnostics",
        "conclusion_object_id",
        passage,
    );
    assert_eq!(
        current_passage["non_suspended_supporting_routes"],
        json!([
            {"argument_id":passage_argument, "grounding_set_id":passage_route}
        ])
    );
    assert_eq!(after["nodes"], before["nodes"]);
    assert_eq!(after["grounding_sets"], before["grounding_sets"]);
    assert_eq!(after["artifact_provenance"], before["artifact_provenance"]);
    assert_eq!(
        after["route_use_history"],
        json!([kept["decision"], suspended])
    );

    let freeze_uri = format!("{base}/bff/operations/{}/sync", Uuid::new_v4());
    let (status, freeze) = invoke(
        case.router.clone(),
        request(
            "POST",
            &freeze_uri,
            &case.token,
            Some(json!({
                "kind":"REANALYZE", "target_analysis_id":first_analysis,
                "input":{"new_analysis_id":second_analysis, "goal":ALDER_SUPPORT_GOAL},
                "step_budget":32
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{freeze}");
    assert_eq!(freeze["state"], "SUCCEEDED", "{freeze}");
    assert_eq!(freeze["result"]["new_analysis_id"], json!(second_analysis));
    let (status, _, revised) =
        alder_analysis(&case, &base, second_analysis, Some(first_analysis)).await;
    assert_eq!(status, StatusCode::OK, "{revised}");
    assert_eq!(revised["operation"]["state"], "SUCCEEDED", "{revised}");
    assert_eq!(revised["analysis_replayed"], true);
    let second_output = format!("analysis-snapshots/{second_analysis}/output");
    let second_basis = format!("analysis-snapshots/{second_analysis}/bff/reasoning-input");
    let revised_output = case.case_request(&second_output, None).await;
    let links = revised_output["workbench_support_links"]
        .as_array()
        .unwrap();
    assert_eq!(links.len(), 2);
    for route in [alternate_route, passage_route] {
        assert!(
            links
                .iter()
                .any(|link| link["grounding_set_id"] == json!(route))
        );
    }
    let revised_support = case.case_request(&second_basis, None).await["workbench_support"].clone();
    assert_eq!(
        case_row(&revised_support, "routes", "grounding_set_id", hat_route)["suspended_by_decision_id"],
        json!(suspend_id)
    );

    let (status, invalid_operation, invalid) =
        alder_analysis(&case, &base, invalid_analysis, Some(first_analysis)).await;
    assert!(
        status.is_server_error() || invalid["operation"]["state"] == "FAILED",
        "{status}: {invalid}"
    );
    let failed = case
        .case_request(&format!("bff/operations/{invalid_operation}"), None)
        .await;
    assert_eq!(failed["state"], "FAILED");
    let plan = case
        .case_request(&format!("bff/operations/{invalid_operation}/agentic"), None)
        .await;
    let synthesis = plan["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| step["kind"] == "SYNTHESIZE")
        .unwrap();
    assert_eq!(synthesis["state"], "FAILED");
    let invalid_output = format!("{base}/analysis-snapshots/{invalid_analysis}/output");
    let (status, _) = invoke(
        case.router.clone(),
        request("GET", &invalid_output, &case.token, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let mut outside = suspend.clone();
    outside["grounding_set_id"] = json!(alternate_route);
    let mut changed = suspend.clone();
    changed["rationale"] = json!("A different rationale is a different decision.");
    for (id, input, expected) in [
        (Uuid::new_v4(), outside, StatusCode::BAD_REQUEST),
        (suspend_id, changed, StatusCode::CONFLICT),
        (suspend_id, suspend, StatusCode::OK),
    ] {
        let uri = format!("{base}/{route_use}/{id}");
        let (status, body) = invoke(
            case.router.clone(),
            request("POST", &uri, &case.token, Some(input)),
        )
        .await;
        assert_eq!(status, expected, "{body}");
    }

    let restore_id = Uuid::new_v4();
    let mut restore = payload;
    restore["rationale"] = json!("Restore this route after accepting its limited value.");
    case.case_request(&format!("{route_use}/{restore_id}"), Some(restore))
        .await;
    let restored = case.case_request("epistemic/workbench", None).await;
    let current = case_row(
        &restored,
        "reasoning_diagnostics",
        "conclusion_object_id",
        hypothesis,
    );
    assert_eq!(
        current["non_suspended_supporting_routes"],
        prior_hypothesis["non_suspended_supporting_routes"]
    );
    let history = restored["route_use_history"].as_array().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0], kept["decision"]);
    assert_eq!(&history[1], suspended);
    assert_eq!(history[2]["event_id"], json!(restore_id));
    assert_eq!(
        case.case_request(&first_output, None).await,
        original_output
    );
    assert_eq!(
        case.case_request(&first_basis, None).await["workbench_support"],
        original_support
    );
    assert_eq!(
        case.case_request(&second_basis, None).await["workbench_support"],
        revised_support
    );
}

#[tokio::test]
async fn alder_bend_replays_claires_correction_across_two_investigations() {
    let mut audit = setup_with_focus("Did the records establish the 20:00 quarry removal?").await;
    if let Ok(url) = std::env::var("NICCOLO_PG_SECURE_URL") {
        audit.router = api::router(PgPoolOptions::new().connect(&url).await.unwrap());
    }
    let admin = PgPoolOptions::new()
        .connect(&std::env::var("NICCOLO_PG_ADMIN_URL").unwrap())
        .await
        .unwrap();
    let owen = Uuid::new_v4();
    let police = Setup {
        router: audit.router.clone(),
        workspace: audit.workspace,
        situation: Uuid::new_v4(),
        token: authz::provision_token(&admin, owen, audit.workspace, true, true, 1)
            .await
            .unwrap(),
        service: audit.service.clone(),
    };
    let police_url = format!(
        "/v1/workspaces/{}/situations/{}",
        police.workspace, police.situation
    );
    let (status, _) = invoke(police.router.clone(), request("POST", &police_url, &police.token,
        Some(json!({"focus":"When could truck 07 have been near the missing person's last location?"})))).await;
    assert_eq!(status, StatusCode::CREATED);

    let old_police = police
        .case_node(
            "PROPOSITION",
            "Owen's initial note placed truck 07 at the pharmacy at 20:00.",
            "Owen combined Claire's closing time with the reported sighting.",
        )
        .await;
    let old_source = police
        .case_source(
            old_police,
            "Owen's initial interview note",
            "Claire, initial interview",
            "Claire closed at 20:00 and saw truck 07 behind the pharmacy.",
        )
        .await;
    let transfer_path = "artifacts/representation-transfers";
    audit
        .case_request(
            &format!("{transfer_path}/{}", Uuid::new_v4()),
            Some(json!({
                "source_situation_id":police.situation, "source_representation_id":old_source,
                "note":"Ruth received Owen's initial interview note for the audit."
            })),
        )
        .await;
    let old_audit = audit
        .case_node(
            "PROPOSITION",
            "Owen's initial note placed truck 07 at the pharmacy at 20:00.",
            "The audit uses the received note, with no new witness.",
        )
        .await;
    audit
        .case_request(
            &format!("epistemic/objects/{old_audit}/artifact-provenance/{old_source}"),
            Some(json!({"rationale":"The copy retains the provenance of Owen's note."})),
        )
        .await;

    let invoice = audit
        .case_node(
            "PROPOSITION",
            "The invoice declares truck 07 completed a quarry removal at 20:00.",
            "This records the bill's statement.",
        )
        .await;
    audit
        .case_source(
            invoice,
            "Quarry invoice",
            "Completed-removal line",
            "20:00 — removal completed — quarry — unit 07.",
        )
        .await;
    let call = audit
        .case_node(
            "PROPOSITION",
            "The first reading of the call extract places truck 07 at the pharmacy at 20:00.",
            "The meaning of the time column is unresolved at intake.",
        )
        .await;
    audit
        .case_source(
            call,
            "Pharmacy call extract",
            "Time column",
            "20:00 — pharmacy — unit 07.",
        )
        .await;
    let mismatch = audit
        .case_node(
            "HYPOTHESIS",
            "The claimed quarry removal may conflict with the 20:00 pharmacy sighting.",
            "An authored suspicion about this invoice.",
        )
        .await;
    let audit_argument = audit
        .case_argument(mismatch, "Compare the two apparent 20:00 claims.")
        .await;
    let sighting_route = audit
        .case_ground(
            audit_argument,
            &[old_audit, invoice],
            "Depends on Owen's time attribution.",
        )
        .await;
    let call_route = audit
        .case_ground(
            audit_argument,
            &[call, invoice],
            "Depends on the first reading of the time column.",
        )
        .await;
    let billing = audit
        .case_node(
            "FINDING",
            "Vale invoiced the quarry removal.",
            "The invoice establishes what was billed.",
        )
        .await;
    let billing_argument = audit
        .case_argument(billing, "Record the invoice claim separately.")
        .await;
    let billed = audit
        .case_ground(billing_argument, &[invoice], "Depends only on the invoice.")
        .await;
    let execution_gap = audit
        .case_node(
            "INFORMATION_GAP",
            "Was the billed quarry removal actually performed?",
            "The invoice is a declaration; no independent completion record is available.",
        )
        .await;

    let early_window = police
        .case_node(
            "FINDING",
            "The original lead placed truck 07 behind the pharmacy at 20:00.",
            "An attributed working timeline.",
        )
        .await;
    let early_argument = police
        .case_argument(early_window, "Track the reported sighting time.")
        .await;
    let early_route = police
        .case_ground(
            early_argument,
            &[old_police],
            "Depends on the interview note.",
        )
        .await;
    let driver = police
        .case_node(
            "HYPOTHESIS",
            "Frank could have been driving truck 07.",
            "The reported sighting has not identified the driver.",
        )
        .await;
    let audit_original = cross_case_freeze(&audit, None).await;
    let police_original = cross_case_freeze(&police, None).await;
    let police_before = police.case_request("epistemic/workbench", None).await;

    const CORRECTION: &str = "I saw truck 07 from Adam's apartment around 21:20, just after the phone call ended at 21:18. I did not see the driver.";
    let correction_artifact = Uuid::new_v4();
    let correction = Uuid::new_v4();
    audit
        .case_request(
            &format!("artifacts/{correction_artifact}"),
            Some(json!({
                "artifact_type":"TRANSCRIPT_FILE","label":"Claire Mercer's corrected declaration"
            })),
        )
        .await;
    audit.case_request(&format!("artifacts/{correction_artifact}/representations/{correction}"), Some(json!({
        "representation_type":"HUMAN_TRANSCRIPTION", "producer_type":"THIRD_PARTY_HUMAN",
        "producer_label":"Claire Mercer", "scope_locator":"Corrected declaration", "content":CORRECTION,
        "completeness_status":"PARTIAL", "epistemic_qualification":"Attributed sighting; time estimated"
    }))).await;
    let audit_corrected = audit
        .case_node(
            "PROPOSITION",
            "Claire now reports the sighting at about 21:20.",
            "The phone record anchors the estimate, and Claire reports the sighting.",
        )
        .await;
    audit
        .case_request(
            &format!("epistemic/objects/{audit_corrected}/artifact-provenance/{correction}"),
            Some(json!({"rationale":"This representation gives Claire's corrected timing."})),
        )
        .await;
    audit.case_request(&format!("epistemic/relations/{}", Uuid::new_v4()), Some(json!({
        "family":"GENEALOGY", "kind":"SUPERSEDES", "left_object_id":audit_corrected,
        "right_object_id":old_audit, "rationale":"Claire corrects how the earlier note located her sighting in time."
    }))).await;

    let audit_mid = audit.case_request("epistemic/workbench", None).await;
    let impacts = audit_mid["reassessment_impacts"].as_array().unwrap();
    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0]["grounding_set_id"], json!(sighting_route));
    let audit_after_claire = cross_case_freeze(&audit, Some(audit_original)).await;
    assert_eq!(
        police.case_request("epistemic/workbench", None).await,
        police_before
    );
    let police_before_receipt = cross_case_freeze(&police, Some(police_original)).await;
    assert_eq!(
        cross_case_packet(&police, police_original)
            .await
            .frozen_input
            .artifact_representations,
        cross_case_packet(&police, police_before_receipt)
            .await
            .frozen_input
            .artifact_representations
    );

    let field = audit
        .case_node(
            "PROPOSITION",
            "The time column labels when the call was opened.",
            "Ruth checked the original form definition.",
        )
        .await;
    let field_source = audit
        .case_source(
            field,
            "Dispatch form definition",
            "Time field",
            "Time = call opening time, not unit arrival or service completion.",
        )
        .await;
    audit
        .case_request(
            &format!("epistemic/relations/{}", Uuid::new_v4()),
            Some(json!({
                "family":"GENEALOGY", "kind":"SUPERSEDES", "left_object_id":field,
                "right_object_id":call, "rationale":"The original form defines the time field."
            })),
        )
        .await;
    let audit_after = audit.case_request("epistemic/workbench", None).await;
    let impacts = audit_after["reassessment_impacts"].as_array().unwrap();
    assert_eq!(impacts.len(), 2);
    for route in [sighting_route, call_route] {
        let affected = case_row(
            &audit_after,
            "reassessment_impacts",
            "grounding_set_id",
            route,
        );
        assert_eq!(affected["conclusion_object_id"], json!(mismatch));
    }
    let surviving = case_row(
        &audit_after,
        "reasoning_diagnostics",
        "conclusion_object_id",
        billing,
    );
    assert_eq!(
        surviving["non_suspended_supporting_routes"][0]["grounding_set_id"],
        json!(billed)
    );
    assert_eq!(
        case_row(&audit_after, "nodes", "object_id", execution_gap)["kind"],
        "INFORMATION_GAP"
    );
    let audit_later = cross_case_freeze(&audit, Some(audit_after_claire)).await;
    let audit_reps = cross_case_packet(&audit, audit_later)
        .await
        .frozen_input
        .artifact_representations;
    let audit_origin = audit_reps
        .iter()
        .find(|r| r.representation_id == correction)
        .unwrap();
    assert_eq!(audit_origin.content, CORRECTION);
    assert_eq!(
        audit_origin.producer_label.as_deref(),
        Some("Claire Mercer")
    );
    assert!(audit_origin.transfer_origin.is_none());
    let audit_intermediate = cross_case_packet(&audit, audit_after_claire)
        .await
        .frozen_input
        .artifact_representations;
    assert!(
        audit_intermediate
            .iter()
            .any(|r| r.representation_id == correction)
    );
    assert!(
        audit_intermediate
            .iter()
            .all(|r| r.representation_id != field_source)
    );
    assert!(
        cross_case_packet(&audit, audit_original)
            .await
            .frozen_input
            .artifact_representations
            .iter()
            .all(|r| r.representation_id != correction && r.representation_id != field_source)
    );

    let handoff = Uuid::new_v4();
    let handoff_body = json!({
        "source_situation_id":audit.situation, "source_representation_id":correction,
        "note":"Ruth forwarded Claire's correction. Owen recorded receipt in the disappearance case."
    });
    let reader = authz::provision_token(&admin, Uuid::new_v4(), audit.workspace, true, false, 1)
        .await
        .unwrap();
    let uri = format!("{police_url}/{transfer_path}/{handoff}");
    let (status, _) = invoke(
        police.router.clone(),
        request("POST", &uri, &reader, Some(handoff_body.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let handoff_result = police
        .case_request(
            &format!("{transfer_path}/{handoff}"),
            Some(handoff_body.clone()),
        )
        .await;
    let recorded = police
        .case_request(&format!("{transfer_path}/{handoff}"), None)
        .await;
    assert_eq!(recorded, handoff_result["transfer"]);
    assert_eq!(recorded["recorded_by"], json!(owen));
    assert_eq!(recorded["source_situation_id"], json!(audit.situation));
    assert_eq!(recorded["representation_id"], json!(correction));
    assert_eq!(recorded["artifact_id"], json!(correction_artifact));
    let (status, repeat) = invoke(
        police.router.clone(),
        request("POST", &uri, &police.token, Some(handoff_body.clone())),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{repeat}");
    assert_eq!(repeat["replayed"], true);
    let duplicate_uri = format!("{police_url}/{transfer_path}/{}", Uuid::new_v4());
    let (status, _) = invoke(
        police.router.clone(),
        request("POST", &duplicate_uri, &police.token, Some(handoff_body)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let police_revised = police
        .case_node(
            "PROPOSITION",
            "Claire's corrected sighting behind the pharmacy is around 21:20.",
            "Received explicitly from the audit.",
        )
        .await;
    police
        .case_request(
            &format!("epistemic/objects/{police_revised}/artifact-provenance/{correction}"),
            Some(json!({"rationale":"Received representation of Claire's own correction."})),
        )
        .await;
    police.case_request(&format!("epistemic/relations/{}", Uuid::new_v4()), Some(json!({
        "family":"GENEALOGY", "kind":"SUPERSEDES", "left_object_id":police_revised,
        "right_object_id":old_police, "rationale":"Revise the sighting window after recorded receipt."
    }))).await;
    let later_window = police
        .case_node(
            "FINDING",
            "The corrected lead places truck 07 behind the pharmacy around 21:20.",
            "The revised timing is approximate and attributed to Claire.",
        )
        .await;
    let later_argument = police
        .case_argument(later_window, "Record the revised sighting window.")
        .await;
    let later_route = police
        .case_ground(
            later_argument,
            &[police_revised],
            "Depends on the received correction.",
        )
        .await;
    let police_after = police.case_request("epistemic/workbench", None).await;
    let impacts = police_after["reassessment_impacts"].as_array().unwrap();
    assert_eq!(impacts.len(), 1);
    assert_eq!(impacts[0]["grounding_set_id"], json!(early_route));
    assert_eq!(impacts[0]["conclusion_object_id"], json!(early_window));
    let updated = case_row(
        &police_after,
        "reasoning_diagnostics",
        "conclusion_object_id",
        later_window,
    );
    assert_eq!(
        updated["non_suspended_supporting_routes"][0]["grounding_set_id"],
        json!(later_route)
    );
    assert!(
        police_after["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|argument| argument["conclusion_object_id"] != json!(driver))
    );
    let police_later = cross_case_freeze(&police, Some(police_before_receipt)).await;
    let police_reps = cross_case_packet(&police, police_later)
        .await
        .frozen_input
        .artifact_representations;
    let received = police_reps
        .iter()
        .find(|r| r.representation_id == correction)
        .unwrap();
    assert_eq!(received.artifact_id, audit_origin.artifact_id);
    assert_eq!(received.content, audit_origin.content);
    assert_eq!(received.producer_label, audit_origin.producer_label);
    let origin = received.transfer_origin.as_ref().unwrap();
    assert_eq!(origin.source_situation_id, audit.situation);
    assert_eq!(origin.transfer_id, handoff);
    assert!(
        police_reps
            .iter()
            .all(|r| r.representation_id != field_source)
    );
    assert!(
        cross_case_packet(&police, police_before_receipt)
            .await
            .frozen_input
            .artifact_representations
            .iter()
            .all(|r| r.representation_id != correction)
    );
}

#[tokio::test]
async fn alder_bend_traces_apparent_corroboration_to_its_sources() {
    let case = setup_with_focus("What supports the presence of truck 07 around 21:20?").await;
    let presence = case
        .case_node(
            "FINDING",
            "Truck 07 was behind the pharmacy around 21:20.",
            "Assess the reported time and location separately from the driver.",
        )
        .await;
    let presence_argument = case
        .case_argument(presence, "Trace each basis for the sighting.")
        .await;
    let claire = case
        .case_node(
            "PROPOSITION",
            "Claire reports seeing truck 07 around 21:20.",
            "Claire describes her own observation from a window.",
        )
        .await;
    let claire_route = case
        .case_ground(presence_argument, &[claire], "Claire's direct account.")
        .await;
    let claire_source = case
        .case_source(
            claire,
            "Claire's corrected interview",
            "Claire Mercer",
            "I saw truck 07 behind the pharmacy around 21:20. I could not see the driver.",
        )
        .await;

    let note_entry = case
        .case_node(
            "FINDING",
            "Owen entered the 21:20 lead in the police file.",
            "A fact about police recordkeeping.",
        )
        .await;
    let note_argument = case
        .case_argument(note_entry, "Establish what the police file records.")
        .await;
    let owen = case
        .case_node(
            "PROPOSITION",
            "Owen's note puts truck 07 behind the pharmacy around 21:20.",
            "The note records a lead, with the underlying source to be checked.",
        )
        .await;
    let note_route = case
        .case_ground(note_argument, &[owen], "Owen's note records the entry.")
        .await;
    let first = cross_case_freeze(&case, None).await;

    let owen_source = case
        .case_source(
            owen,
            "Owen's case note",
            "Owen Pike's entry",
            "Truck 07 behind the pharmacy around 21:20, per Claire Mercer's corrected interview.",
        )
        .await;
    let derived_route = case
        .case_ground(
            presence_argument,
            &[owen, claire],
            "Owen's note relays Claire's observation and depends on it for the sighting.",
        )
        .await;
    let camera = case
        .case_node(
            "PROPOSITION",
            "The pharmacy camera description places unit 07 there around 21:20.",
            "The operator describes a separately recorded image sequence; the cab is out of view.",
        )
        .await;
    let camera_source = case.case_representation(camera, "VIDEO", "Pharmacy camera recording",
        "21:19–21:21", "HUMAN_DESCRIPTION",
        "Truck number 07 is visible behind the pharmacy around 21:20. The camera does not show the cab.").await;
    let camera_route = case.case_ground(presence_argument, &[camera],
        "The separately collected camera description provides another basis for the vehicle's presence.").await;
    let unsigned = case
        .case_node(
            "PROPOSITION",
            "An unsigned note also reports unit 07 behind the pharmacy.",
            "Its author and whether it repeats another source are unknown.",
        )
        .await;
    let unsigned_source = case
        .case_source(
            unsigned,
            "Unsigned note",
            "Unsigned entry; author and original source unknown",
            "Unit 07 behind the pharmacy, about 21:20.",
        )
        .await;
    let gap = case
        .case_node(
            "INFORMATION_GAP",
            "Who produced the unsigned note and on what basis?",
            "Trace its origin before treating it as an independent account.",
        )
        .await;
    let gap_question = case
        .case_node(
            "QUESTION",
            "Can we trace the source of the unsigned note?",
            "The note may be a copy or an observation.",
        )
        .await;
    case.case_request(
        &format!("epistemic/relations/{}", Uuid::new_v4()),
        Some(json!({
            "family":"CONTEXTUAL", "kind":"ADDRESSES_GAP", "left_object_id":gap_question,
            "right_object_id":gap, "rationale":"Identify the author and the basis of the entry."
        })),
    )
    .await;
    let driver = case
        .case_node(
            "HYPOTHESIS",
            "Frank Vale was driving truck 07.",
            "The material about the vehicle does not identify its driver.",
        )
        .await;

    let workbench = case.case_request("epistemic/workbench", None).await;
    let owen_artifact = &case_row(
        &workbench,
        "artifact_provenance",
        "representation_id",
        owen_source,
    )["artifact_id"];
    let claire_artifact = &case_row(
        &workbench,
        "artifact_provenance",
        "representation_id",
        claire_source,
    )["artifact_id"];
    let derivation = Uuid::new_v4();
    case.case_request(&format!("artifact-relationships/{derivation}"), Some(json!({
        "subject_artifact_id":owen_artifact, "object_artifact_id":claire_artifact,
        "relationship_type":"DERIVED_FROM", "qualification":"Owen's note explicitly cites Claire's interview."
    }))).await;

    let presence_diagnostic = case_row(
        &workbench,
        "reasoning_diagnostics",
        "conclusion_object_id",
        presence,
    );
    assert_eq!(
        presence_diagnostic["non_suspended_supporting_routes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let shared = &presence_diagnostic["shared_grounds"];
    let claire_shared = case_row(
        presence_diagnostic,
        "shared_grounds",
        "ground_object_id",
        claire,
    );
    let shared_routes = claire_shared["routes"].as_array().unwrap();
    assert_eq!(shared.as_array().unwrap().len(), 1);
    assert_eq!(shared_routes.len(), 2);
    for route in [claire_route, derived_route] {
        assert!(
            shared_routes
                .iter()
                .any(|entry| entry["grounding_set_id"] == json!(route))
        );
    }
    assert_eq!(
        presence_diagnostic["direct_evidence_lineage"]["establishes_independence"],
        false
    );
    assert_eq!(
        case_row(
            &workbench,
            "artifact_provenance",
            "representation_id",
            unsigned_source
        )["object_id"],
        json!(unsigned)
    );
    assert_eq!(
        case_row(&workbench, "nodes", "object_id", gap)["kind"],
        "INFORMATION_GAP"
    );
    assert!(
        workbench["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|argument| argument["conclusion_object_id"] != json!(driver))
    );

    let revised = cross_case_freeze(&case, Some(first)).await;
    let original = cross_case_packet(&case, first).await.frozen_input;
    let current = cross_case_packet(&case, revised).await.frozen_input;
    assert!(original.artifact_relationships.is_empty());
    assert_eq!(current.artifact_relationships.len(), 1);
    let relationship = &current.artifact_relationships[0];
    assert_eq!(relationship.relationship_id, derivation);
    assert_eq!(relationship.relationship_type, "DERIVED_FROM");
    assert_eq!(json!(relationship.subject_artifact_id), *owen_artifact);
    assert_eq!(json!(relationship.object_artifact_id), *claire_artifact);
    assert!(!relationship.relationship_is_verified);
    let camera_representation = current
        .artifact_representations
        .iter()
        .find(|representation| representation.representation_id == camera_source)
        .unwrap();
    assert_eq!(
        camera_representation.representation_type,
        "HUMAN_DESCRIPTION"
    );
    assert!(!camera_representation.artifact_was_parsed);
    assert!(!camera_representation.representation_establishes_truth);
    assert!(
        current
            .artifact_representations
            .iter()
            .any(|representation| representation.representation_id == unsigned_source)
    );

    let initial_support = json!(original.workbench_support.unwrap());
    let support = json!(current.workbench_support.unwrap());
    assert_eq!(initial_support["routes"].as_array().unwrap().len(), 2);
    assert_eq!(support["routes"].as_array().unwrap().len(), 4);
    for (route, conclusion, expected_grounds) in [
        (claire_route, presence, vec![(claire, claire_source)]),
        (
            derived_route,
            presence,
            vec![(owen, owen_source), (claire, claire_source)],
        ),
        (camera_route, presence, vec![(camera, camera_source)]),
        (note_route, note_entry, vec![(owen, owen_source)]),
    ] {
        let row = case_row(&support, "routes", "grounding_set_id", route);
        assert_eq!(row["conclusion_object_id"], json!(conclusion));
        assert_eq!(
            row["grounds"].as_array().unwrap().len(),
            expected_grounds.len()
        );
        for (object, source) in expected_grounds {
            let ground = case_row(row, "grounds", "object_id", object);
            assert_eq!(ground["artifact_representation_ids"], json!([source]));
            assert_eq!(ground["artifact_provenance_truncated"], false);
        }
    }
    let old_note_route = case_row(&initial_support, "routes", "grounding_set_id", note_route);
    assert_eq!(
        case_row(old_note_route, "grounds", "object_id", owen)["artifact_representation_ids"],
        json!([])
    );
    assert_eq!(
        case_row(&initial_support, "routes", "grounding_set_id", claire_route),
        case_row(&support, "routes", "grounding_set_id", claire_route)
    );
}

#[tokio::test]
async fn alder_bend_flags_late_source_dependence_for_the_affected_route() {
    let mut case = setup_with_focus("Review the basis for the 21:20 pharmacy sighting.").await;
    if let Ok(url) = std::env::var("NICCOLO_PG_SECURE_URL") {
        case.service = PgPoolOptions::new().connect(&url).await.unwrap();
        case.router = api::router(case.service.clone());
    }
    let sighting = case
        .case_node(
            "FINDING",
            "Truck 07 was behind the pharmacy around 21:20.",
            "Assess the evidence for the reported sighting.",
        )
        .await;
    let claire = case
        .case_node(
            "PROPOSITION",
            "Claire says she saw truck 07 around 21:20.",
            "Claire describes an observation from a window.",
        )
        .await;
    let claire_rep = case
        .case_source(
            claire,
            "Claire's statement",
            "Claire Mercer",
            "I saw truck 07 behind the pharmacy around 21:20.",
        )
        .await;
    let owen = case
        .case_node(
            "PROPOSITION",
            "Owen's case note records the same time and place.",
            "The basis of the entry is still being checked.",
        )
        .await;
    let owen_rep = case
        .case_source(
            owen,
            "Owen's case note",
            "Initial entry",
            "Truck 07 at rear of pharmacy, approximately 21:20.",
        )
        .await;
    let camera = case
        .case_node(
            "PROPOSITION",
            "A separately collected camera description places unit 07 there.",
            "The investigator reports the vehicle visible near 21:20.",
        )
        .await;
    case.case_representation(
        camera,
        "VIDEO",
        "Pharmacy camera",
        "Rear lot",
        "HUMAN_DESCRIPTION",
        "Unit 07 is in view behind the pharmacy around 21:20.",
    )
    .await;
    let witness_argument = case
        .case_argument(sighting, "Claire's direct account.")
        .await;
    let note_argument = case
        .case_argument(sighting, "Provisional reliance on the police note.")
        .await;
    let claire_route = case
        .case_ground(witness_argument, &[claire], "Claire's direct account.")
        .await;
    let owen_route = case
        .case_ground(
            note_argument,
            &[owen],
            "A separate lead with origin pending.",
        )
        .await;
    let camera_route = case
        .case_ground(
            witness_argument,
            &[camera],
            "The camera records the lot separately.",
        )
        .await;
    let combined_route = case
        .case_ground(
            witness_argument,
            &[owen, claire],
            "Compare the police entry and its possible source together.",
        )
        .await;

    let filing = case
        .case_node(
            "FINDING",
            "Owen recorded the lead in the police file.",
            "The police note supports this documentary claim directly.",
        )
        .await;
    let filing_argument = case
        .case_argument(filing, "Establish what the police file records.")
        .await;
    let filing_route = case
        .case_ground(filing_argument, &[owen], "The police entry itself.")
        .await;

    let path = "epistemic/workbench";
    let before = case.case_request(path, None).await;
    let owen_link = case_row(
        &before,
        "artifact_provenance",
        "representation_id",
        owen_rep,
    );
    let claire_link = case_row(
        &before,
        "artifact_provenance",
        "representation_id",
        claire_rep,
    );
    let earlier = cross_case_freeze(&case, None).await;
    let initial_basis = cross_case_packet(&case, earlier).await.frozen_input;
    let before_sighting = case_row(
        &before,
        "reasoning_diagnostics",
        "conclusion_object_id",
        sighting,
    );
    assert_eq!(before_sighting["artifact_dependence_reviews"], json!([]));
    assert!(initial_basis.artifact_relationships.is_empty());

    let relationship = Uuid::new_v4();
    const QUALIFICATION: &str = "Owen confirmed that his entry summarizes Claire's statement.";
    case.case_request(&format!("artifact-relationships/{relationship}"), Some(json!({
        "subject_artifact_id":owen_link["artifact_id"], "object_artifact_id":claire_link["artifact_id"],
        "relationship_type":"CLAIMED_TO_BE_DERIVED_FROM", "qualification":QUALIFICATION
    }))).await;

    let after = case.case_request(path, None).await;
    let after_sighting = case_row(
        &after,
        "reasoning_diagnostics",
        "conclusion_object_id",
        sighting,
    );
    let review = case_row(
        after_sighting,
        "artifact_dependence_reviews",
        "relationship_id",
        relationship,
    );
    assert_eq!(
        after_sighting["artifact_dependence_reviews"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(review["derived_artifact_id"], owen_link["artifact_id"]);
    assert_eq!(review["source_artifact_id"], claire_link["artifact_id"]);
    assert_eq!(review["qualification"], QUALIFICATION);
    assert_eq!(review["recorded_by"], owen_link["recorded_by"]);
    assert_eq!(
        review["routes_to_review"],
        json!([
            {"argument_id":note_argument, "grounding_set_id":owen_route}
        ])
    );
    let source_routes = review["source_routes"].as_array().unwrap();
    assert_eq!(source_routes.len(), 2);
    for route in [claire_route, combined_route] {
        assert!(
            source_routes
                .contains(&json!({"argument_id":witness_argument, "grounding_set_id":route}))
        );
    }
    let active = &after_sighting["non_suspended_supporting_routes"];
    assert_eq!(*active, before_sighting["non_suspended_supporting_routes"]);
    assert!(
        active
            .as_array()
            .unwrap()
            .contains(&json!({"argument_id":witness_argument, "grounding_set_id":camera_route}))
    );
    let after_filing = case_row(
        &after,
        "reasoning_diagnostics",
        "conclusion_object_id",
        filing,
    );
    assert_eq!(after_filing["artifact_dependence_reviews"], json!([]));
    assert_eq!(
        after_filing["non_suspended_supporting_routes"],
        json!([
            {"argument_id":filing_argument, "grounding_set_id":filing_route}
        ])
    );
    assert_eq!(after["route_use_history"], before["route_use_history"]);

    let revised = cross_case_freeze(&case, Some(earlier)).await;
    let revised_basis = cross_case_packet(&case, revised).await.frozen_input;
    let replayed_basis = cross_case_packet(&case, earlier).await.frozen_input;
    assert_eq!(
        replayed_basis.artifact_relationships,
        initial_basis.artifact_relationships
    );
    assert_eq!(
        replayed_basis.workbench_support,
        initial_basis.workbench_support
    );
    assert_eq!(
        revised_basis.workbench_support,
        initial_basis.workbench_support
    );
    assert_eq!(revised_basis.artifact_relationships.len(), 1);
    let frozen_relationship = &revised_basis.artifact_relationships[0];
    assert_eq!(frozen_relationship.relationship_id, relationship);
    assert!(!frozen_relationship.relationship_is_verified);
}

async fn cross_case_freeze(case: &Setup, parent: Option<Uuid>) -> Uuid {
    let analysis = Uuid::new_v4();
    case.case_request(
        &format!("analysis-snapshots/{analysis}"),
        Some(json!({
            "goal":"Freeze the sources known to this investigation at this stage.",
            "reanalysis_of":parent
        })),
    )
    .await;
    analysis
}

async fn cross_case_packet(case: &Setup, analysis: Uuid) -> reasoning_packet::ReasoningPacket {
    let digest = authz::credential_digest(&case.token).unwrap();
    authz::with_auth_digest(
        digest,
        reasoning_packet::build(
            &case.service,
            AnalysisScope {
                workspace: case.workspace,
                situation: case.situation,
                analysis,
            },
        ),
    )
    .await
    .unwrap()
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
