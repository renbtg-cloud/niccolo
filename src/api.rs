//! Bound, explicit HTTP boundary. Not a public Internet deployment template.
//! AuthN/AuthZ decision precedes EVERY workspace read and write. The client
//! may select a workspace but can never authorize itself by merely naming it.
use crate::actors::{self, ActorError};
use crate::analysis_context_summary::{self, AnalysisContextSummaryError};
use crate::analysis_context_usage::{self, AnalysisContextUsageError};
use crate::analysis_coverage::{self, AnalysisCoverageError};
use crate::analysis_delta::{self, AnalysisDeltaError};
use crate::analysis_output::{self, AnalysisOutputError};
use crate::analysis_review::{self, AnalysisReviewError};
use crate::analysis_snapshot::{self, AnalysisSnapshotError};
use crate::authz::{self, AuthError, Capability};
use crate::bff_intent::{self, IntentError, IntentRequest};
use crate::bff_operations::{self, BffOperationError, NewOperation, OperationScope};
use crate::checkpoint_git_publication::{self, CheckpointGitPublicationError};
use crate::checkpoint_milestone::{self, MilestoneError};
use crate::checkpoint_package::{self, CheckpointPackageError};
use crate::clarification::{self, ClarificationError};
use crate::clarification_answer::{self, ClarificationAnswerError};
use crate::collection_provenance::{self, CollectionEventInput, CollectionProvenanceError};
use crate::decision_points::{self, DecisionError, DecisionOption, SelectionEvent};
use crate::decision_readiness::{
    self, DecisionReadinessError, DecisionReadinessInput, ReversalConditionInput,
};
use crate::discovery;
use crate::epistemic_policy_store::{self, NewPolicy, PolicyStoreError};
use crate::epistemic_weights::{self, WeightPolicy, WeightedObservation};
use crate::forensic_context::{self, ForensicContextError};
use crate::forensic_import::{self, ForensicImportError};
use crate::git_publication::{self, GitPublicationError};
use crate::github_publisher::{self, GitHubPublishError};
use crate::grounded_claim_weights::{self, DeclaredEvidenceStrength, GroundedWeightError};
use crate::grounded_score_store::{self, GroundedScoreStoreError, NewGroundedScoreRecord};
use crate::human_credence::{self, CredenceError, NewHumanCredence};
use crate::human_feedback::{self, FeedbackError, HumanResponse};
use crate::hypotheses::{self, HypothesisError};
use crate::hypothesis_portfolio::{self, HypothesisPortfolioError};
use crate::identity::{self, IdentityAction, IdentityError};
use crate::intake_git_publication::{self, IntakeGitPublicationError};
use crate::intake_package;
use crate::integration_diagnostics::{self, IntegrationDiagnosticsError};
use crate::integrity_scope::{self, IntegrityAssessmentInput, IntegrityScopeError};
use crate::local_handoff::{LocalHandoff, LocalHandoffError};
use crate::model_adapter::{ReasoningModel, UnavailableModel};
use crate::observation_retrieval::{self, ObservationSearchError};
use crate::operation_budget::{self, BudgetError};
use crate::operation_checkpoint::{self, CheckpointError};
use crate::postgres_ingest::{self, ImportError};
use crate::postgres_repository::{self, RepositoryError};
use crate::presentation_package::{self, PresentationError};
use crate::purpose_history::{self, NewPurpose, PurposeError};
use crate::question_refs::{self, ReferenceError};
use crate::reanalysis_revision;
use crate::resource_diagnostics;
use crate::review_governance::{self, HandoffReceipt, ReviewGovernanceError};
use crate::situation_forensic_export::{self, ForensicExportError, ForensicSituationExport};
use crate::situation_store::{self, ContributionKind, SituationStoreError};
use crate::source_catalog;
use crate::source_import::SourceFormat;
use crate::source_scope::{self, SourceScopeError, SourceScopeInput};
use crate::temporal_state::{self, TemporalAssertionInput, TemporalStateError};
use crate::upstream_processing::{self, NewProcessingDeclaration, UpstreamProcessingError};
use crate::{
    adversarial_dossier, agentic_controller, agentic_policy, bff_dashboard, bff_epistemic,
    bff_executor, reasoning_handoff, reasoning_packet,
};
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

const FORENSIC_IMPORT_HTTP_BODY_LIMIT: usize = 192 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub model: Arc<dyn ReasoningModel>,
    pub local_handoff: LocalHandoff,
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

fn unique_header_text<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ApiError> {
    let mut values = headers.get_all(name).iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "ambiguous source locator",
        ));
    }
    first
        .to_str()
        .map(Some)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid source locator"))
}

fn unique_operational_event_id(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    let mut values = headers.get_all("x-operational-event-id").iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "ambiguous operational event id header",
        ));
    }
    first.to_str().map(Some).map_err(|_| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "invalid operational event id header",
        )
    })
}

fn unsafe_locator_character(value: char) -> bool {
    value.is_control()
        || matches!(
            value,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

fn decode_percent_encoded_locator(encoded: &str) -> Result<String, ApiError> {
    if encoded.is_empty() || encoded.len() > 3 * 1024 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"));
    }
    let raw = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(raw.len().min(1024));
    let mut index = 0;
    while index < raw.len() {
        let byte = if raw[index] == b'%' {
            if index + 2 >= raw.len() {
                return Err(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"));
            }
            let hex = |value: u8| -> Option<u8> {
                match value {
                    b'0'..=b'9' => Some(value - b'0'),
                    b'a'..=b'f' => Some(value - b'a' + 10),
                    b'A'..=b'F' => Some(value - b'A' + 10),
                    _ => None,
                }
            };
            let high = hex(raw[index + 1])
                .ok_or(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"))?;
            let low = hex(raw[index + 2])
                .ok_or(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"))?;
            index += 3;
            (high << 4) | low
        } else {
            let value = raw[index];
            if !value.is_ascii() {
                return Err(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"));
            }
            index += 1;
            value
        };
        decoded.push(byte);
        if decoded.len() > 1024 {
            return Err(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"));
        }
    }
    let locator = String::from_utf8(decoded)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid source locator"))?;
    if locator.is_empty() || locator.chars().any(unsafe_locator_character) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"));
    }
    Ok(locator)
}

fn source_locator(headers: &HeaderMap) -> Result<String, ApiError> {
    let encoded = unique_header_text(headers, "x-source-locator-percent-encoded")?;
    let legacy = unique_header_text(headers, "x-source-locator")?;
    match (encoded, legacy) {
        (Some(_), Some(_)) => Err(ApiError(
            StatusCode::BAD_REQUEST,
            "ambiguous source locator",
        )),
        (Some(value), None) => decode_percent_encoded_locator(value),
        (None, Some(value)) => {
            if value.is_empty() || value.len() > 1024 || value.chars().any(unsafe_locator_character)
            {
                Err(ApiError(StatusCode::BAD_REQUEST, "invalid source locator"))
            } else {
                Ok(value.to_owned())
            }
        }
        (None, None) => Err(ApiError(StatusCode::BAD_REQUEST, "missing source locator")),
    }
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

// The React build generates deterministic module chunks. Serve only the
// explicitly embedded assets; never interpret URL paths as filesystem paths.
async fn frontend_technical_chunk(Path(name): Path<String>) -> Response {
    let body: Option<&'static str> = match name.as_str() {
        "AnalysisPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/AnalysisPage.js"
        )),
        "InspectorPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/InspectorPage.js"
        )),
        "ReasoningPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/ReasoningPage.js"
        )),
        "IdentitiesPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/IdentitiesPage.js"
        )),
        "FeedbackPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/FeedbackPage.js"
        )),
        "EpistemicPlaygroundPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/EpistemicPlaygroundPage.js"
        )),
        "ForensicPage.js" => Some(include_str!(
            "../frontend/dist/assets/chunks/ForensicPage.js"
        )),
        _ => None,
    };
    match body {
        Some(source) => (
            [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
            source,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub fn router(pool: PgPool) -> Router {
    router_with_model(pool, Arc::new(UnavailableModel))
}

pub fn router_with_model(pool: PgPool, model: Arc<dyn ReasoningModel>) -> Router {
    router_with_model_and_handoff(pool, model, LocalHandoff::from_env())
}

fn router_with_model_and_handoff(
    pool: PgPool,
    model: Arc<dyn ReasoningModel>,
    local_handoff: LocalHandoff,
) -> Router {
    Router::new()
        .route(
            "/",
            get(frontend_index),
        )
        .route("/analysis", get(frontend_index))
        .route("/reasoning", get(frontend_index))
        .route("/feedback", get(frontend_index))
        .route("/identities", get(frontend_index))
        .route("/operations", get(frontend_index))
        .route("/review", get(frontend_index))
        .route("/inspector", get(frontend_index))
        .route("/forensic", get(frontend_index))
        .route("/playground", get(frontend_index))
        .route("/assets/chunks/{name}", get(frontend_technical_chunk))
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
        .route("/v1/local-handoff", post(consume_local_handoff))
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/readyz", get(readiness))
        .route(
            "/v1/workspaces/{workspace}/actors",
            get(list_actors),
        )
        .route(
            "/v1/workspaces/{workspace}/actors/{actor_id}",
            post(create_actor),
        )
        .route(
            "/v1/workspaces/{workspace}/actors/{actor_id}/kind-events/{event}",
            post(record_actor_kind),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/actor-bindings/{event}",
            post(record_actor_binding),
        )
        .route(
            "/v1/workspaces/{workspace}/identities",
            get(list_identity_subjects),
        )
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
        .route("/v1/workspaces/{workspace}/epistemic/preview", post(preview_epistemic_balance))
        .route("/v1/workspaces/{workspace}/epistemic/compare", post(compare_epistemic_policies))
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/epistemic/evaluate",
            post(evaluate_grounded_claim_weights),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/epistemic/records/{score}",
            post(record_grounded_claim_score),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/epistemic/records",
            get(list_grounded_claim_scores),
        )
        .route("/v1/workspaces/{workspace}/epistemic/policy", get(get_effective_epistemic_policy))
        .route("/v1/workspaces/{workspace}/epistemic/policies/{policy}", post(record_epistemic_policy))
        .route("/v1/workspaces/{workspace}/leads", post(propose_leads))
        .route("/v1/workspaces/{workspace}/sources", get(list_sources))
        .route(
            "/v1/workspaces/{workspace}/integration-diagnostics",
            get(get_integration_diagnostics),
        )
        .route(
            "/v1/workspaces/{workspace}/runtime-resources",
            get(get_runtime_resources),
        )
        .route(
            "/v1/workspaces/{workspace}/integration-processing-declarations",
            get(list_upstream_processing_declarations),
        )
        .route(
            "/v1/workspaces/{workspace}/integration-processing-declarations/{declaration}",
            post(record_upstream_processing_declaration),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/speakers",
            get(list_source_speakers),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/original",
            get(download_source_original),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/intake-receipt-package",
            get(get_intake_receipt_package),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/git-publication-preview",
            get(get_intake_git_publication_preview),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/git-publication-intents/{publication}",
            post(record_intake_git_publication_intent),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/git-publication-intents/{publication}/remote-status",
            get(get_intake_git_publication_remote_status),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/git-publication-intents/{publication}/remote-events",
            get(list_intake_git_publication_remote_events),
        )
        .route(
            "/v1/workspaces/{workspace}/sources/{document}/git-publication-intents/{publication}/publish",
            post(publish_intake_git_publication_intent),
        )
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
            "/v1/workspaces/{workspace}/situations/{situation}/purpose",
            get(list_purpose_history),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/purpose/{event}",
            post(record_purpose),
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
            "/v1/workspaces/{workspace}/situations/{situation}/forensic-export",
            get(get_situation_forensic_export),
        )
        .route(
            "/v1/workspaces/{workspace}/forensic-imports",
            get(list_forensic_import_batches),
        )
        .route(
            "/v1/workspaces/{workspace}/forensic-imports/{batch}",
            get(get_forensic_import_batch)
                .post(import_forensic_package)
                .layer(DefaultBodyLimit::max(FORENSIC_IMPORT_HTTP_BODY_LIMIT)),
        )
        .route(
            "/v1/workspaces/{workspace}/forensic-imports/{batch}/items",
            get(list_forensic_import_items),
        )
        .route(
            "/v1/workspaces/{workspace}/forensic-imports/{batch}/items/{item}",
            get(get_forensic_import_item),
        )
        .route(
            "/v1/workspaces/{workspace}/forensic-imports/{batch}/items/{item}/original",
            get(download_forensic_import_original),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/intents/{operation}/sync",
            post(bff_intent_sync),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/intents/{operation}/async",
            post(bff_intent_async),
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
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/budget",
            get(get_bff_operation_budget),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/events",
            get(get_bff_operation_events),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints",
            get(get_bff_operation_checkpoints),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/package",
            get(get_bff_operation_checkpoint_package),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/milestone",
            get(get_bff_operation_checkpoint_milestone),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/milestones/{milestone}",
            post(declare_bff_operation_checkpoint_milestone),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/git-publication-preview",
            get(get_bff_operation_checkpoint_git_publication_preview),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/git-publication-intents/{publication}",
            post(record_bff_operation_checkpoint_git_publication_intent),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/git-publication-intents/{publication}/remote-status",
            get(get_bff_operation_checkpoint_git_publication_remote_status),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/git-publication-intents/{publication}/remote-events",
            get(list_bff_operation_checkpoint_git_publication_remote_events),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/checkpoints/{checkpoint}/git-publication-intents/{publication}/publish",
            post(publish_bff_operation_checkpoint_git_publication_intent),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/wait",
            get(wait_bff_operation),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/bff/operations/{operation}/observe",
            get(observe_bff_operation),
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
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/presentation-package",
            get(get_presentation_package).post(post_presentation_package_with_examined_sources),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/presentation-package/source-preview",
            get(get_presentation_source_preview),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/presentation-package/export-events",
            get(list_presentation_export_events),
        )
        .route(
            "/v1/workspaces/{workspace}/git-publication-targets",
            get(list_git_publication_targets),
        )
        .route(
            "/v1/workspaces/{workspace}/git-publication-targets/{target}",
            post(register_git_publication_target),
        )
        .route(
            "/v1/workspaces/{workspace}/git-publication-targets/{target}/verify",
            post(verify_git_publication_target),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-preview",
            get(get_git_publication_preview),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents",
            get(list_git_publication_intents),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{publication}",
            post(record_git_publication_intent),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{publication}/remote-status",
            get(get_git_publication_remote_status),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{publication}/remote-events",
            get(list_git_publication_remote_events),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/git-publication-intents/{publication}/publish",
            post(publish_git_publication_intent),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/adversarial-dossier",
            get(get_adversarial_dossier),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/dashboard",
            get(get_bff_dashboard),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/epistemic",
            get(get_bff_epistemic),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/credence",
            get(list_human_credence),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/claims/{claim}/credence/{credence}",
            post(record_human_credence),
        )

        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/reasoning-input",
            get(get_reasoning_input_summary),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/handoff",
            get(get_reasoning_handoff),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/handoff/receipts",
            get(list_reasoning_handoff_receipts),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/bff/handoff/receipts/{receipt}",
            post(record_reasoning_handoff_receipt),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/observations/search",
            get(search_frozen_observations),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/input",
            get(get_analysis_input),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/forensic-context",
            get(list_analysis_forensic_context),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/forensic-context/{binding}",
            post(bind_analysis_forensic_context),
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
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/reanalysis-revision",
            get(get_reanalysis_revision),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/used-contributions/{contribution}",
            post(record_used_contribution),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/decision-points",
            get(list_decision_points),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/decision-points/{decision}",
            post(create_decision_point),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/decision-points/{decision}/options/{option}",
            post(add_decision_option),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/decision-points/{decision}/selections/{event}",
            post(record_decision_selection),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/decision-readiness",
            get(list_decision_readiness),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/decision-points/{decision}/readiness/{assessment}",
            post(record_decision_readiness),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/integrity-scope",
            get(list_integrity_scope),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/integrity-scope/{document}/{assessment}",
            post(record_integrity_scope),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/collection-provenance",
            get(list_collection_provenance),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/collection-provenance/{event}",
            post(record_collection_provenance),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/source-scope",
            get(list_source_scope),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/source-scope/{scope_id}",
            post(record_source_scope),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/temporal-state",
            get(list_temporal_state),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/temporal-state/{assertion}",
            post(record_temporal_state),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/hypothesis-portfolio",
            get(get_hypothesis_portfolio),
        )
        .route(
            "/v1/workspaces/{workspace}/situations/{situation}/analysis-snapshots/{analysis}/hypothesis-portfolio/{interpretation}/events/{event}",
            post(record_hypothesis_portfolio_event),
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
        .with_state(AppState {
            pool,
            model,
            local_handoff,
        })
        .layer(axum::middleware::from_fn(request_digest_context))
        .layer(axum::middleware::from_fn(response_security_headers))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalHandoffRequest {
    code: String,
}

fn local_handoff_origin_allowed(headers: &HeaderMap) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let Some(origin) = origins.next() else {
        // Non-browser local clients such as the deterministic smoke test do not
        // send Origin. The capability itself remains required and one-shot.
        return true;
    };
    if origins.next().is_some() {
        return false;
    }
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let mut hosts = headers.get_all(header::HOST).iter();
    let Some(host) = hosts.next() else {
        return false;
    };
    if hosts.next().is_some() {
        return false;
    }
    let Ok(host) = host.to_str() else {
        return false;
    };
    let Some(port) = host.strip_prefix("127.0.0.1:") else {
        return false;
    };
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    origin == format!("http://{host}")
}

async fn consume_local_handoff(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LocalHandoffRequest>,
) -> Result<Response, ApiError> {
    if !local_handoff_origin_allowed(&headers) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "local handoff origin rejected",
        ));
    }
    match state.local_handoff.consume(&request.code) {
        Ok(token) => Ok(no_store(json!({"token": token}), StatusCode::OK)),
        Err(LocalHandoffError::NotFound) => {
            Err(ApiError(StatusCode::NOT_FOUND, "local handoff unavailable"))
        }
        Err(LocalHandoffError::SecretUnavailable) => Err(ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "local handoff secret unavailable",
        )),
    }
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
fn import_error_for_format(error: ImportError, format: SourceFormat) -> ApiError {
    match error {
        ImportError::Parse(_) => match format {
            SourceFormat::Eml => ApiError(
                StatusCode::BAD_REQUEST,
                "invalid or unsupported plain-text email source",
            ),
            SourceFormat::WhatsappExport => {
                ApiError(StatusCode::BAD_REQUEST, "invalid WhatsApp chat export")
            }
        },
        ImportError::InvalidIdentity
        | ImportError::Locator
        | ImportError::OperationalEventInvalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid source metadata")
        }
        ImportError::DocumentConflict => ApiError(StatusCode::CONFLICT, "source document conflict"),
        ImportError::OperationalEventConflict => {
            ApiError(StatusCode::CONFLICT, "operational event identity conflict")
        }
        ImportError::WorkspaceInvisible => ApiError(StatusCode::NOT_FOUND, "workspace unavailable"),
        _ => ApiError(StatusCode::INTERNAL_SERVER_ERROR, "source unavailable"),
    }
}

impl From<ImportError> for ApiError {
    fn from(e: ImportError) -> Self {
        match e {
            ImportError::Parse(_)
            | ImportError::InvalidIdentity
            | ImportError::Locator
            | ImportError::OperationalEventInvalid => {
                Self(StatusCode::BAD_REQUEST, "invalid source")
            }
            ImportError::DocumentConflict => Self(StatusCode::CONFLICT, "source document conflict"),
            ImportError::OperationalEventConflict => {
                Self(StatusCode::CONFLICT, "operational event identity conflict")
            }
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
              (has_table_privilege('niccolo_api',c.oid,'SELECT') AND NOT EXISTS ( \
                 SELECT 1 FROM pg_policy p WHERE p.polrelid=c.oid \
                   AND NOT p.polpermissive AND p.polname='api_read_gate' AND p.polcmd='r' \
                   AND 'niccolo_api'::regrole::oid=ANY(p.polroles))) OR \
              (has_table_privilege('niccolo_api',c.oid,'INSERT') AND NOT EXISTS ( \
                 SELECT 1 FROM pg_policy p WHERE p.polrelid=c.oid \
                   AND NOT p.polpermissive AND p.polname='api_write_insert_gate' AND p.polcmd='a' \
                   AND 'niccolo_api'::regrole::oid=ANY(p.polroles))) OR \
              (has_table_privilege('niccolo_api',c.oid,'UPDATE') AND NOT EXISTS ( \
                 SELECT 1 FROM pg_policy p WHERE p.polrelid=c.oid \
                   AND NOT p.polpermissive AND p.polname='api_write_update_gate' AND p.polcmd='w' \
                   AND 'niccolo_api'::regrole::oid=ANY(p.polroles))) OR \
              (has_table_privilege('niccolo_api',c.oid,'DELETE') AND NOT EXISTS ( \
                 SELECT 1 FROM pg_policy p WHERE p.polrelid=c.oid \
                   AND NOT p.polpermissive AND p.polname='api_write_delete_gate' AND p.polcmd='d' \
                   AND 'niccolo_api'::regrole::oid=ANY(p.polroles))))) \
         FROM pg_roles r WHERE r.rolname=current_user",
    )
    .fetch_one(&state.pool)
    .await;
    if matches!(check, Ok((true, true, true, true, true, true))) {
        no_store(
            json!({
                "status":"ready",
                "model_configured":state.model.provider() != "unconfigured"
            }),
            StatusCode::OK,
        )
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamProcessingRequest {
    processing_kind: String,
    scope_locator: String,
    declaration_text: String,
    source_document_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamProcessingQuery {
    after: Option<i64>,
    limit: Option<i64>,
}

async fn record_upstream_processing_declaration(
    State(state): State<AppState>,
    Path((workspace, declaration)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<UpstreamProcessingRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let recorded = upstream_processing::record(
        &state.pool,
        workspace,
        declaration,
        actor,
        NewProcessingDeclaration {
            processing_kind: &body.processing_kind,
            scope_locator: &body.scope_locator,
            declaration_text: &body.declaration_text,
            source_document_id: body.source_document_id,
        },
    )
    .await
    .map_err(|error| match error {
        UpstreamProcessingError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid upstream processing declaration",
        ),
        UpstreamProcessingError::InvisibleDocument => {
            ApiError(StatusCode::NOT_FOUND, "source document unavailable")
        }
        UpstreamProcessingError::Conflict => ApiError(
            StatusCode::CONFLICT,
            "upstream processing declaration conflict",
        ),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "upstream processing declarations unavailable",
        ),
    })?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_upstream_processing_declarations(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<UpstreamProcessingQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let declarations = upstream_processing::history(
        &state.pool,
        workspace,
        query.after.unwrap_or(0),
        query.limit.unwrap_or(100),
    )
    .await
    .map_err(|error| match error {
        UpstreamProcessingError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid upstream processing declaration query",
        ),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "upstream processing declarations unavailable",
        ),
    })?;
    let next_after = declarations.last().map(|item| item.event_no);
    Ok(no_store(
        json!({
            "declarations": declarations,
            "next_after": next_after,
            "declarations_are_verified": false,
            "declarations_are_evidence": false
        }),
        StatusCode::OK,
    ))
}

async fn get_runtime_resources(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let diagnostic = resource_diagnostics::current().map_err(|_| {
        ApiError(
            StatusCode::SERVICE_UNAVAILABLE,
            "runtime resource diagnostic unavailable",
        )
    })?;
    Ok(no_store(json!(diagnostic), StatusCode::OK))
}

async fn get_integration_diagnostics(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let diagnostics = integration_diagnostics::summarize(&state.pool, workspace)
        .await
        .map_err(|error| match error {
            IntegrationDiagnosticsError::Privileged => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "integration diagnostics unavailable",
            ),
            IntegrationDiagnosticsError::Database(_) => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "integration diagnostics unavailable",
            ),
        })?;
    Ok(no_store(json!(diagnostics), StatusCode::OK))
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
                "sha256":d.sha256,"byte_length":d.byte_length,"fragment_count":d.fragment_count,
                "original_bytes_available":d.original_bytes_available
            })).collect::<Vec<_>>(),"next_after":page.next_after
        }),
        StatusCode::OK,
    ))
}

fn intake_git_publication_error(error: IntakeGitPublicationError) -> ApiError {
    match error {
        IntakeGitPublicationError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid intake git publication request",
        ),
        IntakeGitPublicationError::Conflict => ApiError(
            StatusCode::CONFLICT,
            "intake git publication identity conflict",
        ),
        IntakeGitPublicationError::Invisible => ApiError(
            StatusCode::NOT_FOUND,
            "intake git publication intent unavailable",
        ),
        IntakeGitPublicationError::Stale => ApiError(
            StatusCode::CONFLICT,
            "intake git publication intent is stale; preview and record a new intent",
        ),
        IntakeGitPublicationError::Source(source_catalog::CatalogError::InvisibleDocument) => {
            ApiError(StatusCode::NOT_FOUND, "source unavailable")
        }
        IntakeGitPublicationError::Git(GitPublicationError::Invisible) => {
            ApiError(StatusCode::NOT_FOUND, "git publication target unavailable")
        }
        IntakeGitPublicationError::Package(intake_package::IntakePackageError::Invalid)
        | IntakeGitPublicationError::Git(GitPublicationError::Invalid) => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid intake git publication request",
        ),
        IntakeGitPublicationError::Package(intake_package::IntakePackageError::TooLarge) => {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "intake receipt exceeds publication limit",
            )
        }
        IntakeGitPublicationError::Git(GitPublicationError::Conflict)
        | IntakeGitPublicationError::Git(GitPublicationError::Stale) => ApiError(
            StatusCode::CONFLICT,
            "intake git publication source is inconsistent",
        ),
        IntakeGitPublicationError::Database(_)
        | IntakeGitPublicationError::Privileged
        | IntakeGitPublicationError::Source(_)
        | IntakeGitPublicationError::Package(_)
        | IntakeGitPublicationError::Git(_) => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "intake git publication planning unavailable",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntakeGitPublicationPreviewQuery {
    target_id: Uuid,
}

async fn get_intake_git_publication_preview(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<IntakeGitPublicationPreviewQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let preview =
        intake_git_publication::preview(&state.pool, workspace, document, query.target_id)
            .await
            .map_err(intake_git_publication_error)?;
    Ok(no_store(json!(preview), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntakeGitPublicationIntentRequest {
    target_id: Uuid,
}

async fn record_intake_git_publication_intent(
    State(state): State<AppState>,
    Path((workspace, document, publication)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<IntakeGitPublicationIntentRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let (intent, replayed) = intake_git_publication::record_intent(
        &state.pool,
        workspace,
        document,
        actor,
        publication,
        request.target_id,
    )
    .await
    .map_err(intake_git_publication_error)?;
    Ok(no_store(
        json!({
            "intent": intent,
            "replayed": replayed,
            "remote_publish_performed": false,
            "requires_provider_authorization_before_publish": true,
            "source_state_preserved": "RECEIVED",
            "establishes_authenticity": false,
            "establishes_truth": false,
            "proves_completeness": false
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn get_intake_git_publication_remote_status(
    State(state): State<AppState>,
    Path((workspace, document, publication)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let status =
        intake_git_publication::remote_status(&state.pool, workspace, document, publication)
            .await
            .map_err(intake_git_publication_error)?;
    Ok(no_store(json!(status), StatusCode::OK))
}

async fn list_intake_git_publication_remote_events(
    State(state): State<AppState>,
    Path((workspace, document, publication)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<GitPublicationRemoteEventListQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let events = intake_git_publication::list_remote_events(
        &state.pool,
        workspace,
        document,
        publication,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(intake_git_publication_error)?;
    Ok(no_store(
        json!({
            "events": events,
            "provider_secret_stored": false,
            "event_payloads_include_provider_response_bodies": false,
            "source_state_preserved": "RECEIVED"
        }),
        StatusCode::OK,
    ))
}

async fn publish_intake_git_publication_intent(
    State(state): State<AppState>,
    Path((workspace, document, publication)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<GitHubPublishRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    if request.attempt_id.is_nil() || request.confirmation != "PUBLISH_INTAKE_RECEIPT_TO_GITHUB" {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "explicit intake GitHub publication confirmation required",
        ));
    }
    let existing =
        intake_git_publication::remote_status(&state.pool, workspace, document, publication)
            .await
            .map_err(intake_git_publication_error)?;
    if existing.remote_publish_verified {
        return Ok(no_store(
            json!({
                "status": existing,
                "replayed_verified_publication": true,
                "provider_secret_stored": false,
                "source_state_preserved": "RECEIVED"
            }),
            StatusCode::OK,
        ));
    }

    let provider_token = transient_github_token(&headers)?;
    let material =
        intake_git_publication::publication_material(&state.pool, workspace, document, publication)
            .await
            .map_err(intake_git_publication_error)?;
    let publish_result = tokio::task::spawn_blocking(move || {
        let transport = github_publisher::CurlGitHubTransport::default();
        github_publisher::publish_intake_package(
            &transport,
            &material.target,
            &material.intent,
            &material.package,
            &provider_token,
        )
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::BAD_GATEWAY,
            "GitHub intake publication task failed",
        )
    })?;

    let verified = match publish_result {
        Ok(verified) => verified,
        Err(error) => {
            let failure_class = match error {
                GitHubPublishError::LocalIntegrity | GitHubPublishError::Invalid => {
                    "LOCAL_INTEGRITY"
                }
                GitHubPublishError::Transport => "TRANSPORT",
                GitHubPublishError::ProviderRejected(_) => "PROVIDER_REJECTED",
                GitHubPublishError::VerificationMismatch
                | GitHubPublishError::InvalidProviderResponse => "VERIFICATION_MISMATCH",
            };
            let _ = intake_git_publication::record_remote_event(
                &state.pool,
                workspace,
                actor,
                &git_publication::RemoteEventInput {
                    event_id: request.attempt_id,
                    publication_id: publication,
                    kind: git_publication::RemoteEventKind::PublishFailed,
                    remote_commit_sha: None,
                    observed_ref: None,
                    failure_class: Some(failure_class.into()),
                },
            )
            .await;
            return Err(github_publish_error(error));
        }
    };

    for (kind, observed_ref) in [
        (git_publication::RemoteEventKind::PushAccepted, None),
        (
            git_publication::RemoteEventKind::RefVerified,
            Some(verified.ref_name.clone()),
        ),
    ] {
        intake_git_publication::record_remote_event(
            &state.pool,
            workspace,
            actor,
            &git_publication::RemoteEventInput {
                event_id: request.attempt_id,
                publication_id: publication,
                kind,
                remote_commit_sha: Some(verified.commit_sha.clone()),
                observed_ref,
                failure_class: None,
            },
        )
        .await
        .map_err(intake_git_publication_error)?;
    }

    let status =
        intake_git_publication::remote_status(&state.pool, workspace, document, publication)
            .await
            .map_err(intake_git_publication_error)?;
    if !status.remote_publish_verified
        || status.verified_commit_sha.as_deref() != Some(verified.commit_sha.as_str())
        || status.verified_ref.as_deref() != Some(verified.ref_name.as_str())
    {
        return Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "remote intake publication verification ledger mismatch",
        ));
    }
    Ok(no_store(
        json!({
            "status": status,
            "repository_full_name": verified.repository_full_name,
            "package_sha256": verified.package_sha256,
            "remote_commit_already_present": verified.already_present,
            "provider_secret_stored": false,
            "remote_publish_verified": true,
            "source_state_preserved": "RECEIVED",
            "establishes_authenticity": false,
            "establishes_truth": false,
            "proves_completeness": false
        }),
        StatusCode::OK,
    ))
}

async fn get_intake_receipt_package(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let source = source_catalog::get_source(&state.pool, workspace, document)
        .await
        .map_err(|error| match error {
            source_catalog::CatalogError::InvisibleDocument => {
                ApiError(StatusCode::NOT_FOUND, "source unavailable")
            }
            _ => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "intake receipt unavailable",
            ),
        })?;
    let package = intake_package::build(workspace, &source).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "intake receipt unavailable",
        )
    })?;
    let package_sha256 = intake_package::package_sha256(&package);
    Ok(no_store(
        json!({
            "receipt": package.receipt,
            "files": package.files,
            "package_sha256": package_sha256,
            "remote_publication_performed": false,
            "establishes_truth": false,
            "establishes_authenticity": false,
        }),
        StatusCode::OK,
    ))
}

async fn download_source_original(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (format, original) = source_catalog::source_original(&state.pool, workspace, document)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "source download unavailable",
            )
        })?
        .ok_or(ApiError(
            StatusCode::NOT_FOUND,
            "source original unavailable",
        ))?;
    let extension = if format == "EML" { "eml" } else { "txt" };
    let content_type = if format == "EML" {
        "message/rfc822"
    } else {
        "text/plain; charset=utf-8"
    };
    let filename = format!("attachment; filename=\"niccolo-source-{document}.{extension}\"");
    let mut response = Bytes::from(original).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type.parse().unwrap());
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, filename.parse().unwrap());
    Ok(response)
}

async fn list_source_speakers(
    State(state): State<AppState>,
    Path((workspace, document)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let speakers = source_catalog::document_speakers(&state.pool, workspace, document)
        .await
        .map_err(|error| match error {
            source_catalog::CatalogError::InvisibleDocument => {
                ApiError(StatusCode::NOT_FOUND, "source unavailable")
            }
            source_catalog::CatalogError::TooManySpeakers => ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "speaker label limit exceeded",
            ),
            _ => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "source speakers unavailable",
            ),
        })?;
    Ok(no_store(
        json!({
            "document_id": document,
            "speakers": speakers.iter().map(|speaker| json!({
                "speaker_label": speaker.speaker_label,
                "fragment_count": speaker.fragment_count,
            })).collect::<Vec<_>>(),
            "speaker_labels_are_verified_identities": false,
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
        .and_then(|value| value.split(';').next())
        .map(str::trim)
    {
        Some(media_type) if media_type.eq_ignore_ascii_case("message/rfc822") => SourceFormat::Eml,
        Some(media_type) if media_type.eq_ignore_ascii_case("text/plain") => {
            SourceFormat::WhatsappExport
        }
        _ => {
            return Err(ApiError(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected message/rfc822 or text/plain",
            ));
        }
    };
    let locator = source_locator(&headers)?;
    let operational_event_id = unique_operational_event_id(&headers)?;
    let result = postgres_ingest::ingest_source_with_operational_id(
        &state.pool,
        workspace,
        document,
        format,
        &locator,
        &body,
        operational_event_id,
    )
    .await
    .map_err(|error| import_error_for_format(error, format))?;
    let warning = result
        .operational_duplicate
        .then_some("duplicate operational delivery suppressed; deduplicate retries upstream");
    Ok(no_store(
        json!({
            "workspace_id":workspace,
            "document_id":result.document_id,
            "sha256":result.sha256,
            "fragment_ids":result.fragment_ids,
            "replayed":result.replayed,
            "revision":result.graph_revision,
            "operational_event_id":result.operational_event_id,
            "operational_duplicate":result.operational_duplicate,
            "delivery_count":result.delivery_count,
            "integration_warning":warning,
            "upstream_deduplication_preferred":true
        }),
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
                "imported_context_count": entry.imported_context_count,
                "current_situation_revision": entry.current_revision,
                "input_is_current": entry.input_is_current,
                "historical_output_preserved": entry.answer_text.is_some()
            })).collect::<Vec<_>>()
        }),
        StatusCode::OK,
    ))
}

async fn get_situation_forensic_export(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let package = situation_forensic_export::build(&state.pool, workspace, situation)
        .await
        .map_err(|error| match error {
            ForensicExportError::Invisible => {
                ApiError(StatusCode::NOT_FOUND, "situation unavailable")
            }
            ForensicExportError::TooLarge(_) => ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "forensic export exceeds local package limits",
            ),
            ForensicExportError::CorruptOriginal => ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "retained source payload failed integrity validation",
            ),
            _ => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "forensic export unavailable",
            ),
        })?;
    Ok(no_store(json!(package), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForensicImportHttpRequest {
    declared_origin: String,
    package: ForensicSituationExport,
}

fn forensic_import_error(error: ForensicImportError) -> ApiError {
    match error {
        ForensicImportError::Invalid | ForensicImportError::ManifestIncomplete => {
            ApiError(StatusCode::BAD_REQUEST, "invalid forensic import package")
        }
        ForensicImportError::Conflict => {
            ApiError(StatusCode::CONFLICT, "forensic import provenance conflict")
        }
        ForensicImportError::Invisible => {
            ApiError(StatusCode::NOT_FOUND, "forensic import unavailable")
        }
        ForensicImportError::Structure(ForensicExportError::TooLarge(_)) => ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "forensic import exceeds local package limits",
        ),
        ForensicImportError::Structure(
            ForensicExportError::CorruptOriginal
            | ForensicExportError::ManifestMismatch
            | ForensicExportError::InvalidPackage(_)
            | ForensicExportError::InvalidCatalogIdentifier,
        ) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "forensic import failed structural verification",
        ),
        ForensicImportError::Structure(_) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "forensic import package unavailable",
        ),
        ForensicImportError::Privileged
        | ForensicImportError::InsecureRole
        | ForensicImportError::Database(_) => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "forensic import unavailable",
        ),
    }
}

async fn import_forensic_package(
    State(state): State<AppState>,
    Path((workspace, batch)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<ForensicImportHttpRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let input = forensic_import::ImportRequest {
        import_batch_id: batch,
        declared_origin: request.declared_origin,
    };
    let imported =
        forensic_import::import_untrusted(&state.pool, workspace, actor, &input, &request.package)
            .await
            .map_err(forensic_import_error)?;
    let status = if imported.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(no_store(json!(imported), status))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForensicImportBatchesQuery {
    before: Option<Uuid>,
    limit: Option<i64>,
}

async fn list_forensic_import_batches(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Query(query): Query<ForensicImportBatchesQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = forensic_import::list_batches(
        &state.pool,
        workspace,
        query.before,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(forensic_import_error)?;
    Ok(no_store(json!(page), StatusCode::OK))
}

async fn get_forensic_import_batch(
    State(state): State<AppState>,
    Path((workspace, batch)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = forensic_import::get_batch(&state.pool, workspace, batch)
        .await
        .map_err(forensic_import_error)?;
    Ok(no_store(json!(view), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForensicImportItemsQuery {
    after: Option<String>,
    limit: Option<i64>,
}

async fn list_forensic_import_items(
    State(state): State<AppState>,
    Path((workspace, batch)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<ForensicImportItemsQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = forensic_import::list_items(
        &state.pool,
        workspace,
        batch,
        query.after.as_deref(),
        query.limit.unwrap_or(50),
    )
    .await
    .map_err(forensic_import_error)?;
    Ok(no_store(json!(page), StatusCode::OK))
}

async fn get_forensic_import_item(
    State(state): State<AppState>,
    Path((workspace, batch, item)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let detail = forensic_import::get_item(&state.pool, workspace, batch, item)
        .await
        .map_err(forensic_import_error)?;
    Ok(no_store(json!(detail), StatusCode::OK))
}

async fn download_forensic_import_original(
    State(state): State<AppState>,
    Path((workspace, batch, item)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let retained = forensic_import::get_original_bytes(&state.pool, workspace, batch, item)
        .await
        .map_err(forensic_import_error)?;
    let filename = format!(
        "attachment; filename=\"niccolo-quarantine-source-{}.bin\"",
        retained.original_document_id
    );
    let mut response = Bytes::from(retained.bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        "application/octet-stream".parse().unwrap(),
    );
    headers.insert(header::CONTENT_DISPOSITION, filename.parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-niccolo-quarantine", "true".parse().unwrap());
    headers.insert("x-niccolo-documentary-evidence", "false".parse().unwrap());
    headers.insert(
        "x-niccolo-source-sha256",
        retained.source_sha256.parse().unwrap(),
    );
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationSearchQuery {
    q: String,
    limit: Option<i64>,
}

async fn search_frozen_observations(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    Query(query): Query<ObservationSearchQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let page =
        observation_retrieval::search(&state.pool, scope, &query.q, query.limit.unwrap_or(10))
            .await
            .map_err(|error| match error {
                ObservationSearchError::Invalid => {
                    ApiError(StatusCode::BAD_REQUEST, "invalid observation search")
                }
                ObservationSearchError::Invisible => {
                    ApiError(StatusCode::NOT_FOUND, "analysis snapshot unavailable")
                }
                ObservationSearchError::Privileged | ObservationSearchError::Database(_) => {
                    ApiError(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "observation search unavailable",
                    )
                }
            })?;
    Ok(no_store(json!(page), StatusCode::OK))
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
        "identity_event_no_max": snapshot.identity_event_no_max,
        "actor_kind_event_no_max": snapshot.actor_kind_event_no_max,
        "actor_binding_event_no_max": snapshot.actor_binding_event_no_max,
        "purpose_event_no_max": snapshot.purpose_event_no_max,
        "declared_purpose": snapshot.declared_purpose,
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

fn forensic_context_error(error: ForensicContextError) -> ApiError {
    match error {
        ForensicContextError::Invalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid forensic context binding")
        }
        ForensicContextError::Invisible => {
            ApiError(StatusCode::NOT_FOUND, "forensic context unavailable")
        }
        ForensicContextError::Conflict => {
            ApiError(StatusCode::CONFLICT, "forensic context binding conflict")
        }
        ForensicContextError::TooMany => ApiError(
            StatusCode::CONFLICT,
            "forensic context binding limit reached",
        ),
        ForensicContextError::Privileged | ForensicContextError::Database(_) => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "forensic context unavailable",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForensicContextBindingRequest {
    import_batch_id: Uuid,
    item_id: Uuid,
    note: Option<String>,
}

async fn bind_analysis_forensic_context(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, binding)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<ForensicContextBindingRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let result = forensic_context::bind(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
        actor,
        &forensic_context::BindingInput {
            binding_id: binding,
            import_batch_id: request.import_batch_id,
            item_id: request.item_id,
            note: request.note,
        },
    )
    .await
    .map_err(forensic_context_error)?;
    Ok(no_store(
        json!(result),
        if result.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_analysis_forensic_context(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let items = forensic_context::list(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(forensic_context_error)?;
    Ok(no_store(
        json!({
            "items": items,
            "quarantine_provenance": true,
            "items_are_documentary_evidence": false,
            "current_credentials_upgrade_imported_statements": false,
            "part_of_frozen_analysis_input": false
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
        "contribution_status": answer.contribution_status,
        "answer_is_currently_active": answer.contribution_status == "ACTIVE",
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

async fn get_reanalysis_revision(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = reanalysis_revision::explain(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(|error| match error {
        reanalysis_revision::RevisionError::Invisible => {
            ApiError(StatusCode::NOT_FOUND, "reanalysis revision not found")
        }
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reanalysis revision unavailable",
        ),
    })?;
    Ok(no_store(json!(view), StatusCode::OK))
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PurposeRequest {
    purpose_text: String,
    reason: String,
    downstream_use: Option<String>,
}

async fn record_purpose(
    State(state): State<AppState>,
    Path((workspace, situation, event)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<PurposeRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let recorded = purpose_history::record(
        &state.pool,
        workspace,
        situation,
        event,
        actor,
        NewPurpose {
            purpose_text: &body.purpose_text,
            reason: &body.reason,
            downstream_use: body.downstream_use.as_deref(),
        },
    )
    .await
    .map_err(|error| match error {
        PurposeError::Invalid => ApiError(StatusCode::BAD_REQUEST, "invalid purpose input"),
        PurposeError::Invisible => ApiError(StatusCode::NOT_FOUND, "situation unavailable"),
        PurposeError::Conflict => ApiError(StatusCode::CONFLICT, "purpose event conflict"),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "purpose history unavailable",
        ),
    })?;
    Ok(no_store(
        json!({
            "event_id":recorded.event_id,
            "event_no":recorded.event_no,
            "purpose_text":recorded.purpose_text,
            "reason":recorded.reason,
            "downstream_use":recorded.downstream_use,
            "recorded_by_principal_id":recorded.recorded_by,
            "replayed":recorded.replayed,
            "purpose_is_evidence":false
        }),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_purpose_history(
    State(state): State<AppState>,
    Path((workspace, situation)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let events = purpose_history::history(&state.pool, workspace, situation)
        .await
        .map_err(|error| match error {
            PurposeError::Invisible => ApiError(StatusCode::NOT_FOUND, "situation unavailable"),
            _ => ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "purpose history unavailable",
            ),
        })?;
    Ok(no_store(
        json!({
            "events":events,
            "purpose_is_evidence":false,
            "purpose_changes_rewrite_history":false
        }),
        StatusCode::OK,
    ))
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

impl From<DecisionError> for ApiError {
    fn from(error: DecisionError) -> Self {
        match error {
            DecisionError::Invalid => Self(StatusCode::BAD_REQUEST, "invalid decision input"),
            DecisionError::Conflict => Self(StatusCode::CONFLICT, "decision replay conflict"),
            DecisionError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decision runtime role unavailable",
            ),
            DecisionError::Database(sqlx::Error::Database(ref db))
                if matches!(db.code().as_deref(), Some("23503" | "23514" | "23505")) =>
            {
                Self(StatusCode::BAD_REQUEST, "invalid decision reference")
            }
            DecisionError::Database(_) => {
                Self(StatusCode::INTERNAL_SERVER_ERROR, "decision unavailable")
            }
        }
    }
}

impl From<DecisionReadinessError> for ApiError {
    fn from(error: DecisionReadinessError) -> Self {
        match error {
            DecisionReadinessError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid decision readiness")
            }
            DecisionReadinessError::Invisible => {
                Self(StatusCode::NOT_FOUND, "decision unavailable")
            }
            DecisionReadinessError::Conflict => {
                Self(StatusCode::CONFLICT, "decision readiness conflict")
            }
            DecisionReadinessError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decision readiness runtime role unavailable",
            ),
            DecisionReadinessError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "decision readiness unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionPointRequest {
    question: String,
    context_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionOptionRequest {
    ordinal: i32,
    text: String,
    tradeoff_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionSelectionRequest {
    kind: String,
    option_id: Option<Uuid>,
    note: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReversalConditionRequest {
    condition_id: Uuid,
    condition_text: String,
    impact: String,
    acquisition_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionReadinessRequest {
    subject_label: String,
    readiness: String,
    rationale: String,
    unresolved_unknowns: String,
    #[serde(default)]
    reversal_conditions: Vec<ReversalConditionRequest>,
}

async fn create_decision_point(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, decision)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<DecisionPointRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    decision_points::create(
        &state.pool,
        scope,
        decision,
        &input.question,
        input.context_note.as_deref(),
        actor,
    )
    .await?;
    Ok(no_store(
        json!({"decision_id":decision,"human_owns_decision":true}),
        StatusCode::CREATED,
    ))
}

async fn add_decision_option(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, decision, option)): Path<(Uuid, Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<DecisionOptionRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    decision_points::add_option(
        &state.pool,
        scope,
        decision,
        &DecisionOption {
            option_id: option,
            ordinal: input.ordinal,
            text: input.text,
            tradeoff_note: input.tradeoff_note,
        },
        actor,
    )
    .await?;
    Ok(no_store(
        json!({"decision_id":decision,"option_id":option,"options_are_automatically_ranked":false}),
        StatusCode::CREATED,
    ))
}

async fn record_decision_selection(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, decision, event)): Path<(Uuid, Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<DecisionSelectionRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    decision_points::append_selection(
        &state.pool,
        scope,
        decision,
        &SelectionEvent {
            event_id: event,
            kind: input.kind,
            option_id: input.option_id,
            note: input.note,
            recorded_by: actor,
        },
    )
    .await?;
    Ok(no_store(
        json!({"decision_id":decision,"event_id":event,"selection_means_executed":false}),
        StatusCode::CREATED,
    ))
}

async fn list_decision_points(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    Ok(no_store(
        json!(decision_points::package(&state.pool, scope).await?),
        StatusCode::OK,
    ))
}

async fn record_decision_readiness(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, decision, assessment)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Json(input): Json<DecisionReadinessRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let view = decision_readiness::record(
        &state.pool,
        scope,
        actor,
        &DecisionReadinessInput {
            assessment_id: assessment,
            decision_id: decision,
            subject_label: input.subject_label,
            readiness: input.readiness,
            rationale: input.rationale,
            unresolved_unknowns: input.unresolved_unknowns,
            reversal_conditions: input
                .reversal_conditions
                .into_iter()
                .map(|x| ReversalConditionInput {
                    condition_id: x.condition_id,
                    condition_text: x.condition_text,
                    impact: x.impact,
                    acquisition_note: x.acquisition_note,
                })
                .collect(),
        },
    )
    .await?;
    Ok(no_store(
        json!(view),
        if view.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_decision_readiness(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    Ok(no_store(
        json!({
            "decision_readiness": decision_readiness::latest(&state.pool, scope).await?,
            "readiness_claims_omniscience": false,
            "human_owns_decision": true
        }),
        StatusCode::OK,
    ))
}

impl From<IntegrityScopeError> for ApiError {
    fn from(error: IntegrityScopeError) -> Self {
        match error {
            IntegrityScopeError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid integrity assessment")
            }
            IntegrityScopeError::Invisible => {
                Self(StatusCode::NOT_FOUND, "integrity source unavailable")
            }
            IntegrityScopeError::Conflict => {
                Self(StatusCode::CONFLICT, "integrity assessment conflict")
            }
            IntegrityScopeError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "integrity runtime role unavailable",
            ),
            IntegrityScopeError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "integrity assessment unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrityScopeRequest {
    integrity_scope: String,
    authenticity: String,
    completeness: String,
    rationale: String,
    assertion_kind: String,
}

async fn record_integrity_scope(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, document, assessment)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Json(input): Json<IntegrityScopeRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let recorded = integrity_scope::record(
        &state.pool,
        scope,
        actor,
        &IntegrityAssessmentInput {
            assessment_id: assessment,
            document_id: document,
            integrity_scope: input.integrity_scope,
            authenticity: input.authenticity,
            completeness: input.completeness,
            rationale: input.rationale,
            assertion_kind: input.assertion_kind,
        },
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_integrity_scope(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    Ok(no_store(
        json!({
            "assessments": integrity_scope::list(&state.pool, scope).await?,
            "byte_continuity_establishes_authenticity": false,
            "byte_continuity_establishes_completeness": false
        }),
        StatusCode::OK,
    ))
}

impl From<CollectionProvenanceError> for ApiError {
    fn from(error: CollectionProvenanceError) -> Self {
        match error {
            CollectionProvenanceError::Invalid => Self(
                StatusCode::BAD_REQUEST,
                "invalid collection provenance event",
            ),
            CollectionProvenanceError::Invisible => Self(
                StatusCode::NOT_FOUND,
                "collection provenance reference unavailable",
            ),
            CollectionProvenanceError::Conflict => {
                Self(StatusCode::CONFLICT, "collection provenance conflict")
            }
            CollectionProvenanceError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "collection provenance runtime role unavailable",
            ),
            CollectionProvenanceError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "collection provenance unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionProvenanceRequest {
    actor_label: String,
    actor_role: String,
    operation: String,
    input_document_id: Option<Uuid>,
    input_evidence_id: Option<Uuid>,
    output_document_id: Option<Uuid>,
    output_evidence_id: Option<Uuid>,
    occurred_at: Option<String>,
    method: String,
    assertion_kind: String,
    qualification: String,
    note: String,
}

async fn record_collection_provenance(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, event)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<CollectionProvenanceRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let analysis_scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let recorded = collection_provenance::record(
        &state.pool,
        analysis_scope,
        actor,
        &CollectionEventInput {
            event_id: event,
            actor_label: input.actor_label,
            actor_role: input.actor_role,
            operation: input.operation,
            input_document_id: input.input_document_id,
            input_evidence_id: input.input_evidence_id,
            output_document_id: input.output_document_id,
            output_evidence_id: input.output_evidence_id,
            occurred_at: input.occurred_at,
            method: input.method,
            assertion_kind: input.assertion_kind,
            qualification: input.qualification,
            note: input.note,
        },
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_collection_provenance(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let analysis_scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let events = collection_provenance::list(&state.pool, analysis_scope).await?;
    Ok(no_store(
        json!({
            "events": events,
            "collector_is_original_author": false,
            "operator_is_epistemically_privileged": false,
            "collection_metadata_establishes_authenticity": false
        }),
        StatusCode::OK,
    ))
}

impl From<SourceScopeError> for ApiError {
    fn from(error: SourceScopeError) -> Self {
        match error {
            SourceScopeError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid source scope assertion")
            }
            SourceScopeError::Invisible => {
                Self(StatusCode::NOT_FOUND, "source scope reference unavailable")
            }
            SourceScopeError::Conflict => {
                Self(StatusCode::CONFLICT, "source scope assertion conflict")
            }
            SourceScopeError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "source scope runtime role unavailable",
            ),
            SourceScopeError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "source scope unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceScopeRequest {
    source_family: String,
    collection_status: String,
    scope_description: String,
    reason: String,
    collector_label: Option<String>,
    date_scope: Option<String>,
    assertion_kind: String,
    document_id: Option<Uuid>,
    supersedes_scope_id: Option<Uuid>,
}

async fn record_source_scope(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, scope_id)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<SourceScopeRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let analysis_scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let recorded = source_scope::record(
        &state.pool,
        analysis_scope,
        actor,
        &SourceScopeInput {
            scope_id,
            source_family: input.source_family,
            collection_status: input.collection_status,
            scope_description: input.scope_description,
            reason: input.reason,
            collector_label: input.collector_label,
            date_scope: input.date_scope,
            assertion_kind: input.assertion_kind,
            document_id: input.document_id,
            supersedes_scope_id: input.supersedes_scope_id,
        },
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_source_scope(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let analysis_scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let assertions = source_scope::list(&state.pool, analysis_scope).await?;
    Ok(no_store(
        json!({
            "source_scope": assertions,
            "missingness_establishes_misconduct": false,
            "uncollected_means_absent": false,
            "unknown_existence_means_clean_collection": false
        }),
        StatusCode::OK,
    ))
}

impl From<TemporalStateError> for ApiError {
    fn from(error: TemporalStateError) -> Self {
        match error {
            TemporalStateError::Invalid => {
                Self(StatusCode::BAD_REQUEST, "invalid temporal assertion")
            }
            TemporalStateError::Invisible => Self(
                StatusCode::NOT_FOUND,
                "temporal assertion reference unavailable",
            ),
            TemporalStateError::Conflict => {
                Self(StatusCode::CONFLICT, "temporal assertion conflict")
            }
            TemporalStateError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "temporal state runtime role unavailable",
            ),
            TemporalStateError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "temporal state unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemporalAssertionRequest {
    subject_label: String,
    assertion_type: String,
    object_text: String,
    valid_from: Option<String>,
    valid_to: Option<String>,
    epistemic_kind: String,
    qualification: String,
    evidence_id: Option<Uuid>,
    contribution_id: Option<Uuid>,
    rationale: String,
}

async fn record_temporal_state(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, assertion)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<TemporalAssertionRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let recorded = temporal_state::record(
        &state.pool,
        scope,
        actor,
        &TemporalAssertionInput {
            assertion_id: assertion,
            subject_label: input.subject_label,
            assertion_type: input.assertion_type,
            object_text: input.object_text,
            valid_from: input.valid_from,
            valid_to: input.valid_to,
            epistemic_kind: input.epistemic_kind,
            qualification: input.qualification,
            evidence_id: input.evidence_id,
            contribution_id: input.contribution_id,
            rationale: input.rationale,
        },
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_temporal_state(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let assertions = temporal_state::list(&state.pool, scope).await?;
    Ok(no_store(
        json!({
            "assertions": assertions,
            "assertions_establish_truth": false,
            "formal_authority_is_practical_influence": false,
            "inferred_awareness_is_observed_knowledge": false
        }),
        StatusCode::OK,
    ))
}

impl From<HypothesisPortfolioError> for ApiError {
    fn from(error: HypothesisPortfolioError) -> Self {
        match error {
            HypothesisPortfolioError::Invalid => Self(
                StatusCode::BAD_REQUEST,
                "invalid hypothesis portfolio input",
            ),
            HypothesisPortfolioError::Conflict => {
                Self(StatusCode::CONFLICT, "hypothesis portfolio conflict")
            }
            HypothesisPortfolioError::Invisible => {
                Self(StatusCode::NOT_FOUND, "hypothesis portfolio unavailable")
            }
            HypothesisPortfolioError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "hypothesis portfolio runtime role unavailable",
            ),
            HypothesisPortfolioError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "hypothesis portfolio unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HypothesisPortfolioEventRequest {
    event_kind: String,
    note: String,
}

async fn get_hypothesis_portfolio(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let portfolio = hypothesis_portfolio::portfolio(&state.pool, scope).await?;
    Ok(no_store(json!(portfolio), StatusCode::OK))
}

async fn record_hypothesis_portfolio_event(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, interpretation, event)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Json(input): Json<HypothesisPortfolioEventRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let recorded = hypothesis_portfolio::append_event(
        &state.pool,
        scope,
        actor,
        interpretation,
        event,
        &input.event_kind,
        &input.note,
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

impl From<ActorError> for ApiError {
    fn from(error: ActorError) -> Self {
        match error {
            ActorError::Invalid => Self(StatusCode::BAD_REQUEST, "invalid actor input"),
            ActorError::Invisible => Self(StatusCode::NOT_FOUND, "actor or source unavailable"),
            ActorError::Conflict => Self(StatusCode::CONFLICT, "actor replay conflict"),
            ActorError::Privileged => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actor runtime role unavailable",
            ),
            ActorError::Database(sqlx::Error::Database(ref db))
                if matches!(db.code().as_deref(), Some("23503" | "23514" | "23505")) =>
            {
                Self(StatusCode::BAD_REQUEST, "invalid actor reference")
            }
            ActorError::Database(_) => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "actor registry unavailable",
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorCreateRequest {
    display_label: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorKindRequest {
    actor_kind: Option<String>,
    rationale: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorBindingRequest {
    speaker_label: String,
    actor_id: Option<Uuid>,
    rationale: String,
}

async fn list_actors(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    Ok(no_store(
        json!({
            "actors": actors::list(&state.pool, workspace).await?,
            "actor_kind_is_investigation_gate": false,
            "actor_kind_establishes_mind": false
        }),
        StatusCode::OK,
    ))
}

async fn create_actor(
    State(state): State<AppState>,
    Path((workspace, actor_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<ActorCreateRequest>,
) -> Result<Response, ApiError> {
    let recorder = guard(&state, &headers, workspace, Capability::Write).await?;
    let (actor, replayed) = actors::create(
        &state.pool,
        workspace,
        recorder,
        actor_id,
        &input.display_label,
    )
    .await?;
    Ok(no_store(
        json!({"actor":actor,"replayed":replayed}),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn record_actor_kind(
    State(state): State<AppState>,
    Path((workspace, actor_id, event)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<ActorKindRequest>,
) -> Result<Response, ApiError> {
    let recorder = guard(&state, &headers, workspace, Capability::Write).await?;
    let recorded = actors::record_kind(
        &state.pool,
        workspace,
        recorder,
        actor_id,
        event,
        input.actor_kind.as_deref(),
        &input.rationale,
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn record_actor_binding(
    State(state): State<AppState>,
    Path((workspace, document, event)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(input): Json<ActorBindingRequest>,
) -> Result<Response, ApiError> {
    let recorder = guard(&state, &headers, workspace, Capability::Write).await?;
    let recorded = actors::bind_source_speaker(
        &state.pool,
        workspace,
        recorder,
        document,
        &input.speaker_label,
        event,
        input.actor_id,
        &input.rationale,
    )
    .await?;
    Ok(no_store(
        json!(recorded),
        if recorded.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn list_identity_subjects(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<SourceQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = identity::list_identities(
        &state.pool,
        workspace,
        query.after,
        query.limit.unwrap_or(100),
    )
    .await
    .map_err(|error| match error {
        IdentityError::Input => ApiError(StatusCode::BAD_REQUEST, "invalid identity page"),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "identity catalog unavailable",
        ),
    })?;
    Ok(no_store(
        json!({
            "identities": page.identities.iter().map(|identity| json!({
                "person_id": identity.person_id,
                "display_label": identity.display_label,
            })).collect::<Vec<_>>(),
            "next_after": page.next_after,
            "catalog_is_human_curated": true,
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
    fn utf8_source_locator_header_decodes_exactly_and_rejects_ambiguity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-source-locator-percent-encoded",
            "%E8%A8%98%E9%8C%B2-%F0%9F%93%A7.eml".parse().unwrap(),
        );
        assert_eq!(source_locator(&headers).unwrap(), "記録-📧.eml");

        headers.insert("x-source-locator", "legacy.eml".parse().unwrap());
        assert!(source_locator(&headers).is_err());
    }

    #[test]
    fn percent_encoded_source_locator_rejects_malformed_utf8_controls_and_oversize() {
        for value in ["%", "%GG", "%FF", "chat%0Aname.txt", "Alice%E2%80%AE.txt"] {
            assert!(decode_percent_encoded_locator(value).is_err(), "{value}");
        }
        let oversized = "%61".repeat(1025);
        assert!(decode_percent_encoded_locator(&oversized).is_err());
        assert_eq!(
            decode_percent_encoded_locator("ordinary%20chat.txt").unwrap(),
            "ordinary chat.txt"
        );
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
    async fn local_handoff_endpoint_is_one_shot_and_wrong_code_does_not_consume() {
        use http_body_util::BodyExt;
        use std::{fs, time::Duration};
        use tower::ServiceExt;

        let path =
            std::env::temp_dir().join(format!("niccolo-http-handoff-{}.token", Uuid::new_v4()));
        let token = format!("ncl_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        fs::write(&path, format!("{token}\n")).unwrap();
        let code = "d".repeat(64);
        let router = router_with_model_and_handoff(
            PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap(),
            Arc::new(UnavailableModel),
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap(),
        );
        let request = |code: &str| {
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/local-handoff")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"code":"{code}"}}"#)))
                .unwrap()
        };

        let wrong = router
            .clone()
            .oneshot(request(&"e".repeat(64)))
            .await
            .unwrap();
        assert_eq!(wrong.status(), StatusCode::NOT_FOUND);

        let accepted = router.clone().oneshot(request(&code)).await.unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        assert_eq!(accepted.headers()[header::CACHE_CONTROL], "no-store");
        let body = accepted.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["token"], token);

        let replay = router.clone().oneshot(request(&code)).await.unwrap();
        assert_eq!(replay.status(), StatusCode::NOT_FOUND);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn local_handoff_rejects_cross_origin_browser_requests_without_consuming_code() {
        use http_body_util::BodyExt;
        use std::{fs, time::Duration};
        use tower::ServiceExt;

        let path =
            std::env::temp_dir().join(format!("niccolo-http-origin-{}.token", Uuid::new_v4()));
        let token = format!("ncl_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        fs::write(&path, format!("{token}\n")).unwrap();
        let code = "c".repeat(64);
        let router = router_with_model_and_handoff(
            PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap(),
            Arc::new(UnavailableModel),
            LocalHandoff::configured(&code, path.clone(), Duration::from_secs(30)).unwrap(),
        );
        let request = |origin: &str| {
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/local-handoff")
                .header(header::HOST, "127.0.0.1:38080")
                .header(header::ORIGIN, origin)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"code":"{code}"}}"#)))
                .unwrap()
        };

        let rejected = router
            .clone()
            .oneshot(request("https://attacker.example"))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

        let accepted = router
            .clone()
            .oneshot(request("http://127.0.0.1:38080"))
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        let body = accepted.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["token"], token);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn embedded_technical_chunks_are_served_as_javascript_without_path_traversal() {
        use tower::ServiceExt;
        let router = router(PgPool::connect_lazy("postgres://x@127.0.0.1/test").unwrap());
        for name in [
            "AnalysisPage",
            "InspectorPage",
            "ReasoningPage",
            "IdentitiesPage",
            "FeedbackPage",
            "EpistemicPlaygroundPage",
            "ForensicPage",
        ] {
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri(format!("/assets/chunks/{name}.js"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "missing JS chunk: {name}"
            );
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/javascript; charset=utf-8"
            );
        }
        for path in [
            "/assets/chunks/nonexistent.js",
            "/assets/chunks/app.js",
            "/assets/chunks/..%2Fapp.js",
        ] {
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
            assert_ne!(
                response.status(),
                StatusCode::OK,
                "unexpected JS asset: {path}"
            );
        }
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

async fn get_bff_epistemic(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = bff_epistemic::get(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "epistemic view unavailable",
        )
    })?;
    Ok(no_store(json!(view), StatusCode::OK))
}

async fn get_reasoning_handoff(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let handoff = reasoning_handoff::bundle(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning handoff unavailable",
        )
    })?;
    let sha256 = reasoning_handoff::sha256(&handoff).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning handoff unavailable",
        )
    })?;
    Ok(no_store(
        json!({
            "handoff": handoff,
            "handoff_sha256": sha256,
            "read_model_is_authority": false,
            "read_model_executes_actions": false
        }),
        StatusCode::OK,
    ))
}

async fn list_reasoning_handoff_receipts(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let current = reasoning_handoff::bundle(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning handoff unavailable",
        )
    })?;
    let current_sha = reasoning_handoff::sha256(&current).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning handoff unavailable",
        )
    })?;
    let rows = review_governance::handoff_receipts(&state.pool, workspace, situation, analysis)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "handoff receipts unavailable",
            )
        })?;
    Ok(no_store(
        json!({
            "receipts": rows.into_iter().map(|r| {
                let matches_current_handoff = r.handoff_sha256 == current_sha;
                json!({
                    "receipt_id": r.receipt_id,
                    "handoff_sha256": r.handoff_sha256,
                    "kind": r.kind,
                    "note": r.note,
                    "recipient_principal_id": r.recipient,
                    "created_at": r.created_at,
                    "matches_current_handoff": matches_current_handoff,
                    "receipt_means_agreement": r.receipt_means_agreement,
                    "receipt_grants_authority": r.receipt_grants_authority
                })
            }).collect::<Vec<_>>(),
            "current_handoff_sha256": current_sha,
            "receipt_means_agreement": false,
            "receipt_grants_authority": false
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HandoffReceiptRequest {
    handoff_sha256: String,
    kind: String,
    note: Option<String>,
}

async fn record_reasoning_handoff_receipt(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, receipt)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<HandoffReceiptRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let current = reasoning_handoff::bundle(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning handoff unavailable",
        )
    })?;
    let current_sha = reasoning_handoff::sha256(&current).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning handoff unavailable",
        )
    })?;
    if body.handoff_sha256 != current_sha {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "handoff changed; refresh before recording receipt",
        ));
    }
    let receipt_kind = body.kind.clone();
    let replayed = review_governance::record_handoff_receipt(
        &state.pool,
        workspace,
        situation,
        analysis,
        &HandoffReceipt {
            receipt_id: receipt,
            handoff_sha256: body.handoff_sha256,
            kind: body.kind,
            note: body.note,
            recipient: actor,
        },
    )
    .await
    .map_err(|error| match error {
        ReviewGovernanceError::Invalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid handoff receipt")
        }
        ReviewGovernanceError::Conflict => {
            ApiError(StatusCode::CONFLICT, "handoff receipt replay conflict")
        }
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "handoff receipt unavailable",
        ),
    })?;
    Ok(no_store(
        json!({
            "receipt_id": receipt,
            "handoff_sha256": current_sha,
            "kind": receipt_kind,
            "recipient_principal_id": actor,
            "replayed": replayed,
            "receipt_means_agreement": false,
            "receipt_grants_authority": false
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn get_reasoning_input_summary(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let packet = reasoning_packet::build(
        &state.pool,
        crate::analysis_structure::AnalysisScope {
            workspace,
            situation,
            analysis,
        },
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning input unavailable",
        )
    })?;
    let sha256 = reasoning_packet::sha256(&packet).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "reasoning input unavailable",
        )
    })?;
    Ok(no_store(
        json!({
            "analysis_id": analysis,
            "input_revision": packet.frozen_input.revision,
            "input_is_current": packet.frozen_input.input_is_current,
            "source_fragment_count_total": packet.frozen_input.source_fragment_count_total,
            "source_fragment_count_included": packet.frozen_input.source_fragment_count_included,
            "source_bytes_included": packet.frozen_input.source_bytes_included,
            "source_packet_truncated": packet.frozen_input.source_packet_truncated,
            "identity_event_no_max": packet.frozen_input.identity_event_no_max,
            "identity_binding_count": packet.frozen_input.identity_bindings.len(),
            "identity_bindings_are_human_confirmed": packet.frozen_input.identity_bindings_are_human_confirmed,
            "matching_speaker_labels_imply_same_person": packet.frozen_input.matching_speaker_labels_imply_same_person,
            "imported_context_count": packet.current_human_control.imported_context.len(),
            "imported_context_was_part_of_frozen_input": packet.current_human_control.imported_context_was_part_of_frozen_input,
            "imported_context_is_documentary_evidence": packet.current_human_control.imported_context_is_documentary_evidence,
            "imported_context_is_current_human_selected_context": true,
            "reasoning_packet_sha256": sha256,
            "packet_asserts_truth": packet.packet_asserts_truth,
            "summary_is_provider_input_observability": true
        }),
        StatusCode::OK,
    ))
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
struct BffObserveQuery {
    after_version: i64,
    after_event: Option<i64>,
    timeout_ms: Option<u64>,
    limit: Option<i64>,
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
fn intent_error(error: IntentError) -> ApiError {
    match error {
        IntentError::Invalid => ApiError(StatusCode::BAD_REQUEST, "invalid human intent request"),
        IntentError::Analysis(error) => error.into(),
        IntentError::Operation(error) => bff_error(error),
    }
}

fn intent_response(
    mut launch: bff_intent::IntentLaunch,
    operation: bff_operations::OperationView,
    status: StatusCode,
) -> Response {
    launch.operation = operation;
    let etag = launch.operation.etag.clone();
    let retry = launch.operation.retry_after_ms;
    let mut response = (status, Json(launch)).into_response();
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

fn spawn_adaptive_bff_execution(
    pool: PgPool,
    model: Arc<dyn ReasoningModel>,
    scope: OperationScope,
    actor: Uuid,
    digest: String,
) {
    tokio::spawn(async move {
        authz::with_auth_digest(digest, async move {
            let _ =
                bff_executor::execute_adaptive_default(&pool, scope, actor, model.as_ref()).await;
        })
        .await;
    });
}

async fn prepare_intent(
    state: &AppState,
    workspace: Uuid,
    situation: Uuid,
    operation: Uuid,
    actor: Uuid,
    request: &IntentRequest,
) -> Result<bff_intent::IntentLaunch, ApiError> {
    bff_intent::submit(&state.pool, workspace, situation, operation, actor, request)
        .await
        .map_err(intent_error)
}

async fn bff_intent_sync(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<IntentRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let launch = prepare_intent(&state, workspace, situation, operation, actor, &request).await?;
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let view = bff_executor::execute(&state.pool, scope, actor, state.model.as_ref())
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "intent execution failed"))?;
    Ok(intent_response(launch, view, StatusCode::OK))
}

async fn bff_intent_async(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<IntentRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let digest = unique_bearer(&headers)
        .and_then(|token| authz::credential_digest(token).ok())
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))?;
    let launch = prepare_intent(&state, workspace, situation, operation, actor, &request).await?;
    let view = launch.operation.clone();
    if !view.terminal && view.state != "WAITING_FOR_HUMAN" {
        let pool = state.pool.clone();
        let model = state.model.clone();
        let scope = OperationScope {
            workspace,
            situation,
            operation,
        };
        spawn_adaptive_bff_execution(pool, model, scope, actor, digest);
    }
    Ok(intent_response(launch, view, StatusCode::ACCEPTED))
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
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let view = bff_executor::execute(&state.pool, scope, actor, state.model.as_ref())
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "operation execution failed",
            )
        })?;
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
        let scope = OperationScope {
            workspace,
            situation,
            operation,
        };
        spawn_adaptive_bff_execution(pool, model, scope, actor, digest);
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
fn checkpoint_error(error: CheckpointError) -> ApiError {
    match error {
        CheckpointError::Invalid => ApiError(StatusCode::BAD_REQUEST, "invalid checkpoint query"),
        CheckpointError::Invisible => ApiError(StatusCode::NOT_FOUND, "checkpoint unavailable"),
        CheckpointError::Conflict => ApiError(
            StatusCode::CONFLICT,
            "checkpoint integrity or replay conflict",
        ),
        CheckpointError::TooMany => {
            ApiError(StatusCode::CONFLICT, "operation checkpoint limit reached")
        }
        CheckpointError::Database(_) | CheckpointError::Json(_) | CheckpointError::Privileged => {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "operation checkpoints unavailable",
            )
        }
    }
}

async fn get_bff_operation_checkpoints(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(q): Query<BffEventsQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = operation_checkpoint::list(
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
    .map_err(checkpoint_error)?;
    Ok(no_store(json!(page), StatusCode::OK))
}

fn checkpoint_package_error(error: CheckpointPackageError) -> ApiError {
    match error {
        CheckpointPackageError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid checkpoint package request",
        ),
        CheckpointPackageError::Integrity => ApiError(
            StatusCode::CONFLICT,
            "checkpoint package integrity mismatch",
        ),
        CheckpointPackageError::TooLarge => ApiError(
            StatusCode::CONFLICT,
            "checkpoint package exceeds export limit",
        ),
        CheckpointPackageError::Json(_) => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "checkpoint package unavailable",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointPackageQuery {
    milestone_id: Option<Uuid>,
}

async fn get_bff_operation_checkpoint_package(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<CheckpointPackageQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let checkpoint = operation_checkpoint::get(&state.pool, scope, checkpoint)
        .await
        .map_err(checkpoint_error)?;
    let package = match query.milestone_id {
        Some(milestone_id) => {
            if milestone_id.is_nil() {
                return Err(ApiError(
                    StatusCode::BAD_REQUEST,
                    "invalid checkpoint milestone package request",
                ));
            }
            let milestone = checkpoint_milestone::get(&state.pool, scope, checkpoint.checkpoint_id)
                .await
                .map_err(checkpoint_milestone_error)?;
            if milestone.milestone_id != milestone_id {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "checkpoint milestone identity mismatch",
                ));
            }
            checkpoint_package::build_milestone(scope, &checkpoint, &milestone)
                .map_err(checkpoint_package_error)?
        }
        None => checkpoint_package::build(scope, &checkpoint).map_err(checkpoint_package_error)?,
    };
    Ok(no_store(json!(package), StatusCode::OK))
}

fn checkpoint_milestone_error(error: MilestoneError) -> ApiError {
    match error {
        MilestoneError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid checkpoint milestone request",
        ),
        MilestoneError::Conflict => ApiError(
            StatusCode::CONFLICT,
            "checkpoint already has a different milestone declaration",
        ),
        MilestoneError::Invisible | MilestoneError::Checkpoint(CheckpointError::Invisible) => {
            ApiError(StatusCode::NOT_FOUND, "checkpoint milestone unavailable")
        }
        MilestoneError::Checkpoint(CheckpointError::Invalid) => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid checkpoint milestone request",
        ),
        MilestoneError::Checkpoint(CheckpointError::Conflict) => ApiError(
            StatusCode::CONFLICT,
            "checkpoint milestone source is inconsistent",
        ),
        MilestoneError::Database(_)
        | MilestoneError::Privileged
        | MilestoneError::Checkpoint(_) => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "checkpoint milestones unavailable",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointMilestoneRequest {
    milestone_kind: String,
    reason: String,
}

async fn get_bff_operation_checkpoint_milestone(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let milestone = checkpoint_milestone::get(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        checkpoint,
    )
    .await
    .map_err(checkpoint_milestone_error)?;
    Ok(no_store(json!(milestone), StatusCode::OK))
}

async fn declare_bff_operation_checkpoint_milestone(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint, milestone)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Json(request): Json<CheckpointMilestoneRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let (milestone, replayed) = checkpoint_milestone::declare(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        checkpoint,
        &checkpoint_milestone::NewMilestone {
            milestone_id: milestone,
            milestone_kind: request.milestone_kind,
            reason: request.reason,
            declared_by: actor,
        },
    )
    .await
    .map_err(checkpoint_milestone_error)?;
    Ok(no_store(
        json!({
            "milestone": milestone,
            "replayed": replayed,
            "remote_publish_performed": false,
            "checkpoint_epistemic_status_changed": false
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

fn checkpoint_git_publication_error(error: CheckpointGitPublicationError) -> ApiError {
    match error {
        CheckpointGitPublicationError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid checkpoint git publication request",
        ),
        CheckpointGitPublicationError::Conflict => ApiError(
            StatusCode::CONFLICT,
            "checkpoint git publication identity conflict",
        ),
        CheckpointGitPublicationError::Invisible => ApiError(
            StatusCode::NOT_FOUND,
            "checkpoint git publication intent unavailable",
        ),
        CheckpointGitPublicationError::Stale => ApiError(
            StatusCode::CONFLICT,
            "checkpoint git publication intent is stale; preview and record a new intent",
        ),
        CheckpointGitPublicationError::Checkpoint(CheckpointError::Invisible) => {
            ApiError(StatusCode::NOT_FOUND, "checkpoint unavailable")
        }
        CheckpointGitPublicationError::Git(GitPublicationError::Invisible) => {
            ApiError(StatusCode::NOT_FOUND, "git publication target unavailable")
        }
        CheckpointGitPublicationError::Milestone(MilestoneError::Invisible) => {
            ApiError(StatusCode::NOT_FOUND, "checkpoint milestone unavailable")
        }
        CheckpointGitPublicationError::Checkpoint(CheckpointError::Invalid)
        | CheckpointGitPublicationError::Git(GitPublicationError::Invalid)
        | CheckpointGitPublicationError::Milestone(MilestoneError::Invalid)
        | CheckpointGitPublicationError::Package(CheckpointPackageError::Invalid) => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid checkpoint git publication request",
        ),
        CheckpointGitPublicationError::Checkpoint(CheckpointError::Conflict)
        | CheckpointGitPublicationError::Milestone(MilestoneError::Conflict)
        | CheckpointGitPublicationError::Package(CheckpointPackageError::Integrity)
        | CheckpointGitPublicationError::Git(GitPublicationError::Conflict)
        | CheckpointGitPublicationError::Git(GitPublicationError::Stale) => ApiError(
            StatusCode::CONFLICT,
            "checkpoint git publication source is inconsistent",
        ),
        CheckpointGitPublicationError::Package(CheckpointPackageError::TooLarge) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "checkpoint package exceeds publication limit",
        ),
        CheckpointGitPublicationError::Database(_)
        | CheckpointGitPublicationError::Privileged
        | CheckpointGitPublicationError::Checkpoint(_)
        | CheckpointGitPublicationError::Milestone(_)
        | CheckpointGitPublicationError::Package(_)
        | CheckpointGitPublicationError::Git(_) => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "checkpoint git publication planning unavailable",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointGitPublicationPreviewQuery {
    target_id: Uuid,
    milestone_id: Option<Uuid>,
}

async fn get_bff_operation_checkpoint_git_publication_preview(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<CheckpointGitPublicationPreviewQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let preview = checkpoint_git_publication::preview(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        checkpoint,
        query.target_id,
        query.milestone_id,
    )
    .await
    .map_err(checkpoint_git_publication_error)?;
    Ok(no_store(json!(preview), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointGitPublicationIntentRequest {
    target_id: Uuid,
    milestone_id: Option<Uuid>,
}

async fn record_bff_operation_checkpoint_git_publication_intent(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint, publication)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Json(request): Json<CheckpointGitPublicationIntentRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let (intent, replayed) = checkpoint_git_publication::record_intent(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        checkpoint,
        actor,
        publication,
        request.target_id,
        request.milestone_id,
    )
    .await
    .map_err(checkpoint_git_publication_error)?;
    Ok(no_store(
        json!({
            "intent": intent,
            "replayed": replayed,
            "remote_publish_performed": false,
            "requires_provider_authorization_before_publish": true,
            "checkpoint_status_preserved": true,
            "establishes_truth": false,
            "proves_completeness": false,
            "authorizes_real_world_action": false
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn get_bff_operation_checkpoint_git_publication_remote_status(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint, publication)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let status = checkpoint_git_publication::remote_status(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        checkpoint,
        publication,
    )
    .await
    .map_err(checkpoint_git_publication_error)?;
    Ok(no_store(json!(status), StatusCode::OK))
}

async fn list_bff_operation_checkpoint_git_publication_remote_events(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint, publication)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Query(query): Query<GitPublicationRemoteEventListQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let events = checkpoint_git_publication::list_remote_events(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        checkpoint,
        publication,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(checkpoint_git_publication_error)?;
    Ok(no_store(
        json!({
            "events": events,
            "provider_secret_stored": false,
            "event_payloads_include_provider_response_bodies": false
        }),
        StatusCode::OK,
    ))
}

async fn publish_bff_operation_checkpoint_git_publication_intent(
    State(state): State<AppState>,
    Path((workspace, situation, operation, checkpoint, publication)): Path<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        Uuid,
    )>,
    headers: HeaderMap,
    Json(request): Json<GitHubPublishRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    if request.attempt_id.is_nil()
        || request.confirmation != "PUBLISH_OPERATION_CHECKPOINT_TO_GITHUB"
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "explicit checkpoint GitHub publication confirmation required",
        ));
    }
    let scope = OperationScope {
        workspace,
        situation,
        operation,
    };
    let existing =
        checkpoint_git_publication::remote_status(&state.pool, scope, checkpoint, publication)
            .await
            .map_err(checkpoint_git_publication_error)?;
    if existing.remote_publish_verified {
        return Ok(no_store(
            json!({
                "status": existing,
                "replayed_verified_publication": true,
                "provider_secret_stored": false
            }),
            StatusCode::OK,
        ));
    }

    let provider_token = transient_github_token(&headers)?;
    let material = checkpoint_git_publication::publication_material(
        &state.pool,
        scope,
        checkpoint,
        publication,
    )
    .await
    .map_err(checkpoint_git_publication_error)?;
    let publish_result = tokio::task::spawn_blocking(move || {
        let transport = github_publisher::CurlGitHubTransport::default();
        github_publisher::publish_checkpoint_package(
            &transport,
            &material.target,
            &material.intent,
            &material.package,
            &provider_token,
        )
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::BAD_GATEWAY,
            "GitHub checkpoint publication task failed",
        )
    })?;

    let verified = match publish_result {
        Ok(verified) => verified,
        Err(error) => {
            let failure_class = match error {
                GitHubPublishError::LocalIntegrity | GitHubPublishError::Invalid => {
                    "LOCAL_INTEGRITY"
                }
                GitHubPublishError::Transport => "TRANSPORT",
                GitHubPublishError::ProviderRejected(_) => "PROVIDER_REJECTED",
                GitHubPublishError::VerificationMismatch
                | GitHubPublishError::InvalidProviderResponse => "VERIFICATION_MISMATCH",
            };
            let _ = checkpoint_git_publication::record_remote_event(
                &state.pool,
                workspace,
                actor,
                &git_publication::RemoteEventInput {
                    event_id: request.attempt_id,
                    publication_id: publication,
                    kind: git_publication::RemoteEventKind::PublishFailed,
                    remote_commit_sha: None,
                    observed_ref: None,
                    failure_class: Some(failure_class.into()),
                },
            )
            .await;
            return Err(github_publish_error(error));
        }
    };

    for (kind, observed_ref) in [
        (git_publication::RemoteEventKind::PushAccepted, None),
        (
            git_publication::RemoteEventKind::RefVerified,
            Some(verified.ref_name.clone()),
        ),
    ] {
        checkpoint_git_publication::record_remote_event(
            &state.pool,
            workspace,
            actor,
            &git_publication::RemoteEventInput {
                event_id: request.attempt_id,
                publication_id: publication,
                kind,
                remote_commit_sha: Some(verified.commit_sha.clone()),
                observed_ref,
                failure_class: None,
            },
        )
        .await
        .map_err(checkpoint_git_publication_error)?;
    }

    let status =
        checkpoint_git_publication::remote_status(&state.pool, scope, checkpoint, publication)
            .await
            .map_err(checkpoint_git_publication_error)?;
    if !status.remote_publish_verified
        || status.verified_commit_sha.as_deref() != Some(verified.commit_sha.as_str())
        || status.verified_ref.as_deref() != Some(verified.ref_name.as_str())
    {
        return Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "remote checkpoint publication verification ledger mismatch",
        ));
    }
    Ok(no_store(
        json!({
            "status": status,
            "repository_full_name": verified.repository_full_name,
            "package_sha256": verified.package_sha256,
            "remote_commit_already_present": verified.already_present,
            "provider_secret_stored": false,
            "remote_publish_verified": true,
            "checkpoint_status_preserved": true
        }),
        StatusCode::OK,
    ))
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

async fn observe_bff_operation(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(q): Query<BffObserveQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let observation = bff_operations::observe(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
        q.after_version,
        q.after_event.unwrap_or(0),
        q.timeout_ms.unwrap_or(5000),
        q.limit.unwrap_or(50),
    )
    .await
    .map_err(bff_error)?;
    let etag = observation.operation.etag.clone();
    let mut response = no_store(json!(observation), StatusCode::OK);
    response
        .headers_mut()
        .insert(header::ETAG, etag.parse().unwrap());
    Ok(response)
}

async fn resolve_agentic_human_boundary(
    State(state): State<AppState>,
    Path((workspace, situation, operation, step)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<AgenticHumanResponseRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let digest = unique_bearer(&headers)
        .and_then(|token| authz::credential_digest(token).ok())
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))?;
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
        agentic_controller::ControllerError::BudgetExhausted => ApiError(
            StatusCode::CONFLICT,
            "agentic step budget exhausted; stop or start another analysis",
        ),
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
    if resolved.executor_resume_required
        && resolved.operation_state == "RUNNING"
        && !controller.waiting_for_human
    {
        let pool = state.pool.clone();
        let model = state.model.clone();
        spawn_adaptive_bff_execution(pool, model, scope, actor, digest);
    }
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

// EPI-004: a human may state a subjective percentage about a specific claim.
// Authentication determines the actor; callers cannot impersonate a principal.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanCredenceRequest {
    confidence_percent: i16,
    usage_policy: String,
    rationale: String,
}
fn credence_api_error(error: CredenceError) -> ApiError {
    match error {
        CredenceError::Invalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid subjective confidence")
        }
        CredenceError::Invisible => ApiError(StatusCode::NOT_FOUND, "claim unavailable"),
        CredenceError::Conflict => ApiError(StatusCode::CONFLICT, "credence ID conflict"),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "human confidence unavailable",
        ),
    }
}
async fn record_human_credence(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, claim, credence)): Path<(Uuid, Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<HumanCredenceRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let submitted = NewHumanCredence {
        credence_id: credence,
        claim_id: claim,
        confidence_percent: body.confidence_percent,
        usage_policy: body.usage_policy,
        rationale: body.rationale,
        recorded_by: actor,
    };
    let inserted = human_credence::record(&state.pool, scope, &submitted)
        .await
        .map_err(credence_api_error)?;
    Ok(no_store(
        json!({
            "credence_id": credence,
            "claim_id": claim,
            "confidence_percent": submitted.confidence_percent,
            "usage_policy": submitted.usage_policy,
            "recorded_by": actor,
            "replayed": !inserted,
            "is_human_declaration": true,
            "is_calibrated_probability": false,
            "is_documentary_evidence": false,
            "makes_claim_a_fact": false
        }),
        if inserted {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
    ))
}
async fn list_human_credence(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, claim)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let entries = human_credence::list_for_claim(&state.pool, scope, claim)
        .await
        .map_err(credence_api_error)?;
    Ok(no_store(
        json!({
            "claim_id": claim,
            "human_credence": entries,
            "confidence_establishes_truth": false
        }),
        StatusCode::OK,
    ))
}

// EPI-007: deterministic, read-only preview for hypothetical weighting settings.
// Inputs are NOT checked against the evidence database; no persisted judgment,
// source-validation or fact claim is created by this endpoint.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeightPreviewRequest {
    /// Explicit trial override; omission picks the effective persisted policy.
    policy: Option<WeightPolicy>,
    situation_id: Option<Uuid>,
    human_declared_confidence_percent: Option<u8>,
    observations: Vec<WeightedObservation>,
}
async fn preview_epistemic_balance(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<WeightPreviewRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    // A supplied Situation must be real and visible even for an explicit trial.
    let stored = if request.policy.is_none() || request.situation_id.is_some() {
        Some(
            epistemic_policy_store::effective(&state.pool, workspace, request.situation_id)
                .await
                .map_err(epistemic_policy_error)?,
        )
    } else {
        None
    };
    let (policy, policy_source) = if let Some(override_policy) = request.policy {
        (override_policy, "EXPLICIT_PREVIEW_OVERRIDE".to_string())
    } else {
        let stored = stored.ok_or(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "epistemic policy unavailable",
        ))?;
        (stored.policy, stored.scope)
    };
    let report = epistemic_weights::evaluate(
        policy,
        request.human_declared_confidence_percent,
        &request.observations,
    )
    .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid preview weights"))?;
    Ok(no_store(
        json!({
            "report": report,
            "policy_used": policy,
            "policy_source": policy_source,
            "mode": "UNVERIFIED_PREVIEW",
            "inputs_checked_against_database": false,
            "changes_any_persisted_state": false,
            "authorizes_real_world_action": false
        }),
        StatusCode::OK,
    ))
}

// EPI-008: a narrow human-configurable policy API.
// All actor identity and workspace permissions come from the bearer token.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyQuery {
    situation_id: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewEpistemicPolicyRequest {
    situation_id: Option<Uuid>,
    policy: WeightPolicy,
    rationale: String,
}
fn epistemic_policy_error(error: PolicyStoreError) -> ApiError {
    match error {
        PolicyStoreError::Invalid => ApiError(StatusCode::BAD_REQUEST, "invalid epistemic policy"),
        PolicyStoreError::Conflict => {
            ApiError(StatusCode::CONFLICT, "epistemic policy ID conflict")
        }
        PolicyStoreError::Invisible => ApiError(StatusCode::NOT_FOUND, "situation unavailable"),
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "epistemic policy unavailable",
        ),
    }
}
async fn record_epistemic_policy(
    State(state): State<AppState>,
    Path((workspace, policy_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(body): Json<NewEpistemicPolicyRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let policy = NewPolicy {
        policy_id,
        workspace_id: workspace,
        situation_id: body.situation_id,
        policy: body.policy,
        rationale: body.rationale,
        recorded_by: actor,
    };
    let inserted = epistemic_policy_store::record(&state.pool, &policy)
        .await
        .map_err(epistemic_policy_error)?;
    Ok(no_store(
        json!({
            "policy_id":policy_id,"situation_id":policy.situation_id,"policy":policy.policy,
            "rationale":policy.rationale,"recorded_by":actor,"replayed":!inserted,
            "is_probability_model":false,"retroactively_changes_frozen_analysis":false
        }),
        if inserted {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
    ))
}
async fn get_effective_epistemic_policy(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    Query(query): Query<PolicyQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = epistemic_policy_store::effective(&state.pool, workspace, query.situation_id)
        .await
        .map_err(epistemic_policy_error)?;
    Ok(no_store(json!({"effective_policy":view}), StatusCode::OK))
}

// EPI-010: one bounded what-if comparison; still not a full reasoning replay.
// No evidence IDs are verified here, and no historical artifacts are mutated.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EpistemicComparisonRequest {
    situation_id: Option<Uuid>,
    baseline_policy: Option<WeightPolicy>,
    trial_policy: WeightPolicy,
    human_declared_confidence_percent: Option<u8>,
    observations: Vec<WeightedObservation>,
}
async fn compare_epistemic_policies(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<EpistemicComparisonRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let inherited = if request.baseline_policy.is_none() || request.situation_id.is_some() {
        Some(
            epistemic_policy_store::effective(&state.pool, workspace, request.situation_id)
                .await
                .map_err(epistemic_policy_error)?,
        )
    } else {
        None
    };
    let (baseline_policy, baseline_source) = if let Some(baseline) = request.baseline_policy {
        (baseline, "EXPLICIT_BASELINE".to_string())
    } else {
        let inherited = inherited.ok_or(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "epistemic policy unavailable",
        ))?;
        (inherited.policy, inherited.scope)
    };
    let baseline = epistemic_weights::evaluate(
        baseline_policy,
        request.human_declared_confidence_percent,
        &request.observations,
    )
    .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid baseline weights"))?;
    let trial = epistemic_weights::evaluate(
        request.trial_policy,
        request.human_declared_confidence_percent,
        &request.observations,
    )
    .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid trial weights"))?;
    Ok(no_store(
        json!({
            "mode": "HEURISTIC_ONLY_COMPARISON",
            "baseline_policy_source": baseline_source,
            "baseline_policy": baseline_policy,
            "trial_policy": request.trial_policy,
            "baseline": baseline,
            "trial": trial,
            "net_points_delta": trial.net_points - baseline.net_points,
            "support_points_delta": trial.support_points - baseline.support_points,
            "counter_points_delta": trial.counter_points - baseline.counter_points,
            "is_full_reasoning_replay": false,
            "inputs_checked_against_database": false,
            "changes_any_persisted_state": false,
            "proves_claim_truth": false,
            "authorizes_real_world_action": false
        }),
        StatusCode::OK,
    ))
}

// EPI-013: source-checked human-declared weights on an existing analysis claim.
// Never infer stance or provenance from the request body.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroundedClaimWeightRequest {
    policy: Option<WeightPolicy>,
    human_declared_confidence_percent: Option<u8>,
    evidence: Vec<DeclaredEvidenceStrength>,
}
fn grounded_weight_error(error: GroundedWeightError) -> ApiError {
    match error {
        GroundedWeightError::Invalid | GroundedWeightError::Weight(_) => {
            ApiError(StatusCode::BAD_REQUEST, "invalid claim evidence weights")
        }
        GroundedWeightError::MissingClaim => ApiError(StatusCode::NOT_FOUND, "claim unavailable"),
        GroundedWeightError::UnlinkedEvidence => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "evidence not linked to this claim",
        ),
        GroundedWeightError::AmbiguousLink => {
            ApiError(StatusCode::CONFLICT, "conflicting claim-to-evidence use")
        }
        GroundedWeightError::Lineage(
            crate::grounded_lineage_store::StoredLineageError::Cluster(
                crate::provenance_clusters::ClusterError::Cycle,
            ),
        ) => ApiError(StatusCode::CONFLICT, "cyclic recorded provenance"),
        GroundedWeightError::Lineage(
            crate::grounded_lineage_store::StoredLineageError::Cluster(
                crate::provenance_clusters::ClusterError::Withdrawn,
            ),
        ) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "withdrawn recorded provenance",
        ),
        GroundedWeightError::Lineage(
            crate::grounded_lineage_store::StoredLineageError::Cluster(
                crate::provenance_clusters::ClusterError::Missing,
            ),
        ) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "missing recorded ancestor",
        ),
        GroundedWeightError::Lineage(
            crate::grounded_lineage_store::StoredLineageError::Invalid,
        )
        | GroundedWeightError::Lineage(
            crate::grounded_lineage_store::StoredLineageError::Cluster(
                crate::provenance_clusters::ClusterError::Invalid,
            ),
        ) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "recorded provenance exceeds validation limits",
        ),

        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "grounded weighting unavailable",
        ),
    }
}
async fn evaluate_grounded_claim_weights(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, claim)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<GroundedClaimWeightRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let (policy, policy_source) = if let Some(policy) = request.policy {
        (policy, "EXPLICIT_TRIAL".to_owned())
    } else {
        let selected = epistemic_policy_store::effective(&state.pool, workspace, Some(situation))
            .await
            .map_err(epistemic_policy_error)?;
        (selected.policy, selected.scope)
    };
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let result = grounded_claim_weights::evaluate_claim(
        &state.pool,
        scope,
        claim,
        policy,
        request.human_declared_confidence_percent,
        &request.evidence,
    )
    .await
    .map_err(grounded_weight_error)?;
    Ok(no_store(
        json!({
            "mode": "GROUNDED_CLAIM_HEURISTIC",
            "policy_source": policy_source,
            "policy_used": policy,
            "assessment": result,
            "is_calibrated_probability": false,
            "changes_any_persisted_state": false,
            "needs_human_interpretation": true
        }),
        StatusCode::OK,
    ))
}

fn presentation_api_error(err: PresentationError) -> ApiError {
    match err {
        PresentationError::Manifest(crate::analysis_manifest::ManifestError::Invisible) => {
            ApiError(StatusCode::NOT_FOUND, "published analysis unavailable")
        }
        PresentationError::TooLarge | PresentationError::SourceExtractTooLarge => ApiError(
            StatusCode::PAYLOAD_TOO_LARGE,
            "presentation package too large",
        ),
        PresentationError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid presentation export request",
        ),
        PresentationError::Conflict => {
            ApiError(StatusCode::CONFLICT, "presentation export event conflict")
        }
        _ => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "presentation package unavailable",
        ),
    }
}

async fn get_presentation_source_preview(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let preview = presentation_package::examined_source_export_preview(
        &state.pool,
        workspace,
        situation,
        analysis,
    )
    .await
    .map_err(presentation_api_error)?;
    Ok(no_store(json!(preview), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationExportEventsQuery {
    limit: Option<i64>,
}

async fn list_presentation_export_events(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<PresentationExportEventsQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let page = presentation_package::list_sensitive_export_events(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(presentation_api_error)?;
    Ok(no_store(json!(page), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationPackageRequest {
    include_examined_source_fragments: bool,
    export_event_id: Uuid,
}

async fn post_presentation_package_with_examined_sources(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<PresentationPackageRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    if !request.include_examined_source_fragments || request.export_event_id.is_nil() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "explicit examined-source inclusion required",
        ));
    }
    let (package, preview) = presentation_package::build_with_examined_sources_and_preview(
        &state.pool,
        workspace,
        situation,
        analysis,
    )
    .await
    .map_err(presentation_api_error)?;
    let package_sha256 =
        presentation_package::package_sha256(&package).map_err(presentation_api_error)?;
    presentation_package::record_sensitive_export_event(
        &state.pool,
        presentation_package::SensitiveExportRequest {
            workspace,
            situation,
            analysis,
            export_event_id: request.export_event_id,
            actor,
            preview: &preview,
            package_sha256: &package_sha256,
        },
    )
    .await
    .map_err(presentation_api_error)?;
    Ok(no_store(json!(package), StatusCode::OK))
}

fn grounded_score_store_error(error: GroundedScoreStoreError) -> ApiError {
    match error {
        GroundedScoreStoreError::Invalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid grounded score record")
        }
        GroundedScoreStoreError::Conflict => {
            ApiError(StatusCode::CONFLICT, "grounded score ID conflict")
        }
        GroundedScoreStoreError::TooMany => {
            ApiError(StatusCode::CONFLICT, "grounded score record limit reached")
        }
        GroundedScoreStoreError::Corrupt
        | GroundedScoreStoreError::Json(_)
        | GroundedScoreStoreError::Database(_)
        | GroundedScoreStoreError::Privileged => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "grounded score record unavailable",
        ),
    }
}

async fn record_grounded_claim_score(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, claim, score)): Path<(Uuid, Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<GroundedClaimWeightRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    if score.is_nil() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "invalid grounded score ID",
        ));
    }
    let (policy, policy_source) = if let Some(policy) = request.policy {
        (policy, "EXPLICIT_TRIAL".to_owned())
    } else {
        let selected = epistemic_policy_store::effective(&state.pool, workspace, Some(situation))
            .await
            .map_err(epistemic_policy_error)?;
        (selected.policy, selected.scope)
    };
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let result = grounded_claim_weights::evaluate_claim(
        &state.pool,
        scope,
        claim,
        policy,
        request.human_declared_confidence_percent,
        &request.evidence,
    )
    .await
    .map_err(grounded_weight_error)?;
    let (record, replayed) = grounded_score_store::record(
        &state.pool,
        &NewGroundedScoreRecord {
            scope,
            claim_id: claim,
            score_id: score,
            policy_source,
            policy,
            human_declared_confidence_percent: request.human_declared_confidence_percent,
            declared_evidence: request.evidence,
            result,
            recorded_by: actor,
        },
    )
    .await
    .map_err(grounded_score_store_error)?;
    Ok(no_store(
        json!({
            "record": record,
            "replayed": replayed,
            "is_immutable_record": true,
            "score_is_probability_of_truth": false,
            "establishes_claim_truth": false,
            "modifies_published_analysis": false
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroundedScoreListQuery {
    limit: Option<i64>,
}

async fn list_grounded_claim_scores(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<GroundedScoreListQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let page =
        grounded_score_store::list_for_analysis(&state.pool, scope, query.limit.unwrap_or(100))
            .await
            .map_err(grounded_score_store_error)?;
    Ok(no_store(json!(page), StatusCode::OK))
}

async fn get_presentation_package(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let package = presentation_package::build(&state.pool, workspace, situation, analysis)
        .await
        .map_err(presentation_api_error)?;
    Ok(no_store(json!(package), StatusCode::OK))
}

fn git_publication_error(error: GitPublicationError) -> ApiError {
    match error {
        GitPublicationError::Invalid => {
            ApiError(StatusCode::BAD_REQUEST, "invalid git publication request")
        }
        GitPublicationError::Invisible => {
            ApiError(StatusCode::NOT_FOUND, "git publication target unavailable")
        }
        GitPublicationError::Conflict => {
            ApiError(StatusCode::CONFLICT, "git publication ID conflict")
        }
        GitPublicationError::Stale => ApiError(
            StatusCode::CONFLICT,
            "git publication intent is stale; preview and record a new intent",
        ),
        GitPublicationError::Presentation(PresentationError::Manifest(
            crate::analysis_manifest::ManifestError::Invisible,
        )) => ApiError(StatusCode::NOT_FOUND, "published analysis unavailable"),
        GitPublicationError::Presentation(PresentationError::Invalid)
        | GitPublicationError::Presentation(PresentationError::SourceExtractTooLarge)
        | GitPublicationError::Presentation(PresentationError::TooLarge) => ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "presentation unavailable for git planning",
        ),
        GitPublicationError::Presentation(_)
        | GitPublicationError::Database(_)
        | GitPublicationError::Privileged => ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git publication planning unavailable",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitPublicationTargetRequest {
    repository_owner: String,
    repository_name: String,
    ref_name: String,
    repository_visibility: String,
    sensitive_exports_allowed: bool,
}

async fn register_git_publication_target(
    State(state): State<AppState>,
    Path((workspace, target)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<GitPublicationTargetRequest>,
) -> Result<Response, ApiError> {
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let (target, replayed) = git_publication::register_target(
        &state.pool,
        workspace,
        actor,
        &git_publication::TargetInput {
            target_id: target,
            repository_owner: request.repository_owner,
            repository_name: request.repository_name,
            ref_name: request.ref_name,
            repository_visibility: request.repository_visibility,
            sensitive_exports_allowed: request.sensitive_exports_allowed,
        },
    )
    .await
    .map_err(git_publication_error)?;
    Ok(no_store(
        json!({
            "target": target,
            "replayed": replayed,
            "provider_connection_verified": false,
            "provider_secret_stored": false,
            "registration_publishes_nothing": true
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

async fn verify_git_publication_target(
    State(state): State<AppState>,
    Path((workspace, target)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let target = git_publication::get_target(&state.pool, workspace, target)
        .await
        .map_err(git_publication_error)?;
    let provider_token = transient_github_token(&headers)?;
    let verified = tokio::task::spawn_blocking(move || {
        let transport = github_publisher::CurlGitHubTransport::default();
        github_publisher::verify_target(&transport, &target, &provider_token)
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::BAD_GATEWAY,
            "GitHub target verification task failed",
        )
    })?
    .map_err(github_publish_error)?;
    Ok(no_store(
        json!({
            "repository_full_name": verified.repository_full_name,
            "ref_name": verified.ref_name,
            "head_commit_sha": verified.head_commit_sha,
            "repository_private": verified.repository_private,
            "requested_ref_exists": verified.requested_ref_exists,
            "creation_base_ref": verified.creation_base_ref,
            "creation_base_commit_sha": verified.creation_base_commit_sha,
            "remote_changes_performed": false,
            "verification_persisted": false,
            "provider_secret_stored": false
        }),
        StatusCode::OK,
    ))
}

async fn list_git_publication_targets(
    State(state): State<AppState>,
    Path(workspace): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let targets = git_publication::list_targets(&state.pool, workspace)
        .await
        .map_err(git_publication_error)?;
    Ok(no_store(
        json!({
            "targets": targets,
            "provider_connection_verified": false,
            "listing_publishes_nothing": true
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitPublicationPreviewQuery {
    target_id: Uuid,
}

async fn get_git_publication_preview(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<GitPublicationPreviewQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let preview =
        git_publication::preview(&state.pool, workspace, situation, analysis, query.target_id)
            .await
            .map_err(git_publication_error)?;
    Ok(no_store(json!(preview), StatusCode::OK))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitPublicationIntentRequest {
    target_id: Uuid,
}

async fn record_git_publication_intent(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, publication)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<GitPublicationIntentRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    let (intent, replayed) = git_publication::record_intent(
        &state.pool,
        workspace,
        situation,
        analysis,
        actor,
        publication,
        request.target_id,
    )
    .await
    .map_err(git_publication_error)?;
    Ok(no_store(
        json!({
            "intent": intent,
            "replayed": replayed,
            "remote_publish_performed": false,
            "requires_provider_authorization_before_publish": true
        }),
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitPublicationIntentListQuery {
    limit: Option<i64>,
}

async fn list_git_publication_intents(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<GitPublicationIntentListQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let intents = git_publication::list_intents(
        &state.pool,
        workspace,
        situation,
        analysis,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(git_publication_error)?;
    Ok(no_store(
        json!({
            "intents": intents,
            "remote_publish_performed": false
        }),
        StatusCode::OK,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitPublicationRemoteEventListQuery {
    limit: Option<i64>,
}

async fn list_git_publication_remote_events(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, publication)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Query(query): Query<GitPublicationRemoteEventListQuery>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let events = git_publication::list_remote_events(
        &state.pool,
        workspace,
        situation,
        analysis,
        publication,
        query.limit.unwrap_or(25),
    )
    .await
    .map_err(git_publication_error)?;
    Ok(no_store(
        json!({
            "events": events,
            "provider_secret_stored": false,
            "event_payloads_include_provider_response_bodies": false
        }),
        StatusCode::OK,
    ))
}

async fn get_git_publication_remote_status(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, publication)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let status =
        git_publication::remote_status(&state.pool, workspace, situation, analysis, publication)
            .await
            .map_err(git_publication_error)?;
    Ok(no_store(json!(status), StatusCode::OK))
}

fn transient_github_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get("x-niccolo-github-authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError(
            StatusCode::UNAUTHORIZED,
            "transient GitHub authorization required",
        ))?;
    let token = value.strip_prefix("Bearer ").ok_or(ApiError(
        StatusCode::UNAUTHORIZED,
        "transient GitHub authorization required",
    ))?;
    if token.trim().is_empty() || token.len() > 4096 || token.contains(['\r', '\n', '\0']) {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "transient GitHub authorization required",
        ));
    }
    Ok(token.to_owned())
}

fn github_publish_error(error: GitHubPublishError) -> ApiError {
    match error {
        GitHubPublishError::Invalid => ApiError(
            StatusCode::BAD_REQUEST,
            "invalid GitHub publication request",
        ),
        GitHubPublishError::LocalIntegrity => ApiError(
            StatusCode::CONFLICT,
            "publication package changed; preview and record a new intent",
        ),
        GitHubPublishError::Transport
        | GitHubPublishError::ProviderRejected(_)
        | GitHubPublishError::VerificationMismatch
        | GitHubPublishError::InvalidProviderResponse => ApiError(
            StatusCode::BAD_GATEWAY,
            "GitHub publication could not be verified",
        ),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitHubPublishRequest {
    attempt_id: Uuid,
    confirmation: String,
}

async fn publish_git_publication_intent(
    State(state): State<AppState>,
    Path((workspace, situation, analysis, publication)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<GitHubPublishRequest>,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let actor = guard(&state, &headers, workspace, Capability::Write).await?;
    if request.attempt_id.is_nil()
        || request.confirmation != "PUBLISH_STANDARD_PRESENTATION_TO_GITHUB"
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "explicit GitHub publication confirmation required",
        ));
    }

    let existing =
        git_publication::remote_status(&state.pool, workspace, situation, analysis, publication)
            .await
            .map_err(git_publication_error)?;
    if existing.remote_publish_verified {
        return Ok(no_store(
            json!({
                "status": existing,
                "replayed_verified_publication": true,
                "provider_secret_stored": false
            }),
            StatusCode::OK,
        ));
    }

    let provider_token = transient_github_token(&headers)?;
    let material = git_publication::publication_material(
        &state.pool,
        workspace,
        situation,
        analysis,
        publication,
    )
    .await
    .map_err(git_publication_error)?;

    let publish_result = tokio::task::spawn_blocking(move || {
        let transport = github_publisher::CurlGitHubTransport::default();
        github_publisher::publish_standard_package(
            &transport,
            &material.target,
            &material.intent,
            &material.package,
            &provider_token,
        )
    })
    .await
    .map_err(|_| ApiError(StatusCode::BAD_GATEWAY, "GitHub publication task failed"))?;

    let verified = match publish_result {
        Ok(verified) => verified,
        Err(error) => {
            let failure_class = match error {
                GitHubPublishError::LocalIntegrity | GitHubPublishError::Invalid => {
                    "LOCAL_INTEGRITY"
                }
                GitHubPublishError::Transport => "TRANSPORT",
                GitHubPublishError::ProviderRejected(_) => "PROVIDER_REJECTED",
                GitHubPublishError::VerificationMismatch
                | GitHubPublishError::InvalidProviderResponse => "VERIFICATION_MISMATCH",
            };
            let _ = git_publication::record_remote_event(
                &state.pool,
                workspace,
                actor,
                &git_publication::RemoteEventInput {
                    event_id: request.attempt_id,
                    publication_id: publication,
                    kind: git_publication::RemoteEventKind::PublishFailed,
                    remote_commit_sha: None,
                    observed_ref: None,
                    failure_class: Some(failure_class.into()),
                },
            )
            .await;
            return Err(github_publish_error(error));
        }
    };

    for (kind, observed_ref) in [
        (git_publication::RemoteEventKind::PushAccepted, None),
        (
            git_publication::RemoteEventKind::RefVerified,
            Some(verified.ref_name.clone()),
        ),
    ] {
        git_publication::record_remote_event(
            &state.pool,
            workspace,
            actor,
            &git_publication::RemoteEventInput {
                event_id: request.attempt_id,
                publication_id: publication,
                kind,
                remote_commit_sha: Some(verified.commit_sha.clone()),
                observed_ref,
                failure_class: None,
            },
        )
        .await
        .map_err(git_publication_error)?;
    }

    let status =
        git_publication::remote_status(&state.pool, workspace, situation, analysis, publication)
            .await
            .map_err(git_publication_error)?;
    if !status.remote_publish_verified
        || status.verified_commit_sha.as_deref() != Some(verified.commit_sha.as_str())
        || status.verified_ref.as_deref() != Some(verified.ref_name.as_str())
    {
        return Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "remote publication verification ledger mismatch",
        ));
    }
    Ok(no_store(
        json!({
            "status": status,
            "repository_full_name": verified.repository_full_name,
            "package_sha256": verified.package_sha256,
            "remote_commit_already_present": verified.already_present,
            "provider_secret_stored": false,
            "remote_publish_verified": true
        }),
        StatusCode::OK,
    ))
}

async fn get_adversarial_dossier(
    State(state): State<AppState>,
    Path((workspace, situation, analysis)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let scope = crate::analysis_structure::AnalysisScope {
        workspace,
        situation,
        analysis,
    };
    let dossier = adversarial_dossier::build(&state.pool, scope)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "adversarial dossier unavailable",
            )
        })?;
    let dossier_sha256 = adversarial_dossier::sha256(&dossier).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "adversarial dossier unavailable",
        )
    })?;
    Ok(no_store(
        json!({
            "dossier": dossier,
            "dossier_sha256": dossier_sha256,
            "dossier_is_verdict": false,
            "dossier_grants_authority": false,
            "dossier_executes_actions": false
        }),
        StatusCode::OK,
    ))
}

async fn get_bff_operation_budget(
    State(state): State<AppState>,
    Path((workspace, situation, operation)): Path<(Uuid, Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    guard(&state, &headers, workspace, Capability::Read).await?;
    let view = operation_budget::view(
        &state.pool,
        OperationScope {
            workspace,
            situation,
            operation,
        },
    )
    .await
    .map_err(|err| match err {
        BudgetError::Missing => ApiError(StatusCode::NOT_FOUND, "operation unavailable"),
        BudgetError::Invalid => {
            ApiError(StatusCode::UNPROCESSABLE_ENTITY, "invalid operation budget")
        }
        _ => ApiError(StatusCode::INTERNAL_SERVER_ERROR, "budget unavailable"),
    })?;
    Ok(no_store(
        json!({"operation_id":operation,"budget":view}),
        StatusCode::OK,
    ))
}
