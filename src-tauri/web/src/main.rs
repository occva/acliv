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
use reqwest::header as reqwest_header;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::task::spawn_blocking;
use tower_http::services::{ServeDir, ServeFile};

#[path = "../../src/paths.rs"]
mod paths;
#[path = "../../src/search_index/mod.rs"]
mod search_index;
#[path = "../../src/session_manager/mod.rs"]
mod session_manager;

#[derive(Clone)]
struct AppState {
    auth_enabled: bool,
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

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            code: Some("request.not_found"),
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthConfigResponse {
    auth_enabled: bool,
    username: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppVersionInfo {
    version: String,
    runtime: String,
    platform: String,
    arch: String,
    update_channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_ref: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdateCheckResponse {
    runtime: String,
    update_channel: String,
    current_version: String,
    current_image: String,
    current_tag: String,
    current_build_ref: Option<String>,
    latest_tag: String,
    latest_digest: String,
    latest_build_ref: Option<String>,
    update_available: Option<bool>,
    release_url: String,
    update_command: String,
}

#[derive(Debug)]
struct LatestImageInfo {
    digest: String,
    build_ref: Option<String>,
}

#[derive(Debug)]
struct GhcrImageRef {
    repository: String,
}

impl GhcrImageRef {
    fn parse(image: &str) -> Result<Self, AppError> {
        let image = image
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let Some(repository) = image.strip_prefix("ghcr.io/") else {
            return Err(AppError::bad_request(
                "Docker update check only supports ghcr.io images",
            )
            .with_code("update.unsupported_image"));
        };
        let repository = repository
            .split('@')
            .next()
            .unwrap_or(repository)
            .rsplit_once(':')
            .map(|(repo, _tag)| repo)
            .unwrap_or(repository)
            .trim_matches('/');

        if repository.is_empty() || !repository.contains('/') {
            return Err(AppError::bad_request("Invalid ghcr.io image name")
                .with_code("update.invalid_image"));
        }

        Ok(Self {
            repository: repository.to_string(),
        })
    }

    fn manifest_url(&self, reference: &str) -> String {
        format!("https://ghcr.io/v2/{}/manifests/{reference}", self.repository)
    }

    fn blob_url(&self, digest: &str) -> String {
        format!("https://ghcr.io/v2/{}/blobs/{digest}", self.repository)
    }

    fn token_scope(&self) -> String {
        format!("repository:{}:pull", self.repository)
    }

    fn latest_image(&self) -> String {
        format!("ghcr.io/{}:latest", self.repository)
    }
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
struct ProviderPathRequest {
    provider_id: String,
    path: Option<String>,
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
    let auth_enabled = parse_env_bool("ACLIV_WEB_AUTH_ENABLED", true);
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
    let (auth_password, auth_token, password_source) = if auth_enabled {
        let (password, source) = match (env_password, legacy_token.clone()) {
            (Some(password), _) => (password, PasswordSource::Env),
            (None, Some(token)) => (token, PasswordSource::LegacyToken),
            (None, None) => (generate_secret(18), PasswordSource::Generated),
        };
        let token = legacy_token.unwrap_or_else(|| derive_auth_token(&auth_username, &password));
        (password, token, Some(source))
    } else {
        (String::new(), "auth-disabled".to_string(), None)
    };

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
    println!(
        "Web authentication: {}",
        if auth_enabled { "enabled" } else { "disabled" }
    );
    if auth_enabled {
        println!("Web login username: {}", auth_username);
        match password_source {
            Some(PasswordSource::Generated) => {
                println!("Web login password (generated): {}", auth_password);
                println!("Tip: set ACLIV_WEB_PASSWORD to keep a fixed password across restarts.");
            }
            Some(PasswordSource::Env) => {
                println!("Web login password source: ACLIV_WEB_PASSWORD");
            }
            Some(PasswordSource::LegacyToken) => {
                println!("Web login password source: ACLIV_TOKEN (legacy fallback)");
                println!("Tip: set ACLIV_WEB_PASSWORD to migrate away from legacy token login.");
            }
            None => {}
        }
    } else {
        println!("Warning: ACLIV_WEB_AUTH_ENABLED=false. All web routes are publicly accessible.");
    }

    let state = AppState {
        auth_enabled,
        auth_token,
        auth_username,
        auth_password,
        index_html_path: index_html.clone(),
    };
    let protected_routes = Router::new()
        .route("/auth/verify", get(verify_auth))
        .route("/sessions", get(list_sessions))
        .route("/provider/paths", get(list_provider_paths))
        .route("/provider/path", post(set_provider_path))
        .route("/provider/path/reset", post(reset_provider_path))
        .route("/search/index/status", get(get_search_index_status))
        .route("/search/index/changes", get(has_search_index_changes))
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
        .route("/app/version", get(get_app_version))
        .route("/app/update-check", get(check_app_update))
        .route("/auth/config", get(get_auth_config))
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

fn get_env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn get_build_ref() -> Option<String> {
    get_env_string("ACLIV_BUILD_REF").filter(|value| {
        let normalized = value.to_ascii_lowercase();
        normalized != "unknown" && normalized != "local-dev"
    })
}

async fn fetch_latest_image_info(image: &str) -> Result<LatestImageInfo, AppError> {
    let image_ref = GhcrImageRef::parse(image)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("acliv-web-update-check")
        .build()
        .map_err(|e| AppError::internal(format!("Failed to create update client: {e}")))?;
    let token = fetch_ghcr_token(&client, &image_ref).await?;

    let accept_manifest =
        "application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json";
    let manifest_response = client
        .get(image_ref.manifest_url("latest"))
        .header(reqwest_header::ACCEPT, accept_manifest)
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Docker latest check failed: {e}")))?;

    if !manifest_response.status().is_success() {
        return Err(AppError::internal(format!(
            "Docker latest check failed: {}",
            manifest_response.status()
        )));
    }

    let latest_digest = manifest_response
        .headers()
        .get("docker-content-digest")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::internal("Docker latest digest is missing"))?;
    let manifest = manifest_response
        .json::<Value>()
        .await
        .map_err(|e| AppError::internal(format!("Docker latest manifest is invalid: {e}")))?;
    let config_digest = find_config_digest(&client, &token, &image_ref, &manifest).await?;
    let build_ref = if let Some(config_digest) = config_digest {
        fetch_image_build_ref(&client, &token, &image_ref, &config_digest).await?
    } else {
        None
    };

    Ok(LatestImageInfo {
        digest: latest_digest,
        build_ref,
    })
}

async fn fetch_ghcr_token(
    client: &reqwest::Client,
    image_ref: &GhcrImageRef,
) -> Result<String, AppError> {
    let scope = image_ref.token_scope();
    let response = client
        .get("https://ghcr.io/token")
        .query(&[("service", "ghcr.io"), ("scope", scope.as_str())])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Docker registry auth failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::internal(format!(
            "Docker registry auth failed: {}",
            response.status()
        )));
    }

    let payload = response.json::<Value>().await.map_err(|e| {
        AppError::internal(format!("Docker registry auth response is invalid: {e}"))
    })?;
    payload
        .get("token")
        .or_else(|| payload.get("access_token"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| AppError::internal("Docker registry auth token is missing"))
}

async fn find_config_digest(
    client: &reqwest::Client,
    token: &str,
    image_ref: &GhcrImageRef,
    manifest: &Value,
) -> Result<Option<String>, AppError> {
    if let Some(config_digest) = manifest
        .get("config")
        .and_then(|config| config.get("digest"))
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        return Ok(Some(config_digest));
    }

    let child_digest = manifest
        .get("manifests")
        .and_then(Value::as_array)
        .and_then(|manifests| {
            manifests
                .iter()
                .find(|entry| {
                    entry
                        .get("platform")
                        .and_then(|platform| platform.get("os"))
                        .and_then(Value::as_str)
                        == Some("linux")
                        && entry
                            .get("platform")
                            .and_then(|platform| platform.get("architecture"))
                            .and_then(Value::as_str)
                            == Some("amd64")
                })
                .or_else(|| manifests.first())
        })
        .and_then(|entry| entry.get("digest"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let Some(child_digest) = child_digest else {
        return Ok(None);
    };

    let response = client
        .get(image_ref.manifest_url(&child_digest))
        .header(
            reqwest_header::ACCEPT,
            "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json",
        )
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Docker image manifest check failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::internal(format!(
            "Docker image manifest check failed: {}",
            response.status()
        )));
    }

    let child_manifest = response
        .json::<Value>()
        .await
        .map_err(|e| AppError::internal(format!("Docker image manifest is invalid: {e}")))?;

    Ok(child_manifest
        .get("config")
        .and_then(|config| config.get("digest"))
        .and_then(Value::as_str)
        .map(str::to_string))
}

async fn fetch_image_build_ref(
    client: &reqwest::Client,
    token: &str,
    image_ref: &GhcrImageRef,
    config_digest: &str,
) -> Result<Option<String>, AppError> {
    let config_response = client
        .get(image_ref.blob_url(config_digest))
        .header(
            reqwest_header::ACCEPT,
            "application/vnd.oci.image.config.v1+json",
        )
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Docker image config check failed: {e}")))?;

    if !config_response.status().is_success() {
        return Err(AppError::internal(format!(
            "Docker image config check failed: {}",
            config_response.status()
        )));
    }

    let config = config_response
        .json::<Value>()
        .await
        .map_err(|e| AppError::internal(format!("Docker image config is invalid: {e}")))?;

    Ok(config
        .get("config")
        .and_then(|config| config.get("Env"))
        .and_then(Value::as_array)
        .and_then(|env| find_env_value(env, "ACLIV_BUILD_REF")))
}

fn find_env_value(values: &[Value], key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    values
        .iter()
        .filter_map(Value::as_str)
        .find_map(|value| value.strip_prefix(&prefix))
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

fn parse_env_bool(key: &str, default: bool) -> bool {
    let Ok(value) = env::var(key) else {
        return default;
    };

    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => default,
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => {
            println!(
                "Invalid {key}={value:?}. Expected one of 1/0/true/false/yes/no/on/off. Using default: {default}."
            );
            default
        }
    }
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
    if !state.auth_enabled {
        return Ok(Json(ApiResult {
            ok: true,
            data: AuthLoginResponse {
                token: state.auth_token.clone(),
                username: state.auth_username.clone(),
            },
        }));
    }

    validate_non_empty("username", &payload.username)?;
    validate_non_empty("password", &payload.password)?;

    let username = payload.username.trim();
    let password = payload.password.trim();
    if username != state.auth_username || password != state.auth_password {
        return Err(AppError::unauthorized("Invalid username or password")
            .with_code("auth.invalid_credentials"));
    }

    Ok(Json(ApiResult {
        ok: true,
        data: AuthLoginResponse {
            token: state.auth_token.clone(),
            username: state.auth_username.clone(),
        },
    }))
}

async fn get_auth_config(State(state): State<AppState>) -> Json<ApiResult<AuthConfigResponse>> {
    Json(ApiResult {
        ok: true,
        data: AuthConfigResponse {
            auth_enabled: state.auth_enabled,
            username: state.auth_username,
        },
    })
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
    if !state.auth_enabled {
        return next.run(request).await;
    }

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

async fn get_app_version() -> Json<ApiResult<AppVersionInfo>> {
    Json(ApiResult {
        ok: true,
        data: AppVersionInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            runtime: "web".to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            update_channel: "docker-image".to_string(),
            image: get_env_string("ACLIV_IMAGE")
                .or_else(|| Some("ghcr.io/occva/acliv".to_string())),
            image_tag: get_env_string("ACLIV_IMAGE_TAG")
                .or_else(|| get_env_string("ACLIV_VERSION"))
                .or_else(|| Some("latest".to_string())),
            build_ref: get_build_ref(),
        },
    })
}

async fn check_app_update() -> Result<Json<ApiResult<AppUpdateCheckResponse>>, AppError> {
    let image = get_env_string("ACLIV_IMAGE").unwrap_or_else(|| "ghcr.io/occva/acliv".to_string());
    let latest_image = GhcrImageRef::parse(&image)?.latest_image();
    let current_tag = get_env_string("ACLIV_IMAGE_TAG")
        .or_else(|| get_env_string("ACLIV_VERSION"))
        .unwrap_or_else(|| "latest".to_string());
    let current_build_ref = get_build_ref();
    let latest = fetch_latest_image_info(&image).await?;
    let update_available = match (&current_build_ref, &latest.build_ref) {
        (Some(current), Some(latest_ref)) => Some(current != latest_ref),
        _ if current_tag == "latest" => None,
        _ => Some(true),
    };

    Ok(Json(ApiResult {
        ok: true,
        data: AppUpdateCheckResponse {
            runtime: "web".to_string(),
            update_channel: "docker-image".to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            current_image: image.clone(),
            current_tag,
            current_build_ref,
            latest_tag: "latest".to_string(),
            latest_digest: latest.digest,
            latest_build_ref: latest.build_ref,
            update_available,
            release_url: "https://github.com/occva/acliv/releases/latest".to_string(),
            update_command: format!("docker pull {latest_image} && docker compose up -d"),
        },
    }))
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

async fn list_provider_paths() -> Result<Json<ApiResult<Vec<paths::ProviderPathInfo>>>, AppError> {
    let paths = spawn_blocking(paths::list_provider_paths)
        .await
        .map_err(|e| AppError::internal(format!("Failed to list provider paths: {e}")))?;

    Ok(Json(ApiResult {
        ok: true,
        data: paths,
    }))
}

async fn set_provider_path(
    Json(payload): Json<ProviderPathRequest>,
) -> Result<Json<ApiResult<paths::ProviderPathInfo>>, AppError> {
    validate_non_empty("providerId", &payload.provider_id)?;
    let path = payload.path.unwrap_or_default();
    validate_non_empty("path", &path)?;

    let provider_id = payload.provider_id.trim().to_string();
    let info = spawn_blocking(move || paths::set_provider_path(&provider_id, &path))
        .await
        .map_err(|e| AppError::internal(format!("Failed to save provider path: {e}")))?
        .map_err(map_domain_error)?;

    Ok(Json(ApiResult {
        ok: true,
        data: info,
    }))
}

async fn reset_provider_path(
    Json(payload): Json<ProviderPathRequest>,
) -> Result<Json<ApiResult<paths::ProviderPathInfo>>, AppError> {
    validate_non_empty("providerId", &payload.provider_id)?;

    let provider_id = payload.provider_id.trim().to_string();
    let info = spawn_blocking(move || paths::reset_provider_path(&provider_id))
        .await
        .map_err(|e| AppError::internal(format!("Failed to reset provider path: {e}")))?
        .map_err(map_domain_error)?;

    Ok(Json(ApiResult {
        ok: true,
        data: info,
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

async fn has_search_index_changes(
) -> Result<Json<ApiResult<search_index::SearchIndexChangeStatus>>, AppError> {
    let status = spawn_blocking(search_index::has_index_changes)
        .await
        .map_err(|e| AppError::internal(format!("Failed to check search index changes: {e}")))?
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
    .map_err(map_search_index_error)?;

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

fn map_search_index_error(error: search_index::SearchIndexError) -> AppError {
    match error {
        search_index::SearchIndexError::NotFound(message) => AppError::not_found(message),
        search_index::SearchIndexError::Internal(message) => AppError::internal(message),
    }
}
