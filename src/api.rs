//! Bound, explicit HTTP boundary. Not a public Internet deployment template.
//! AuthN/AuthZ decision precedes EVERY workspace read and write. The client
//! may select a workspace but can never authorize itself by merely naming it.
use crate::analysis_context_summary::{self, AnalysisContextSummaryError};
use crate::analysis_context_usage::{self, AnalysisContextUsageError};
use crate::analysis_coverage::{self, AnalysisCoverageError};
use crate::analysis_delta::{self, AnalysisDeltaError};
use crate::analysis_output::{self, AnalysisOutputError};
use crate::analysis_review::{self, AnalysisReviewError};
use crate::analysis_snapshot::{self, AnalysisSnapshotError};
use crate::authz::{self, AuthError, Capability};
use crate::bff_operations::{self, BffOperationError, NewOperation, OperationScope};
use crate::clarification::{self, ClarificationError};
use crate::clarification_answer::{self, ClarificationAnswerError};
use crate::discovery;
use crate::human_feedback::{self, FeedbackError, HumanResponse};
use crate::hypotheses::{self, HypothesisError};
use crate::identity::{self, IdentityAction, IdentityError};
use crate::model_adapter::{ReasoningModel, UnavailableModel};
use crate::postgres_ingest::{self, ImportError};
use crate::postgres_repository::{self, RepositoryError};
use crate::question_refs::{self, ReferenceError};
use crate::situation_store::{self, ContributionKind, SituationStoreError};
use crate::source_catalog;
use crate::source_import::SourceFormat;
use crate::{agentic_controller, agentic_policy, bff_dashboard, bff_executor};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub model: Arc<dyn ReasoningModel>,
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
    headers.insert(
        "content-security-policy",
        "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
            .parse()
            .unwrap(),
    );
    response
}

async fn frontend_index() -> Html<&'static str> {
    Html(include_str!("../frontend/dist/index.html"))
}

pub fn router(pool: PgPool) -> Router {
    router_with_model(pool, Arc::new(UnavailableModel))
}

pub fn router_with_model(pool: PgPool, model: Arc<dyn ReasoningModel>) -> Router {
    Router::new()
        .route(
            "/",
            get(frontend_index),
        )
        .route("/analysis", get(frontend_index))
        .route("/operations", get(frontend_index))
        .route("/review", get(frontend_index))
        .route("/inspector", get(frontend_index))
        .route(
            "/assets/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../frontend/dist/assets/app.js"),
                )
            }),
        )
        .route(
            "/assets/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../frontend/dist/assets/app.css"),
                )
            }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/api.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/api.js"),
                )
            }),
        )
        .route(
            "/state.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/state.js"),
                )
            }),
        )
        .route(
            "/styles.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/styles.css"),
                )
            }),
        )
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
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}",
            get(get_situation).post(create_situation),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/focus",
            post(set_situation_focus),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/sources/{document}",
            post(add_situation_source),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/contributions/{contribution}",
            post(add_situation_contribution),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/contributions/{contribution}/corrections/{replacement}",
            post(correct_situation_contribution),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/contributions/{contribution}/retract",
            post(retract_situation_contribution),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/revisions",
            get(list_situation_revisions),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/revisions/{revision}",
            get(get_situation_revision),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-history",
            get(list_analysis_history),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/sync",
            post(bff_operation_sync),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/async",
            post(bff_operation_async),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}",
            get(get_bff_operation),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/events",
            get(get_bff_operation_events),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/wait",
            get(wait_bff_operation),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/agentic",
            get(get_agentic_operation).post(plan_agentic_operation),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/agentic/steps/{step}/human-response",
            post(resolve_agentic_human_boundary),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/cancel",
            post(cancel_bff_operation),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}",
            get(get_analysis_snapshot).post(begin_analysis_snapshot),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/dashboard",
            get(get_bff_dashboard),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/input",
            get(get_analysis_input),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/output",
            get(get_analysis_output).post(publish_analysis_output),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/examined-fragments",
            get(list_examined_fragments),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/coverage",
            get(analysis_coverage_summary),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/used-contributions",
            get(list_used_contributions),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/context-usage",
            get(analysis_context_summary),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/delta",
            get(analysis_delta),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/review",
            get(analysis_review),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/used-contributions/{contribution}",
            post(record_used_contribution),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/clarifications",
            get(list_analysis_clarifications),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/clarifications/{question}",
            post(propose_analysis_clarification),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/clarifications/{question}/answers",
            get(list_clarification_answers),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/clarifications/{question}/answers/{answer}",
            post(answer_clarification),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/examined-fragments/{evidence}",
            post(record_examined_fragment),
        )
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(AppState { pool, model })
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

impl From<AnalysisSnapshotError> for ApiError {
    fn from(error: AnalysisSnapshotError) -> Self {
        match error {
            AnalysisSnapshotError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid analysis snapshot request")
            }
            AnalysisSnapshotError::Conflict => {
                Self(StatusCode::CONFLICT, "analysis identifier conflict")
            }
            AnalysisSnapshotError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis snapshot unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis snapshot unavailable",
            ),
        }
    }
}

impl From<AnalysisContextSummaryError> for ApiError {
    fn from(error: AnalysisContextSummaryError) -> Self {
        match error {
            AnalysisContextSummaryError::Invalid => Self(
                StatusCode::BAD_REQUEST,
                "invalid analysis context summary request",
            ),
            AnalysisContextSummaryError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis snapshot unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis context summary unavailable",
            ),
        }
    }
}

impl From<AnalysisContextUsageError> for ApiError {
    fn from(error: AnalysisContextUsageError) -> Self {
        match error {
            AnalysisContextUsageError::Invalid => Self(
                StatusCode::BAD_REQUEST,
                "invalid analysis context usage request",
            ),
            AnalysisContextUsageError::Conflict => {
                Self(StatusCode::CONFLICT, "analysis context usage conflict")
            }
            AnalysisContextUsageError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis contribution unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis context usage unavailable",
            ),
        }
    }
}

impl From<AnalysisReviewError> for ApiError {
    fn from(error: AnalysisReviewError) -> Self {
        match error {
            AnalysisReviewError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis snapshot unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis review unavailable",
            ),
        }
    }
}

impl From<AnalysisDeltaError> for ApiError {
    fn from(error: AnalysisDeltaError) -> Self {
        match error {
            AnalysisDeltaError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis snapshot unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis delta unavailable",
            ),
        }
    }
}

impl From<AnalysisCoverageError> for ApiError {
    fn from(error: AnalysisCoverageError) -> Self {
        match error {
            AnalysisCoverageError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid analysis coverage request")
            }
            AnalysisCoverageError::Conflict => {
                Self(StatusCode::CONFLICT, "analysis coverage conflict")
            }
            AnalysisCoverageError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis fragment unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis coverage unavailable",
            ),
        }
    }
}

impl From<ClarificationAnswerError> for ApiError {
    fn from(error: ClarificationAnswerError) -> Self {
        match error {
            ClarificationAnswerError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid clarification answer")
            }
            ClarificationAnswerError::Conflict => {
                Self(StatusCode::CONFLICT, "clarification answer conflict")
            }
            ClarificationAnswerError::Invisible => {
                Self(StatusCode::NOT_FOUND, "clarification unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "clarification answer unavailable",
            ),
        }
    }
}

impl From<ClarificationError> for ApiError {
    fn from(error: ClarificationError) -> Self {
        match error {
            ClarificationError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid clarification request")
            }
            ClarificationError::Conflict => {
                Self(StatusCode::CONFLICT, "clarification identifier conflict")
            }
            ClarificationError::Invisible => Self(
                StatusCode::NOT_FOUND,
                "analysis or clarification unavailable",
            ),
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "clarification unavailable",
            ),
        }
    }
}

impl From<AnalysisOutputError> for ApiError {
    fn from(error: AnalysisOutputError) -> Self {
        match error {
            AnalysisOutputError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid analysis output")
            }
            AnalysisOutputError::Conflict => Self(StatusCode::CONFLICT, "analysis output conflict"),
            AnalysisOutputError::Invisible => {
                Self(StatusCode::NOT_FOUND, "analysis output unavailable")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "analysis output unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisHistoryQuery {
    before: Option<Uuid>,
    limit: Option<i64>,
}

async fn list_analysis_history(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<AnalysisHistoryQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (entries, next_before) = analysis_snapshot::history(
        &state.pool,
        workspace,
        situation,
        query.before,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "situation_id": situation,
            "next_before": next_before,
            "analyses": entries.iter().map(|entry| json!({
                "analysis_id": entry.analysis_id,
                "input_revision": entry.input_revision,
                "input_fingerprint": entry.input_fingerprint,
                "goal": entry.goal,
                "requested_by_principal_id": entry.requested_by,
                "reanalysis_of": entry.reanalysis_of,
                "answer_text": entry.answer_text,
                "coverage_note": entry.coverage_note,
                "published_by_principal_id": entry.published_by,
                "current_situation_revision": entry.current_revision,
                "input_is_current": entry.input_is_current,
                "historical_output_preserved": entry.answer_text.is_some()
            })).collect::<Vec<_>>()
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisSnapshotRequest {
    goal: String,
    reanalysis_of: Option<Uuid>,
}

fn analysis_snapshot_json(snapshot: &analysis_snapshot::AnalysisInputSnapshot) -> Value {
    json!({
        "analysis_id": snapshot.analysis_id,
        "situation_id": snapshot.situation_id,
        "input_revision": snapshot.input_revision,
        "input_fingerprint": snapshot.input_fingerprint,
        "goal": snapshot.goal,
        "requested_by_principal_id": snapshot.requested_by,
        "reanalysis_of": snapshot.reanalysis_of,
        "replayed": snapshot.replayed,
        "frozen_input": true,
        "incorporates_future_context": false
    })
}

async fn begin_analysis_snapshot(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<AnalysisSnapshotRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let snapshot = analysis_snapshot::begin(
        &state.pool,
        workspace,
        actor,
        situation,
        analysis,
        &input.goal,
        input.reanalysis_of,
    )
    .await?;
    Ok(no_store(
        analysis_snapshot_json(&snapshot),
        if snapshot.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn get_analysis_snapshot(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let snapshot = analysis_snapshot::get(&state.pool, workspace, situation, analysis).await?;
    Ok(no_store(analysis_snapshot_json(&snapshot), StatusCode::OK))
}

async fn get_analysis_input(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let materialized =
        analysis_snapshot::materialize(&state.pool, workspace, situation, analysis).await?;
    Ok(no_store(
        json!({
            "analysis": analysis_snapshot_json(&materialized.analysis),
            "input": situation_json(&materialized.input),
            "current_situation_revision": materialized.current_revision,
            "input_is_current": materialized.input_is_current,
            "future_context_included": false
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisOutputRequest {
    answer_text: String,
    coverage_note: String,
}

fn analysis_output_json(output: &analysis_output::AnalysisOutput) -> Value {
    json!({
        "analysis_id": output.analysis_id,
        "situation_id": output.situation_id,
        "input_revision": output.input_revision,
        "input_fingerprint": output.input_fingerprint,
        "answer_text": output.answer_text,
        "coverage_note": output.coverage_note,
        "published_by_principal_id": output.published_by,
        "current_situation_revision": output.current_revision,
        "input_is_current": output.input_is_current,
        "replayed": output.replayed,
        "historical_output_preserved": true
    })
}

async fn publish_analysis_output(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<AnalysisOutputRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let output = analysis_output::publish(
        &state.pool,
        workspace,
        actor,
        situation,
        analysis,
        &input.answer_text,
        &input.coverage_note,
    )
    .await?;
    Ok(no_store(
        analysis_output_json(&output),
        if output.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn get_analysis_output(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let output = analysis_output::get(&state.pool, workspace, situation, analysis).await?;
    Ok(no_store(analysis_output_json(&output), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClarificationRequest {
    text: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClarificationQuery {
    after: Option<Uuid>,
    limit: Option<i64>,
}

fn clarification_json(question: &clarification::Clarification) -> Value {
    json!({
        "question_id": question.question_id,
        "text": question.text,
        "reason": question.reason,
        "proposed_by_principal_id": question.proposed_by,
        "input_revision": question.input_revision,
        "current_situation_revision": question.current_revision,
        "input_is_current": question.input_revision == question.current_revision,
        "replayed": question.replayed,
        "is_evidence": false,
        "purpose": "TARGETED_CLARIFICATION"
    })
}

async fn propose_analysis_clarification(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, question)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<ClarificationRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let question = clarification::propose(
        &state.pool,
        workspace,
        actor,
        situation,
        analysis,
        question,
        &input.text,
        &input.reason,
    )
    .await?;
    Ok(no_store(
        clarification_json(&question),
        if question.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_analysis_clarifications(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<ClarificationQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (questions, next_after) = clarification::list(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "analysis_id": analysis,
            "next_after": next_after,
            "clarifications": questions.iter().map(clarification_json).collect::<Vec<_>>(),
            "questions_are_not_evidence": true,
            "later_context_does_not_rewrite_question_origin": true
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClarificationAnswerRequest {
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClarificationAnswerQuery {
    after: Option<Uuid>,
    limit: Option<i64>,
}

fn clarification_answer_json(answer: &clarification_answer::ClarificationAnswer) -> Value {
    json!({
        "answer_id": answer.answer_id,
        "question_id": answer.question_id,
        "analysis_id": answer.analysis_id,
        "contribution_id": answer.contribution_id,
        "text": answer.text,
        "answered_by_principal_id": answer.answered_by,
        "question_input_revision": answer.question_input_revision,
        "answer_revision": answer.answer_revision,
        "current_situation_revision": answer.current_revision,
        "answer_is_in_original_analysis_input": answer.answer_revision <= answer.question_input_revision,
        "kind": "USER_REPORTED",
        "is_evidence": false,
        "replayed": answer.replayed
    })
}

async fn answer_clarification(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, question, answer)): Path<(Uuid, Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<ClarificationAnswerRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let answer = clarification_answer::answer(
        &state.pool,
        workspace,
        actor,
        situation,
        analysis,
        question,
        answer,
        &input.text,
    )
    .await?;
    Ok(no_store(
        clarification_answer_json(&answer),
        if answer.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_clarification_answers(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, question)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<ClarificationAnswerQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (answers, next_after) = clarification_answer::list(
        &state.pool,
        workspace,
        situation,
        analysis,
        question,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "analysis_id": analysis,
            "question_id": question,
            "next_after": next_after,
            "answers": answers.iter().map(clarification_answer_json).collect::<Vec<_>>(),
            "answers_are_new_attributed_context": true,
            "answers_do_not_rewrite_frozen_analysis_input": true,
            "answers_are_not_evidence": true
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsedContributionQuery {
    after: Option<Uuid>,
    limit: Option<i64>,
}

fn used_contribution_json(item: &analysis_context_usage::UsedContribution) -> Value {
    json!({
        "contribution_id":item.contribution_id,
        "author_principal_id":item.author,
        "kind":item.kind,
        "text":item.text,
        "scope":item.scope,
        "related_evidence_id":item.related_evidence_id,
        "input_revision":item.input_revision,
        "recorded_by_principal_id":item.recorded_by,
        "replayed":item.replayed,
        "used_does_not_imply_true":true,
        "is_verified_evidence":false
    })
}

async fn record_used_contribution(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, contribution)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let item = analysis_context_usage::record(
        &state.pool,
        workspace,
        actor,
        situation,
        analysis,
        contribution,
    )
    .await?;
    Ok(no_store(
        used_contribution_json(&item),
        if item.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_used_contributions(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<UsedContributionQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (items, next_after) = analysis_context_usage::list(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "analysis_id":analysis,
            "next_after":next_after,
            "used_contributions":items.iter().map(used_contribution_json).collect::<Vec<_>>(),
            "presence_in_input_does_not_imply_use":true,
            "use_does_not_imply_truth":true
        }),
        StatusCode::OK,
    ))
}

async fn analysis_context_summary(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<UsedContributionQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (items, next_after) = analysis_context_summary::list(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.after,
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "analysis_id":analysis,
            "input_revision":items.first().map(|item| item.input_revision),
            "next_after":next_after,
            "contributions":items.iter().map(|item| json!({
                "contribution_id":item.contribution_id,
                "author_principal_id":item.author,
                "kind":item.kind,
                "text":item.text,
                "scope":item.scope,
                "related_evidence_id":item.related_evidence_id,
                "present_in_frozen_input":true,
                "used_by_analysis":item.used_by_analysis,
                "use_recorded_by_principal_id":item.use_recorded_by,
                "is_verified_evidence":false
            })).collect::<Vec<_>>(),
            "presence_in_input_does_not_imply_use":true,
            "use_does_not_imply_truth":true
        }),
        StatusCode::OK,
    ))
}

async fn analysis_delta(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let delta = analysis_delta::get(&state.pool, workspace, situation, analysis).await?;
    Ok(no_store(
        json!({
            "analysis_id":delta.analysis_id,
            "input_revision":delta.input_revision,
            "current_situation_revision":delta.current_revision,
            "input_fingerprint":delta.input_fingerprint,
            "current_fingerprint":delta.current_fingerprint,
            "input_is_current":delta.input_is_current,
            "focus_changed":delta.focus_before!=delta.focus_now,
            "focus_before":delta.focus_before,
            "focus_now":delta.focus_now,
            "added_source_document_ids":delta.added_sources,
            "removed_source_document_ids":delta.removed_sources,
            "added_contributions":delta.added_contributions.iter().map(|c|json!({
                "contribution_id":c.id,"author_principal_id":c.author,"kind":c.kind,
                "text":c.text,"scope":c.scope,"related_evidence_id":c.related_evidence_id,
                "is_verified_evidence":false
            })).collect::<Vec<_>>(),
            "removed_contributions":delta.removed_contributions.iter().map(|c|json!({
                "contribution_id":c.contribution_id,"kind":c.kind,"text":c.text,
                "current_status":c.current_status,"corrected_by":c.corrected_by,
                "is_verified_evidence":false
            })).collect::<Vec<_>>(),
            "delta_is_not_a_truth_judgement":true,
            "reanalysis_is_not_automatic":true
        }),
        StatusCode::OK,
    ))
}

async fn analysis_review(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let r = analysis_review::get(&state.pool, workspace, situation, analysis).await?;
    Ok(no_store(
        json!({
            "analysis_id":r.analysis_id,"goal":r.goal,"requested_by_principal_id":r.requested_by,
            "reanalysis_of":r.reanalysis_of,"input_revision":r.input_revision,
            "current_situation_revision":r.current_revision,"input_is_current":r.input_is_current,
            "output_present":r.output_present,
            "sources":{"present_in_input":r.input_source_count,"catalogued_fragments":r.catalogued_fragment_count,"examined_fragments":r.examined_fragment_count},
            "human_context":{"present_in_input":r.input_context_count,"recorded_as_used":r.used_context_count},
            "clarifications":{"questions":r.clarification_question_count,"answered_questions":r.answered_question_count,"answers":r.clarification_answer_count},
            "delta":{"focus_changed":r.focus_changed,"sources_added":r.added_source_count,"sources_removed":r.removed_source_count,"context_added":r.added_context_count,"context_removed":r.removed_context_count},
            "counts_do_not_establish_completeness_or_truth":true,
            "human_review_required":true
        }),
        StatusCode::OK,
    ))
}

fn examined_fragment_json(fragment: &analysis_coverage::ExaminedFragment) -> Value {
    json!({
        "evidence_id": fragment.evidence_id,
        "document_id": fragment.document_id,
        "ordinal": fragment.ordinal,
        "byte_start": fragment.byte_start,
        "byte_end": fragment.byte_end,
        "speaker_label": fragment.speaker_label,
        "recorded_by_principal_id": fragment.recorded_by,
        "replayed": fragment.replayed,
        "coverage_claim": "EXAMINED_FRAGMENT",
        "proves_full_source_examined": false
    })
}

async fn record_examined_fragment(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, evidence)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let fragment =
        analysis_coverage::record(&state.pool, workspace, actor, situation, analysis, evidence)
            .await?;
    Ok(no_store(
        examined_fragment_json(&fragment),
        if fragment.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisCoverageQuery {
    after: Option<Uuid>,
    limit: Option<i64>,
}

async fn analysis_coverage_summary(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<AnalysisCoverageQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (documents, next_after) = analysis_coverage::summarize(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.after,
        query.limit.unwrap_or(50),
    )
    .await?;
    Ok(no_store(
        json!({
            "analysis_id": analysis,
            "coverage_scope": "FROZEN_ANALYSIS_INPUT",
            "next_after": next_after,
            "documents": documents.iter().map(|document| json!({
                "document_id": document.document_id,
                "format": document.format,
                "catalogued_fragment_count": document.catalogued_fragment_count,
                "examined_fragment_count": document.examined_fragment_count,
                "all_catalogued_fragments_examined": document.catalogued_fragment_count > 0
                    && document.catalogued_fragment_count == document.examined_fragment_count,
                "proves_full_source_examined": false
            })).collect::<Vec<_>>(),
            "source_present_does_not_mean_examined": true,
            "all_catalogued_fragments_examined_does_not_prove_unsupported_media_examined": true
        }),
        StatusCode::OK,
    ))
}

async fn list_examined_fragments(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<AnalysisCoverageQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (fragments, next_after) = analysis_coverage::list(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.after,
        query.limit.unwrap_or(50),
    )
    .await?;
    Ok(no_store(
        json!({
            "analysis_id": analysis,
            "next_after": next_after,
            "fragments": fragments.iter().map(examined_fragment_json).collect::<Vec<_>>(),
            "empty_means_no_examined_fragment_was_recorded": true,
            "empty_does_not_mean_no_source_was_available": true
        }),
        StatusCode::OK,
    ))
}

impl From<SituationStoreError> for ApiError {
    fn from(error: SituationStoreError) -> Self {
        match error {
            SituationStoreError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid situation input")
            }
            SituationStoreError::Conflict => {
                Self(StatusCode::CONFLICT, "situation identifier conflict")
            }
            SituationStoreError::Invisible => {
                Self(StatusCode::NOT_FOUND, "situation or source unavailable")
            }
            SituationStoreError::Inactive => {
                Self(StatusCode::CONFLICT, "contribution is no longer active")
            }
            SituationStoreError::Database(sqlx::Error::Database(ref db))
                if matches!(db.code().as_deref(), Some("23514" | "23503")) =>
            {
                Self(
                    StatusCode::BAD_REQUEST,
                    "situation referential validation failed",
                )
            }
            _ => Self(StatusCode::INTERNAL_SERVER_ERROR, "situation unavailable"),
        }
    }
}

fn situation_json(snapshot: &situation_store::SituationSnapshot) -> Value {
    json!({
        "situation_id": snapshot.situation_id,
        "revision": snapshot.revision,
        "fingerprint": snapshot.fingerprint,
        "focus": snapshot.focus,
        "source_document_ids": snapshot.sources,
        "contributions": snapshot.contributions.iter().map(|c| json!({
            "contribution_id": c.id,
            "author_principal_id": c.author,
            "kind": c.kind,
            "text": c.text,
            "scope": c.scope,
            "related_evidence_id": c.related_evidence_id,
            "status_at_revision": "ACTIVE",
            "is_verified_evidence": false
        })).collect::<Vec<_>>(),
        "snapshot_is_frozen": true
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SituationCreateRequest {
    focus: String,
}

async fn create_situation(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<SituationCreateRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let result =
        situation_store::create(&state.pool, workspace, actor, situation, &input.focus).await?;
    let mut value = situation_json(&result.snapshot);
    value["replayed"] = json!(result.replayed);
    Ok(no_store(
        value,
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn get_situation(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let snapshot = situation_store::get_snapshot(&state.pool, workspace, situation, None).await?;
    Ok(no_store(situation_json(&snapshot), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SituationFocusRequest {
    focus: String,
}

async fn set_situation_focus(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<SituationFocusRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let result =
        situation_store::set_focus(&state.pool, workspace, actor, situation, &input.focus).await?;
    let mut value = situation_json(&result.snapshot);
    value["replayed"] = json!(result.replayed);
    Ok(no_store(value, StatusCode::OK))
}

async fn add_situation_source(
    State(state): State<AppState>,
    Path((workspace, situation, document)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let result =
        situation_store::add_source(&state.pool, workspace, actor, situation, document).await?;
    let mut value = situation_json(&result.snapshot);
    value["replayed"] = json!(result.replayed);
    Ok(no_store(
        value,
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SituationContributionRequest {
    kind: String,
    text: String,
    related_evidence_id: Option<Uuid>,
}

async fn add_situation_contribution(
    State(state): State<AppState>,
    Path((workspace, situation, contribution)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<SituationContributionRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let kind = ContributionKind::parse(&input.kind).ok_or(ApiError(
        StatusCode::BAD_REQUEST,
        "invalid contribution kind",
    ))?;
    let result = situation_store::add_contribution(
        &state.pool,
        workspace,
        actor,
        situation,
        contribution,
        kind,
        &input.text,
        input.related_evidence_id,
    )
    .await?;
    let mut value = situation_json(&result.snapshot);
    value["replayed"] = json!(result.replayed);
    Ok(no_store(
        value,
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SituationCorrectionRequest {
    text: String,
    related_evidence_id: Option<Uuid>,
}

async fn correct_situation_contribution(
    State(state): State<AppState>,
    Path((workspace, situation, contribution, replacement)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<SituationCorrectionRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let result = situation_store::correct_contribution(
        &state.pool,
        workspace,
        actor,
        situation,
        contribution,
        replacement,
        &input.text,
        input.related_evidence_id,
    )
    .await?;
    let mut value = situation_json(&result.snapshot);
    value["replayed"] = json!(result.replayed);
    Ok(no_store(
        value,
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn retract_situation_contribution(
    State(state): State<AppState>,
    Path((workspace, situation, contribution)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let result = situation_store::retract_contribution(
        &state.pool,
        workspace,
        actor,
        situation,
        contribution,
    )
    .await?;
    let mut value = situation_json(&result.snapshot);
    value["replayed"] = json!(result.replayed);
    Ok(no_store(value, StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SituationRevisionQuery {
    after: Option<i64>,
    limit: Option<i64>,
}

async fn list_situation_revisions(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<SituationRevisionQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (revisions, next_after) = situation_store::list_revisions(
        &state.pool,
        workspace,
        situation,
        query.after.unwrap_or(0),
        query.limit.unwrap_or(25),
    )
    .await?;
    Ok(no_store(
        json!({
            "situation_id": situation,
            "revisions": revisions.iter().map(|r| json!({
                "revision": r.revision,
                "fingerprint": r.fingerprint,
                "focus": r.focus,
                "actor_principal_id": r.actor,
                "cause": r.cause
            })).collect::<Vec<_>>(),
            "next_after": next_after
        }),
        StatusCode::OK,
    ))
}

async fn get_situation_revision(
    State(state): State<AppState>,
    Path((workspace, situation, revision)): Path<(Uuid, Uuid, i64)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let snapshot =
        situation_store::get_snapshot(&state.pool, workspace, situation, Some(revision)).await?;
    Ok(no_store(situation_json(&snapshot), StatusCode::OK))
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
        for path in ["/", "/app.js", "/styles.css", "/healthz", "/does-not-exist"] {
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
            assert!(response.headers().contains_key("content-security-policy"));
        }
    }

    #[tokio::test]
    async fn first_party_shell_exposes_all_three_intentions_without_database_access() {
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let router = router(PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap());
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = std::str::from_utf8(&body).unwrap();
        assert!(html.contains("I am about to say or do something"));
        assert!(html.contains("I am trying to understand something"));
        assert!(html.contains("Help me figure out what matters"));
        assert!(html.contains("does not upload or claim to analyze them"));
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

#[derive(Deserialize)]
struct BffDashboardQuery {
    operation: Option<Uuid>,
}
async fn get_bff_dashboard(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(q): Query<BffDashboardQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = bff_dashboard::get(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
        q.operation,
    )
    .await
    .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "dashboard unavailable"))?;
    Ok(no_store(json!(view), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BffOperationRequest {
    kind: String,
    target_analysis_id: Option<Uuid>,
    parent_operation_id: Option<Uuid>,
    idempotency_key: Option<String>,
    #[serde(default = "empty_object")]
    input: Value,
    deadline_at: Option<String>,
    step_budget: Option<i32>,
}
fn empty_object() -> Value {
    json!({})
}
#[derive(Deserialize)]
struct BffEventsQuery {
    after: Option<i64>,
    limit: Option<i64>,
}
#[derive(Deserialize)]
struct BffWaitQuery {
    after_version: i64,
    timeout_ms: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BffCancelRequest {
    expected_version: i64,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgenticHumanResponseRequest {
    response_id: Uuid,
    decision: String,
    response_text: String,
}

fn bff_error(e: BffOperationError) -> ApiError {
    match e {
        BffOperationError::Invalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid operation request")
        }
        BffOperationError::Invisible => ApiError(StatusCode::NOT_FOUND, "operation not found"),
        BffOperationError::IdempotencyConflict | BffOperationError::StateConflict => {
            ApiError(StatusCode::CONFLICT, "operation conflict")
        }
        BffOperationError::UnsupportedKind => ApiError(
            StatusCode::NOT_IMPLEMENTED,
            "operation kind not executable yet",
        ),
        _ => ApiError(StatusCode::INTERNAL_SERVER_ERROR, "operation unavailable"),
    }
}
fn bff_response(view: bff_operations::OperationView, status: StatusCode) -> Response {
    let etag = view.etag.clone();
    let retry = view.retry_after_ms;
    let mut response = (status, Json(view)).into_response();
    response
        .headers_mut()
        .insert(header::ETAG, etag.parse().unwrap());
    if let Some(ms) = retry {
        response
            .headers_mut()
            .insert("x-poll-after-ms", ms.to_string().parse().unwrap());
        response.headers_mut().insert(
            header::RETRY_AFTER,
            ms.div_ceil(1000).to_string().parse().unwrap(),
        );
    }
    response
}
async fn submit_bff(
    state: &AppState,
    workspace: Uuid,
    situation: Uuid,
    operation: Uuid,
    actor: Uuid,
    request: BffOperationRequest,
) -> Result<bff_operations::OperationView, ApiError> {
    bff_operations::submit(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        &NewOperation {
            kind: request.kind,
            target_analysis_id: request.target_analysis_id,
            parent_operation_id: request.parent_operation_id,
            idempotency_key: request.idempotency_key,
            input: request.input,
            deadline_at: request.deadline_at,
            step_budget: request.step_budget.unwrap_or(32),
            requested_by: actor,
        },
    )
    .await
    .map_err(bff_error)
}
async fn bff_operation_sync(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<BffOperationRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    submit_bff(&state, workspace, situation, operation, actor, request).await?;
    let scope = OperationScope { workspace, situation, operation };
    let view = bff_executor::execute(&state.pool, scope, actor, state.model.as_ref())
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "operation execution failed"))?;
    Ok(bff_response(view, StatusCode::OK))
}
async fn bff_operation_async(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<BffOperationRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let digest = unique_bearer(&headers)
        .and_then(|token| authz::credential_digest(token).ok())
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))?;
    let view = submit_bff(&state, workspace, situation, operation, actor, request).await?;
    if !view.terminal && view.state != "WAITING_FOR_HUMAN" {
        let pool = state.pool.clone();
        let model = state.model.clone();
        let scope = OperationScope { workspace, situation, operation };
        tokio::spawn(async move {
            authz::with_auth_digest(digest, async move {
                let _ = bff_executor::execute(&pool, scope, actor, model.as_ref()).await;
            }).await;
        });
    }
    Ok(bff_response(view, StatusCode::ACCEPTED))
}
async fn get_bff_operation(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = bff_operations::get(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
    )
    .await
    .map_err(bff_error)?;
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|x| x.to_str().ok())
        == Some(view.etag.as_str())
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        response
            .headers_mut()
            .insert(header::ETAG, view.etag.parse().unwrap());
        return Ok(response);
    }
    Ok(bff_response(view, StatusCode::OK))
}
async fn get_bff_operation_events(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(q): Query<BffEventsQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = bff_operations::events(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        q.after.unwrap_or(0),
        q.limit.unwrap_or(50),
    )
    .await
    .map_err(bff_error)?;
    Ok(no_store(json!(page), StatusCode::OK))
}
async fn wait_bff_operation(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(q): Query<BffWaitQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = bff_operations::wait_for_version(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        q.after_version,
        q.timeout_ms.unwrap_or(5000),
    )
    .await
    .map_err(bff_error)?;
    Ok(bff_response(view, StatusCode::OK))
}

async fn resolve_agentic_human_boundary(
    State(state): State<AppState>,
    Path((workspace, situation, operation, step)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<AgenticHumanResponseRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let resolved = agentic_controller::resolve_human_boundary(
        &state.pool,
        scope,
        step,
        request.response_id,
        actor,
        &request.decision,
        &request.response_text,
    )
    .await
    .map_err(|e| match e {
        agentic_controller::ControllerError::StateConflict => {
            ApiError(StatusCode::CONFLICT, "agentic human boundary conflict")
        }
        agentic_controller::ControllerError::Invisible => {
            ApiError(StatusCode::NOT_FOUND, "agentic step not found")
        }
        _ => ApiError(StatusCode::BAD_REQUEST, "invalid agentic human response"),
    })?;
    let controller = agentic_controller::view(&state.pool, scope)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "agentic plan unavailable",
            )
        })?;
    Ok(no_store(
        json!({"resolution":resolved,"controller":controller}),
        StatusCode::OK,
    ))
}
async fn get_agentic_operation(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = agentic_controller::view(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
    )
    .await
    .map_err(|_| ApiError(StatusCode::NOT_FOUND, "agentic plan not found"))?;
    Ok(no_store(json!(view), StatusCode::OK))
}
async fn plan_agentic_operation(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let op = bff_operations::get(&state.pool, scope)
        .await
        .map_err(bff_error)?;
    if op.kind != "AGENTIC_ANALYSIS" {
        return Err(ApiError(StatusCode::CONFLICT, "not an agentic operation"));
    }
    let analysis = op.target_analysis_id.ok_or(ApiError(
        StatusCode::BAD_REQUEST,
        "agentic operation requires analysis",
    ))?;
    let steps = agentic_policy::default_plan(&state.pool, workspace, situation, analysis)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "agentic planning unavailable",
            )
        })?;
    match agentic_controller::view(&state.pool, scope).await {
        Ok(view) => Ok(no_store(json!(view), StatusCode::OK)),
        Err(agentic_controller::ControllerError::Invisible) => {
            agentic_controller::create_plan(&state.pool, scope, actor, &steps)
                .await
                .map_err(|_| {
                    ApiError(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "agentic planning unavailable",
                    )
                })?;
            let view = agentic_controller::view(&state.pool, scope)
                .await
                .map_err(|_| {
                    ApiError(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "agentic plan unavailable",
                    )
                })?;
            Ok(no_store(json!(view), StatusCode::CREATED))
        }
        Err(_) => Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "agentic plan unavailable",
        )),
    }
}
async fn cancel_bff_operation(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<BffCancelRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let view = bff_operations::cancel(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        request.expected_version,
        actor,
        &request.reason,
    )
    .await
    .map_err(bff_error)?;
    Ok(bff_response(view, StatusCode::OK))
}
