use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::{
    extract::{Path as AxumPath, State},
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, get_service, post},
    Json, Router,
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::task::spawn_blocking;
use tower_http::services::{ServeDir, ServeFile};

#[path = "../paths.rs"]
mod paths;
#[path = "../search_index/mod.rs"]
mod search_index;
#[path = "../session_manager/mod.rs"]
mod session_manager;

#[derive(Clone)]
struct AppState {
    auth_token: String,
    auth_username: String,
    auth_password: String,
    index_html_path: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiResult<T> {
    ok: bool,
    data: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiErrorBody {
    ok: bool,
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
    code: Option<&'static str>,
}

impl AppError {
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            code: None,
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            code: Some("request.bad_request"),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            code: None,
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            code: Some("request.internal_error"),
        }
    }

    fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(ApiErrorBody {
            ok: false,
            error: self.message,
            code: self.code,
        });
        (self.status, body).into_response()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionMessagesRequest {
    provider_id: String,
    source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthLoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthLoginResponse {
    token: String,
    username: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthVerifyResponse {
    username: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteSessionRequest {
    provider_id: String,
    session_id: String,
    source_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchContentRequest {
    query: String,
    provider_id: Option<String>,
    limit: Option<u32>,
    since_ts: Option<i64>,
    project_path: Option<String>,
    sort_by: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexedSessionsQuery {
    provider_id: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexedSessionsPageQuery {
    provider_id: Option<String>,
    project_path: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexedSessionsByPathsRequest {
    provider_id: String,
    source_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    ok: bool,
}

enum PasswordSource {
    Env,
    LegacyToken,
    Generated,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = env::var("ACLIV_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("ACLIV_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(17860);
    let auth_username = env::var("ACLIV_WEB_USERNAME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "admin".to_string());
    let env_password = env::var("ACLIV_WEB_PASSWORD")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let legacy_token = env::var("ACLIV_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let (auth_password, password_source) = match (env_password, legacy_token.clone()) {
        (Some(password), _) => (password, PasswordSource::Env),
        (None, Some(token)) => (token, PasswordSource::LegacyToken),
        (None, None) => (generate_secret(18), PasswordSource::Generated),
    };
    let auth_token =
        legacy_token.unwrap_or_else(|| derive_auth_token(&auth_username, &auth_password));

    let socket: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| format!("Invalid address {host}:{port}: {e}"))?;
    let frontend_dist = resolve_frontend_dist();
    let index_html = frontend_dist.join("index.html");
    if !index_html.exists() {
        return Err(format!(
            "Frontend dist not found. Expected file: {}. Run `npm run build` first.",
            index_html.display()
        )
        .into());
    }
    println!("ACLIV (Web)");
    println!("Listening on: http://{host}:{port}");
    println!("Frontend dist: {}", frontend_dist.display());
    println!("Web login username: {}", auth_username);
    match password_source {
        PasswordSource::Generated => {
            println!("Web login password (generated): {}", auth_password);
            println!("Tip: set ACLIV_WEB_PASSWORD to keep a fixed password across restarts.");
        }
        PasswordSource::Env => {
            println!("Web login password source: ACLIV_WEB_PASSWORD");
        }
        PasswordSource::LegacyToken => {
            println!("Web login password source: ACLIV_TOKEN (legacy fallback)");
            println!("Tip: set ACLIV_WEB_PASSWORD to migrate away from legacy token login.");
        }
    }

    let state = AppState {
        auth_token,
        auth_username,
        auth_password,
        index_html_path: index_html.clone(),
    };
    let protected_routes = Router::new()
        .route("/auth/verify", get(verify_auth))
        .route("/sessions", get(list_sessions))
        .route("/search/index/status", get(get_search_index_status))
        .route("/search/index/rebuild", post(rebuild_search_index))
        .route("/search/index/refresh", post(refresh_search_index))
        .route("/search/content", post(search_content))
        .route("/search/index/sessions", get(list_indexed_sessions))
        .route(
            "/search/index/sessions/page",
            get(list_indexed_sessions_page),
        )
        .route("/search/index/projects", get(list_indexed_projects))
        .route(
            "/search/index/sessions/by-paths",
            post(list_indexed_sessions_by_source_paths),
        )
        .route(
            "/search/index/session/messages",
            post(get_indexed_session_messages),
        )
        .route("/session/messages", post(get_session_messages))
        .route("/session/delete", post(delete_session))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));
    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/auth/login", post(login_auth))
        .merge(protected_routes);

    let static_service = ServeDir::new(&frontend_dist).append_index_html_on_directories(true);
    let icon_file = frontend_dist.join("icon.png");

    let app = Router::new()
        .nest("/api", api_routes)
        .route("/", get(serve_spa_shell))
        .route("/icon.png", get_service(ServeFile::new(icon_file)))
        .route("/:session_id", get(serve_session_spa_shell))
        .fallback_service(static_service)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(socket).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn resolve_frontend_dist() -> PathBuf {
    if let Ok(value) = env::var("ACLIV_FRONTEND_DIST") {
        let path = Path::new(value.trim());
        if !value.trim().is_empty() {
            return path.to_path_buf();
        }
    }

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let dist = cwd.join("dist");
    if dist.exists() {
        return dist;
    }

    cwd.join("../dist")
}

fn load_spa_shell(index_html: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let html = std::fs::read_to_string(index_html)?;
    if html.contains("<base ") {
        return Ok(html);
    }

    if let Some(head_index) = html.find("<head>") {
        let insert_at = head_index + "<head>".len();
        let mut patched = String::with_capacity(html.len() + 24);
        patched.push_str(&html[..insert_at]);
        patched.push_str("\n    <base href=\"/\" />");
        patched.push_str(&html[insert_at..]);
        return Ok(patched);
    }

    Ok(html)
}

fn generate_secret(len: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut rng = thread_rng();
    (0..len)
        .map(|_| {
            let index = rng.gen_range(0..CHARSET.len());
            CHARSET[index] as char
        })
        .collect()
}

fn derive_auth_token(username: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"acliv-web-auth-token:v1\0");
    hasher.update(username.as_bytes());
    hasher.update(b"\0");
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn render_spa_shell(index_html: &Path) -> Result<Html<String>, AppError> {
    let html = load_spa_shell(index_html)
        .map_err(|e| AppError::internal(format!("Failed to load SPA shell: {e}")))?;
    Ok(Html(html))
}

async fn serve_spa_shell(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    render_spa_shell(&state.index_html_path)
}

async fn serve_session_spa_shell(
    AxumPath(_session_id): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<Html<String>, AppError> {
    render_spa_shell(&state.index_html_path)
}

async fn login_auth(
    State(state): State<AppState>,
    Json(payload): Json<AuthLoginRequest>,
) -> Result<Json<ApiResult<AuthLoginResponse>>, AppError> {
    validate_non_empty("username", &payload.username)?;
    validate_non_empty("password", &payload.password)?;

    let username = payload.username.trim();
    let password = payload.password.trim();
    if username != state.auth_username || password != state.auth_password {
        return Err(
            AppError::unauthorized("Invalid username or password")
                .with_code("auth.invalid_credentials"),
        );
    }

    Ok(Json(ApiResult {
        ok: true,
        data: AuthLoginResponse {
            token: state.auth_token.clone(),
            username: state.auth_username.clone(),
        },
    }))
}

async fn verify_auth(
    State(state): State<AppState>,
) -> Result<Json<ApiResult<AuthVerifyResponse>>, AppError> {
    Ok(Json(ApiResult {
        ok: true,
        data: AuthVerifyResponse {
            username: state.auth_username.clone(),
        },
    }))
}

async fn require_auth(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == state.auth_token)
        .unwrap_or(false);

    if !authorized {
        return AppError::unauthorized("Unauthorized")
            .with_code("auth.missing_token")
            .into_response();
    }

    next.run(request).await
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn list_sessions() -> Result<Json<ApiResult<Vec<session_manager::SessionMeta>>>, AppError> {
    let sessions = spawn_blocking(session_manager::scan_sessions)
        .await
        .map_err(|e| AppError::internal(format!("Failed to scan sessions: {e}")))?;

    Ok(Json(ApiResult {
        ok: true,
        data: sessions,
    }))
}

async fn get_search_index_status(
) -> Result<Json<ApiResult<search_index::SearchIndexStatus>>, AppError> {
    let status = spawn_blocking(search_index::get_index_status)
        .await
        .map_err(|e| AppError::internal(format!("Failed to load search index status: {e}")))?
        .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: status,
    }))
}

async fn rebuild_search_index(
) -> Result<Json<ApiResult<search_index::RebuildSearchIndexResult>>, AppError> {
    let result = spawn_blocking(search_index::rebuild_index)
        .await
        .map_err(|e| AppError::internal(format!("Failed to rebuild search index: {e}")))?
        .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: result,
    }))
}

async fn refresh_search_index(
) -> Result<Json<ApiResult<search_index::RefreshSearchIndexResult>>, AppError> {
    let result = spawn_blocking(search_index::refresh_index)
        .await
        .map_err(|e| AppError::internal(format!("Failed to refresh search index: {e}")))?
        .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: result,
    }))
}

async fn search_content(
    Json(payload): Json<SearchContentRequest>,
) -> Result<Json<ApiResult<search_index::SearchContentResult>>, AppError> {
    validate_non_empty("query", &payload.query)?;

    let query = payload.query.trim().to_string();
    let provider_id = payload.provider_id.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let project_path = payload.project_path.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let limit = usize::try_from(payload.limit.unwrap_or(50)).unwrap_or(50);

    let since_ts = payload.since_ts;
    let hits = spawn_blocking(move || {
        search_index::search_content(
            &query,
            limit,
            provider_id.as_deref(),
            since_ts,
            project_path.as_deref(),
            payload.sort_by.as_deref(),
        )
    })
    .await
    .map_err(|e| AppError::internal(format!("Failed to search indexed content: {e}")))?
    .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: hits,
    }))
}

async fn list_indexed_sessions(
    axum::extract::Query(query): axum::extract::Query<IndexedSessionsQuery>,
) -> Result<Json<ApiResult<Vec<search_index::IndexedSession>>>, AppError> {
    let provider_id = query.provider_id.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let limit = usize::try_from(query.limit.unwrap_or(200)).unwrap_or(200);

    let sessions =
        spawn_blocking(move || search_index::list_indexed_sessions(limit, provider_id.as_deref()))
            .await
            .map_err(|e| AppError::internal(format!("Failed to list indexed sessions: {e}")))?
            .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: sessions,
    }))
}

async fn list_indexed_sessions_page(
    axum::extract::Query(query): axum::extract::Query<IndexedSessionsPageQuery>,
) -> Result<Json<ApiResult<search_index::PagedIndexedSessionsResult>>, AppError> {
    let provider_id = query.provider_id.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let project_path = query.project_path.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let limit = usize::try_from(query.limit.unwrap_or(50)).unwrap_or(50);
    let offset = usize::try_from(query.offset.unwrap_or(0)).unwrap_or(0);

    let result = spawn_blocking(move || {
        search_index::list_indexed_sessions_page(
            limit,
            offset,
            provider_id.as_deref(),
            project_path.as_deref(),
        )
    })
    .await
    .map_err(|e| AppError::internal(format!("Failed to list paged indexed sessions: {e}")))?
    .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: result,
    }))
}

async fn list_indexed_projects(
    axum::extract::Query(query): axum::extract::Query<IndexedSessionsQuery>,
) -> Result<Json<ApiResult<Vec<search_index::IndexedProjectOption>>>, AppError> {
    let provider_id = query.provider_id.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let projects =
        spawn_blocking(move || search_index::list_indexed_projects(provider_id.as_deref()))
            .await
            .map_err(|e| AppError::internal(format!("Failed to list indexed projects: {e}")))?
            .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: projects,
    }))
}

async fn list_indexed_sessions_by_source_paths(
    Json(payload): Json<IndexedSessionsByPathsRequest>,
) -> Result<Json<ApiResult<Vec<search_index::IndexedSession>>>, AppError> {
    validate_non_empty("providerId", &payload.provider_id)?;

    let provider_id = payload.provider_id.trim().to_string();
    let source_paths = payload
        .source_paths
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    let sessions = spawn_blocking(move || {
        search_index::list_indexed_sessions_by_source_paths(&provider_id, &source_paths)
    })
    .await
    .map_err(|e| {
        AppError::internal(format!(
            "Failed to list indexed sessions by source paths: {e}"
        ))
    })?
    .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: sessions,
    }))
}

async fn get_indexed_session_messages(
    Json(payload): Json<SessionMessagesRequest>,
) -> Result<Json<ApiResult<Vec<search_index::IndexedMessage>>>, AppError> {
    validate_non_empty("providerId", &payload.provider_id)?;
    validate_non_empty("sourcePath", &payload.source_path)?;

    let provider_id = payload.provider_id.trim().to_string();
    let source_path = payload.source_path.trim().to_string();
    let messages = spawn_blocking(move || {
        search_index::get_indexed_session_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|e| AppError::internal(format!("Failed to load indexed session messages: {e}")))?
    .map_err(AppError::internal)?;

    Ok(Json(ApiResult {
        ok: true,
        data: messages,
    }))
}

async fn get_session_messages(
    Json(payload): Json<SessionMessagesRequest>,
) -> Result<Json<ApiResult<Vec<session_manager::SessionMessage>>>, AppError> {
    validate_non_empty("providerId", &payload.provider_id)?;
    validate_non_empty("sourcePath", &payload.source_path)?;

    let provider_id = payload.provider_id.clone();
    let source_path = payload.source_path.clone();
    let messages =
        spawn_blocking(move || session_manager::load_messages(&provider_id, &source_path))
            .await
            .map_err(|e| AppError::internal(format!("Failed to load session messages: {e}")))?
            .map_err(map_domain_error)?;

    Ok(Json(ApiResult {
        ok: true,
        data: messages,
    }))
}

async fn delete_session(
    Json(payload): Json<DeleteSessionRequest>,
) -> Result<Json<ApiResult<bool>>, AppError> {
    validate_non_empty("providerId", &payload.provider_id)?;
    validate_non_empty("sessionId", &payload.session_id)?;
    validate_non_empty("sourcePath", &payload.source_path)?;

    let provider_id = payload.provider_id.clone();
    let session_id = payload.session_id.clone();
    let source_path = payload.source_path.clone();

    let ok = spawn_blocking(move || {
        let deleted = session_manager::delete_session(&provider_id, &session_id, &source_path)?;
        if deleted {
            let _ = search_index::delete_indexed_session(&provider_id, &source_path);
        }
        Ok(deleted)
    })
    .await
    .map_err(|e| AppError::internal(format!("Failed to delete session: {e}")))?
    .map_err(map_domain_error)?;

    Ok(Json(ApiResult { ok: true, data: ok }))
}

fn validate_non_empty(label: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        let code = if matches!(label, "username" | "password") {
            "auth.missing_credentials"
        } else {
            "request.bad_request"
        };
        return Err(AppError::bad_request(format!("{label} is required")).with_code(code));
    }
    Ok(())
}

fn map_domain_error(message: String) -> AppError {
    if message.contains("outside provider root") {
        AppError::forbidden(message).with_code("request.path_outside_provider_root")
    } else {
        AppError::bad_request(message)
    }
}
