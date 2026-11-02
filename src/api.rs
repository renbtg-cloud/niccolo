//! Bound, explicit HTTP boundary. Not a public Internet deployment template.
//! AuthN/AuthZ decision precedes EVERY workspace read and write. The client
//! may select a workspace but can never authorize itself by merely naming it.
use crate::authz::{self, AuthError, Capability};
use crate::postgres_ingest::{self, ImportError};
use crate::postgres_repository::{self, RepositoryError};
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

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/healthz", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/readyz", get(readiness))
        .route("/v1/workspaces/{workspace}/graph", get(graph))
        .route("/v1/workspaces/{workspace}/assess", post(assess))
        .route("/v1/workspaces/{workspace}/sources", get(list_sources))
        .route(
            "/v1/workspaces/{workspace}/sources/{document}",
            post(ingest),
        )
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(AppState { pool })
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
    let raw = headers
        .get(header::AUTHORIZATION)
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "missing bearer token"))?
        .to_str()
        .map_err(|_| ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))?;
    raw.strip_prefix("Bearer ")
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "invalid bearer token"))
}
async fn guard(
    state: &AppState,
    headers: &HeaderMap,
    workspace: Uuid,
    capability: Capability,
) -> Result<(), ApiError> {
    let token = bearer(headers)?;
    match authz::authorize(&state.pool, token, workspace, capability).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ApiError(StatusCode::FORBIDDEN, "workspace access denied")),
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
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(1) => no_store(json!({"status":"ready"}), StatusCode::OK),
        _ => no_store(
            json!({"status":"unavailable"}),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
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
