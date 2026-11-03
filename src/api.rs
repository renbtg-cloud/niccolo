//! Bound, explicit HTTP boundary. Not a public Internet deployment template.
//! AuthN/AuthZ decision precedes EVERY workspace read and write. The client
//! may select a workspace but can never authorize itself by merely naming it.
use crate::authz::{self, AuthError, Capability};
use crate::discovery;
use crate::human_feedback::{self, FeedbackError, HumanResponse};
use crate::hypotheses::{self, HypothesisError};
use crate::identity::{self, IdentityAction, IdentityError};
use crate::postgres_ingest::{self, ImportError};
use crate::postgres_repository::{self, RepositoryError};
use crate::question_refs::{self, ReferenceError};
use crate::source_catalog;
use crate::source_import::SourceFormat;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

/// Reject ambiguous authorization headers before either middleware or handler
/// can use one of several possible values. Edge proxies may otherwise choose
/// different credentials from the backend.
fn unique_bearer(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let first = values.next()?;
    if values.next().is_some() {
        return None;
    }
    first.to_str().ok()?.strip_prefix("Bearer ")
}

/// Bind a validated token digest only for the lifetime of the downstream HTTP
/// future. All SQLx workspace transactions apply the digest with SET LOCAL.
async fn request_digest_context(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let raw = unique_bearer(req.headers());
    if let Some(digest) = raw.and_then(|token| authz::credential_digest(token).ok()) {
        return authz::with_auth_digest(digest, next.run(req)).await;
    }
    next.run(req).await
}

/// All responses are privacy-sensitive by default, including framework-level
/// rejections (404/405/413/415), before the request reaches an application handler.
async fn response_security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    response
}

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/readyz", get(readiness))
        .route(
            "/v1/workspaces/{workspace}/identities/{person}",
            post(create_identity),
        )
        .route(
            "/v1/workspaces/{workspace}/identity-events/{event}",
            post(record_identity_event),
        )
        .route(
            "/v1/workspaces/{workspace}/documents/{document}/identities",
            get(effective_identities),
        )
        .route(
            "/v1/workspaces/{workspace}/documents/{document}/identity-history",
            get(identity_history),
        )
        .route(
            "/v1/workspaces/{workspace}/hypotheses",
            get(list_hypotheses),
        )
        .route(
            "/v1/workspaces/{workspace}/hypotheses/{hypothesis}",
            post(record_hypothesis),
        )
        .route("/v1/workspaces/{workspace}/questions", get(list_questions))
        .route(
            "/v1/workspaces/{workspace}/questions/{question}",
            post(create_question),
        )
        .route(
            "/v1/workspaces/{workspace}/questions/{question}/feedback/{event}",
            post(record_feedback),
        )
        .route(
            "/v1/workspaces/{workspace}/questions/{question}/feedback",
            get(feedback_history),
        )
        .route(
            "/v1/workspaces/{workspace}/questions/{question}/positions",
            get(question_positions),
        )
        .route(
            "/v1/workspaces/{workspace}/questions/{question}/references",
            get(list_question_refs),
        )
        .route(
            "/v1/workspaces/{workspace}/questions/{question}/references/{reference}",
            post(attach_question_ref),
        )
        .route("/v1/workspaces/{workspace}/graph", get(graph))
        .route("/v1/workspaces/{workspace}/assess", post(assess))
        .route("/v1/workspaces/{workspace}/leads", post(propose_leads))
        .route("/v1/workspaces/{workspace}/sources", get(list_sources))
        .route(
            "/v1/workspaces/{workspace}/sources/{document}",
            post(ingest),
        )
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(AppState { pool })
        .layer(axum::middleware::from_fn(request_digest_context))
        .layer(axum::middleware::from_fn(response_security_headers))
}

#[derive(Debug)]
struct ApiError(StatusCode, &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut resp = (self.0, Json(json!({"error":self.1}))).into_response();
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        resp.headers_mut()
            .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
        resp
    }
}
impl From<RepositoryError> for ApiError {
    fn from(e: RepositoryError) -> Self {
        match e {
            RepositoryError::InvisibleReport(_) => Self(StatusCode::NOT_FOUND, "unknown report"),
            RepositoryError::LimitExceeded { .. } => {
                Self(StatusCode::UNPROCESSABLE_ENTITY, "graph budget exceeded")
            }
            RepositoryError::Trace(_) => {
                Self(StatusCode::UNPROCESSABLE_ENTITY, "untraceable evidence")
            }
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "graph unavailable"),
        }
    }
}
impl From<ImportError> for ApiError {
    fn from(e: ImportError) -> Self {
        match e {
            ImportError::Parse(_) | ImportError::Locator => {
                Self(StatusCode::BAD_REQUEST, "invalid source")
            }
            ImportError::DocumentConflict => Self(StatusCode::CONFLICT, "source document conflict"),
            ImportError::WorkspaceInvisible => Self(StatusCode::NOT_FOUND, "workspace unavailable"),
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "source unavailable"),
        }
    }
}
fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    unique_bearer(headers).ok_or(ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))
}

async fn guard(
    state: &AppState,
    headers: &HeaderMap,
    workspace: Uuid,
    capability: Capability,
) -> Result<Uuid, ApiError> {
    let token = bearer(headers)?;
    match authz::authorized_principal(&state.pool, token, workspace, capability).await {
        Ok(Some(principal)) => Ok(principal),
        Ok(None) => Err(ApiError(StatusCode::FORBIDDEN, "workspace access denied")),
        Err(AuthError::InvalidCredential) => {
            Err(ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))
        }
        Err(_) => Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "authorization unavailable",
        )),
    }
}

fn no_store(value: Value, code: StatusCode) -> Response {
    let mut response = (code, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}
async fn readiness(State(state): State<AppState>) -> Response {
    // A reachable DB is not necessarily a usable or securely configured DB.
    // Fail closed if this pool can bypass RLS, lacks its dedicated API-role
    // membership, or points to an unmigrated schema.
    let check: Result<(bool, bool, bool, bool, bool, bool), sqlx::Error> = sqlx::query_as(
        "SELECT NOT (r.rolsuper OR r.rolbypassrls), \
         pg_has_role(current_user,'niccolo_api','member'), \
         to_regclass('niccolo.workspaces') IS NOT NULL, \
         to_regclass('niccolo.source_documents') IS NOT NULL, \
         to_regprocedure('niccolo_auth.rls_credential_allows(uuid,text)') IS NOT NULL, \
         NOT EXISTS ( \
           SELECT 1 FROM pg_class c \
           JOIN pg_namespace n ON n.oid=c.relnamespace \
           JOIN pg_attribute a ON a.attrelid=c.oid \
           WHERE n.nspname='niccolo' AND c.relkind='r' \
             AND a.attname='workspace_id' AND NOT a.attisdropped \
             AND (NOT c.relrowsecurity OR NOT c.relforcerowsecurity OR \
              (SELECT count(*) FROM pg_policy p \
               WHERE p.polrelid=c.oid AND NOT p.polpermissive \
                 AND p.polname IN ('api_read_gate','api_write_insert_gate', \
                                   'api_write_update_gate','api_write_delete_gate') \
                 AND 'niccolo_api'::regrole::oid=ANY(p.polroles)) <> 4)) \
         FROM pg_roles r WHERE r.rolname=current_user",
    )
    .fetch_one(&state.pool)
    .await;
    if matches!(check, Ok((true, true, true, true, true, true))) {
        no_store(json!({"status":"ready"}), StatusCode::OK)
    } else {
        no_store(
            json!({"status":"unavailable"}),
            StatusCode::SERVICE_UNAVAILABLE,
        )
    }
}

async fn graph(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let snapshot = postgres_repository::load_workspace_graph(&state.pool, workspace).await?;
    Ok(no_store(
        json!({"workspace_id": workspace, "revision":snapshot.revision(),
        "node_count":snapshot.node_count(), "edge_count":snapshot.edge_count()}),
        StatusCode::OK,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssessmentRequest {
    report_ids: Vec<Uuid>,
}
async fn assess(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<AssessmentRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    if request.report_ids.is_empty() || request.report_ids.len() > 64 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "report count must be 1..64",
        ));
    }
    if request
        .report_ids
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != request.report_ids.len()
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "duplicate report identifiers",
        ));
    }
    let snapshot = postgres_repository::load_workspace_graph(&state.pool, workspace).await?;
    let report = snapshot.assess(&request.report_ids)?;
    Ok(no_store(
        json!({
            "revision":snapshot.revision(),"report_count":report.report_count,
            "report_ids":request.report_ids,
            "traceable_roots":report.traceable_roots,
            "roots_per_report":report.roots_per_report,
            "document_ids_per_report":report.document_ids_per_report,
            "shared_source_documents":report.shared_source_documents.iter().map(|x| json!({
                "left_report_index":x.left_report_index,"right_report_index":x.right_report_index,"documents":x.documents
            })).collect::<Vec<_>>(),
            "shared_origins":report.shared_origins.iter().map(|x| json!({
                "left_report_index":x.left_report_index,"right_report_index":x.right_report_index,"origins":x.origins
            })).collect::<Vec<_>>(),
            "independence_established":false
        }),
        StatusCode::OK,
    ))
}
async fn propose_leads(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<AssessmentRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    if request.report_ids.is_empty()
        || request.report_ids.len() > 64
        || request
            .report_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != request.report_ids.len()
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "report identifiers must be 1..64 unique values",
        ));
    }
    let snapshot = postgres_repository::load_workspace_graph(&state.pool, workspace).await?;
    let assessment = snapshot.assess(&request.report_ids)?;
    let leads = discovery::propose(&assessment);
    Ok(no_store(
        json!({
            "revision":snapshot.revision(),
            "report_ids":request.report_ids,
            "proposals":leads,
            "human_review_required":true,
            "generated_from":"deterministic_provenance_heuristics",
            "independence_established":false
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceQuery {
    limit: Option<i64>,
    after: Option<Uuid>,
}
async fn list_sources(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<SourceQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = source_catalog::list_sources(
        &state.pool,
        workspace,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(|e| match e {
        source_catalog::CatalogError::InvalidLimit => {
            ApiError(StatusCode::BAD_REQUEST, "invalid page limit")
        }
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "source catalog unavailable",
        ),
    })?;
    Ok(no_store(
        json!({
            "documents":page.documents.iter().map(|d|json!({
                "document_id":d.document_id,"format":d.format,"locator":d.locator,
                "sha256":d.sha256,"byte_length":d.byte_length,"fragment_count":d.fragment_count
            })).collect::<Vec<_>>(),"next_after":page.next_after
        }),
        StatusCode::OK,
    ))
}

async fn ingest(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Write).await?;
    let format = match headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        Some("message/rfc822") => SourceFormat::Eml,
        Some("text/plain") => SourceFormat::WhatsappExport,
        _ => {
            return Err(ApiError(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected message/rfc822 or text/plain",
            ));
        }
    };
    let locator = headers
        .get("x-source-locator")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError(StatusCode::BAD_REQUEST, "missing source locator"))?;
    let result =
        postgres_ingest::ingest_source(&state.pool, workspace, document, format, locator, &body)
            .await?;
    Ok(no_store(
        json!({"workspace_id":workspace,"document_id":document,
        "sha256":result.sha256,"fragment_ids":result.fragment_ids,
        "replayed":result.replayed,"revision":result.graph_revision}),
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

impl From<FeedbackError> for ApiError {
    fn from(error: FeedbackError) -> Self {
        match error {
            FeedbackError::Invalid => Self(StatusCode::BAD_REQUEST, "invalid feedback input"),
            FeedbackError::Conflict => Self(StatusCode::CONFLICT, "feedback ID conflict"),
            FeedbackError::Invisible => {
                Self(StatusCode::NOT_FOUND, "question or workspace unavailable")
            }
            FeedbackError::Sql(sqlx::Error::Database(ref db))
                if matches!(db.code().as_deref(), Some("23514" | "23503")) =>
            {
                Self(
                    StatusCode::BAD_REQUEST,
                    "question feedback referential validation failed",
                )
            }
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "feedback unavailable"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewQuestion {
    text: String,
}
async fn create_question(
    State(state): State<AppState>,
    Path((workspace, question)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<NewQuestion>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Write).await?;
    let created =
        human_feedback::create_question(&state.pool, workspace, question, &body.text).await?;
    Ok(no_store(
        json!({"question_id":question,"text":body.text,"replayed":!created,
        "status":"PROPOSED","is_evidence":false}),
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionsQuery {
    limit: Option<i64>,
    after: Option<Uuid>,
}
async fn list_questions(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Query(query): Query<QuestionsQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (questions, next_after) = human_feedback::list_questions(
        &state.pool,
        workspace,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({"questions":questions.iter().map(|q|json!({
        "question_id":q.question_id,"text":q.text,"status":q.status,"is_evidence":false
    })).collect::<Vec<_>>(),"next_after":next_after}),
        StatusCode::OK,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewFeedback {
    response: String,
    human_reason: String,
}
async fn record_feedback(
    State(state): State<AppState>,
    Path((workspace, question, event)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<NewFeedback>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let response = HumanResponse::parse(&body.response).ok_or(ApiError(
        StatusCode::BAD_REQUEST,
        "invalid feedback response",
    ))?;
    let event = human_feedback::record_feedback(
        &state.pool,
        workspace,
        actor,
        event,
        question,
        response,
        &body.human_reason,
    )
    .await?;
    Ok(no_store(
        json!({"event_id":event.event_id,"event_no":event.event_no,
        "question_id":question,"response":event.response,"human_reason":event.human_reason,
        "actor_principal_id":event.actor_principal_id,"replayed":event.replayed,
        "is_evidence":false}),
        if event.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeedbackQuery {
    after: Option<i64>,
    limit: Option<i64>,
}
async fn feedback_history(
    State(state): State<AppState>,
    Path((workspace, question)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<FeedbackQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (events, next_after) = human_feedback::feedback_history(
        &state.pool,
        workspace,
        question,
        query.after.unwrap_or(0),
        query.limit.unwrap_or(50),
    )
    .await?;
    Ok(no_store(
        json!({"question_id":question,"next_after":next_after,
        "events":events.iter().map(|e|json!({"event_id":e.event_id,"event_no":e.event_no,
           "response":e.response,"human_reason":e.human_reason,
           "actor_principal_id":e.actor_principal_id,"is_evidence":false
        })).collect::<Vec<_>>() }),
        StatusCode::OK,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionQuery {
    after: Option<Uuid>,
    limit: Option<i64>,
}
async fn question_positions(
    State(state): State<AppState>,
    Path((workspace, question)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<PositionQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (positions, next_after) = human_feedback::positions(
        &state.pool,
        workspace,
        question,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({"question_id":question,"next_after":next_after,
      "positions":positions.iter().map(|x| json!({"actor_principal_id":x.actor_principal_id,
         "event_id":x.event_id,"event_no":x.event_no,"response":x.response,
         "human_reason":x.human_reason,"is_evidence":false})).collect::<Vec<_>>(),
      "consensus_established":false}),
        StatusCode::OK,
    ))
}
impl From<ReferenceError> for ApiError {
    fn from(e: ReferenceError) -> Self {
        match e {
            ReferenceError::Invalid => Self(StatusCode::BAD_REQUEST, "invalid evidence reference"),
            ReferenceError::Conflict => Self(StatusCode::CONFLICT, "reference ID conflict"),
            ReferenceError::Invisible => {
                Self(StatusCode::NOT_FOUND, "question or evidence unavailable")
            }
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "reference unavailable"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewEvidenceReference {
    evidence_id: Uuid,
    relation: String,
    human_reason: String,
}
async fn attach_question_ref(
    State(state): State<AppState>,
    Path((workspace, question, reference)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<NewEvidenceReference>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let saved = question_refs::attach(
        &state.pool,
        workspace,
        actor,
        reference,
        question,
        input.evidence_id,
        &input.relation,
        &input.human_reason,
    )
    .await?;
    Ok(no_store(
        json!({"reference_id":reference,"question_id":question,"evidence_id":saved.evidence_id,
        "relation":saved.relation,"human_reason":saved.human_reason,"actor_principal_id":saved.actor_principal_id,
        "replayed":saved.replayed,"is_evidence":false,"proof_established":false}),
        if saved.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefQuery {
    limit: Option<i64>,
    after: Option<Uuid>,
}
async fn list_question_refs(
    State(state): State<AppState>,
    Path((workspace, question)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<RefQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (refs, next_after) = question_refs::list(
        &state.pool,
        workspace,
        question,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({"question_id":question,"next_after":next_after,"proof_established":false,
        "references":refs.iter().map(|r|json!({"reference_id":r.reference_id,"evidence_id":r.evidence_id,
            "relation":r.relation,"human_reason":r.human_reason,"actor_principal_id":r.actor_principal_id,
            "is_evidence":false})).collect::<Vec<_>>() }),
        StatusCode::OK,
    ))
}
impl From<IdentityError> for ApiError {
    fn from(e: IdentityError) -> Self {
        match e {
            IdentityError::Sql(sqlx::Error::Database(ref db))
                if matches!(db.code().as_deref(), Some("23514" | "23503")) =>
            {
                Self(
                    StatusCode::BAD_REQUEST,
                    "identity referential validation failed",
                )
            }
            IdentityError::Input => Self(StatusCode::BAD_REQUEST, "invalid identity input"),
            IdentityError::Conflict => Self(StatusCode::CONFLICT, "identity or event ID conflict"),
            IdentityError::Invisible => {
                Self(StatusCode::NOT_FOUND, "document or workspace unavailable")
            }
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "identity unavailable"),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityCreate {
    display_label: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HypothesisInput {
    kind: String,
    left_evidence_id: Uuid,
    right_evidence_id: Uuid,
    human_note: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HypothesisPageQuery {
    after: Option<Uuid>,
    limit: Option<i64>,
}
impl From<HypothesisError> for ApiError {
    fn from(err: HypothesisError) -> Self {
        match err {
            HypothesisError::Invalid => Self(StatusCode::BAD_REQUEST, "invalid human hypothesis"),
            HypothesisError::Conflict => Self(StatusCode::CONFLICT, "hypothesis ID conflict"),
            HypothesisError::Invisible => {
                Self(StatusCode::NOT_FOUND, "workspace or evidence unavailable")
            }
            HypothesisError::Database(sqlx::Error::Database(ref db))
                if matches!(db.code().as_deref(), Some("23514" | "23503")) =>
            {
                Self(StatusCode::BAD_REQUEST, "invalid hypothesis reference")
            }
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "hypotheses unavailable"),
        }
    }
}
async fn record_hypothesis(
    State(state): State<AppState>,
    Path((workspace, hypothesis)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<HypothesisInput>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let r = hypotheses::record(
        &state.pool,
        workspace,
        actor,
        hypothesis,
        &input.kind,
        input.left_evidence_id,
        input.right_evidence_id,
        &input.human_note,
    )
    .await?;
    Ok(no_store(
        json!({
            "hypothesis_id":r.id,"kind":r.kind,"left_evidence_id":r.left,
            "right_evidence_id":r.right,"human_note":r.human_note,
            "actor_principal_id":r.actor,"replayed":r.replayed,
            "status":"HUMAN_HYPOTHESIS","is_verified_fact":false
        }),
        if r.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}
async fn list_hypotheses(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Query(query): Query<HypothesisPageQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (records, next_after) = hypotheses::list(
        &state.pool,
        workspace,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "hypotheses":records.iter().map(|r|json!({
                "hypothesis_id":r.id,"kind":r.kind,"left_evidence_id":r.left,
                "right_evidence_id":r.right,"human_note":r.human_note,
                "actor_principal_id":r.actor,"status":"HUMAN_HYPOTHESIS","is_verified_fact":false
            })).collect::<Vec<_>>(),
            "next_after":next_after
        }),
        StatusCode::OK,
    ))
}

async fn create_identity(
    State(state): State<AppState>,
    Path((workspace, person)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<IdentityCreate>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Write).await?;
    let created =
        identity::create_identity(&state.pool, workspace, person, &input.display_label).await?;
    Ok(no_store(
        json!({"person_id":person,"display_label":input.display_label,"replayed":!created}),
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityEventRequest {
    document_id: Uuid,
    speaker_label: String,
    operation: String,
    person_id: Option<Uuid>,
    human_reason: String,
}
async fn record_identity_event(
    State(state): State<AppState>,
    Path((workspace, event_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<IdentityEventRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let operation = match input.operation.as_str() {
        "assign" => IdentityAction::Assign,
        "clear" => IdentityAction::Clear,
        _ => {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "invalid identity operation",
            ));
        }
    };
    let result = identity::record_event(
        &state.pool,
        workspace,
        actor,
        event_id,
        input.document_id,
        &input.speaker_label,
        operation,
        input.person_id,
        &input.human_reason,
    )
    .await?;
    Ok(no_store(
        json!({"event_id":result.event_id,"event_no":result.event_no,
        "document_id":result.document_id,"speaker_label":result.speaker_label,
        "operation":result.action,"person_id":result.person_id,
        "actor_principal_id":result.actor_principal_id,"human_reason":result.human_reason,
        "replayed":result.replayed}),
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}
async fn effective_identities(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let bindings = identity::effective_bindings(&state.pool, workspace, document).await?;
    Ok(no_store(
        json!({"document_id":document,"bindings":bindings.iter().map(|b|json!({
       "speaker_label":b.speaker_label,"person_id":b.person_id,"event_id":b.event_id,
       "event_no":b.event_no
    })).collect::<Vec<_>>()}),
        StatusCode::OK,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryQuery {
    after: Option<i64>,
    limit: Option<i64>,
}
async fn identity_history(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (events, next_after) = identity::event_history(
        &state.pool,
        workspace,
        document,
        query.after.unwrap_or(0),
        query.limit.unwrap_or(50),
    )
    .await?;
    Ok(no_store(
        json!({"document_id":document,"next_after":next_after,
           "events":events.iter().map(|e|json!({"event_id":e.event_id,"event_no":e.event_no,
              "speaker_label":e.speaker_label,"operation":e.action,"person_id":e.person_id,
              "actor_principal_id":e.actor_principal_id,"human_reason":e.human_reason
           })).collect::<Vec<_>>()
        }),
        StatusCode::OK,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    #[test]
    fn strict_bearer_header() {
        let mut h = HeaderMap::new();
        assert!(bearer(&h).is_err());
        h.insert(header::AUTHORIZATION, "basic test".parse().unwrap());
        assert!(bearer(&h).is_err());
        h.insert(header::AUTHORIZATION, "Bearer ncl_test".parse().unwrap());
        assert_eq!(bearer(&h).unwrap(), "ncl_test");
    }
    #[test]
    fn ambiguous_bearer_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(header::AUTHORIZATION, "Bearer token_one".parse().unwrap());
        headers.append(header::AUTHORIZATION, "Bearer token_two".parse().unwrap());
        assert!(bearer(&headers).is_err());
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer token_one".parse().unwrap());
        assert_eq!(bearer(&headers).unwrap(), "token_one");
    }
    #[tokio::test]
    async fn all_http_responses_have_privacy_headers() {
        use tower::ServiceExt;
        let router = router(PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap());
        for path in ["/healthz", "/does-not-exist"] {
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
            assert_eq!(response.headers()["referrer-policy"], "no-referrer");
            assert_eq!(response.headers()["x-frame-options"], "DENY");
        }
    }
    #[tokio::test]
    async fn framework_generated_errors_keep_privacy_headers() {
        use tower::ServiceExt;
        let router = router(PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap());
        let too_big = vec![0_u8; 4 * 1024 * 1024 + 1];
        let attempts = [
            (
                "PUT",
                "/healthz",
                Vec::new(),
                StatusCode::METHOD_NOT_ALLOWED,
            ),
            (
                "POST",
                "/v1/workspaces/00000000-0000-0000-0000-000000000001/sources/00000000-0000-0000-0000-000000000002",
                too_big,
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ];
        for (method, path, body, expected) in attempts {
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        }
    }
    #[tokio::test]
    async fn health_is_not_a_private_data_endpoint() {
        use tower::ServiceExt;
        let router = router(PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap());
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
