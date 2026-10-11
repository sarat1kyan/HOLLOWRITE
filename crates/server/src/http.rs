use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hollowrite_persistence::{DbError, SessionInfo};
use hollowrite_protocol::{validate_name, MAX_CLIENT_FRAME_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;

pub const SESSION_COOKIE: &str = "hw_session";
pub const CSRF_HEADER: &str = "x-hollowrite";
const MIN_PASSWORD: usize = 10;
const MAX_PASSWORD: usize = 128;
const LOCKOUT_THRESHOLD: i32 = 5;
const LOCKOUT: Duration = Duration::from_secs(5 * 60);

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/ws", get(ws));
    if let Some(dir) = &state.config.web_dir {
        let index = dir.join("index.html");
        app = app.fallback_service(
            tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)),
        );
    }
    app.layer(axum::middleware::from_fn(security_headers)).with_state(state)
}

async fn security_headers(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
             font-src 'self' https://fonts.gstatic.com; connect-src 'self' ws: wss:; img-src 'self' data:; \
             object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    res
}

fn api_error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

async fn healthz() -> &'static str {
    "ok"
}

async fn readyz(State(s): State<AppState>) -> Response {
    let beat = s.metrics.world_heartbeat_ms.load(Ordering::Relaxed) as i64;
    let world_ok = chrono::Utc::now().timestamp_millis() - beat < 2_000;
    let db_ok = tokio::time::timeout(Duration::from_secs(2), s.db.ping()).await.is_ok_and(|r| r.is_ok());
    let status = if world_ok && db_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "world": world_ok, "database": db_ok }))).into_response()
}

async fn metrics(State(s): State<AppState>, ConnectInfo(addr): ConnectInfo<SocketAddr>) -> Response {
    if !addr.ip().is_loopback() && !s.config.public_metrics {
        return api_error(StatusCode::FORBIDDEN, "metrics are only served on loopback");
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], s.metrics.render()).into_response()
}

/// Rejects cross-site form posts: a custom header cannot be attached by a
/// foreign origin without a CORS preflight, which this server never grants.
fn csrf_ok(s: &AppState, headers: &HeaderMap) -> bool {
    if headers.get(CSRF_HEADER).is_none() {
        return false;
    }
    match headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()) {
        Some(origin) => s.config.allowed_origins.iter().any(|a| a == origin),
        None => true,
    }
}

pub fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == SESSION_COOKIE)
        .map(|(_, v)| v.to_string())
}

pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim().to_string())
}

fn session_cookie(s: &AppState, token: &str, max_age: u64) -> HeaderValue {
    let secure = if s.config.secure_cookies { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}"
    ))
    .unwrap()
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
    #[serde(default)]
    client: Option<String>,
}

#[derive(Serialize)]
struct AuthOk {
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

fn client_kind(c: &Option<String>) -> &'static str {
    match c.as_deref() {
        Some("terminal") => "terminal",
        Some("bot") => "bot",
        _ => "web",
    }
}

pub async fn hash_password(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string())
    })
    .await?
    .map_err(|e| anyhow::anyhow!("hashing failed: {e}"))
}

async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
    })
    .await
    .unwrap_or(false)
}

async fn issue_session(s: &AppState, account: uuid::Uuid, username: String, client: &'static str) -> Response {
    let ttl = Duration::from_secs(s.config.session_days * 86_400);
    let token = match s.db.create_session(account, client, ttl).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "creating session failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "could not create a session");
        }
    };
    if client == "web" {
        let mut res = Json(AuthOk { username, token: None }).into_response();
        res.headers_mut().insert(header::SET_COOKIE, session_cookie(s, &token, ttl.as_secs()));
        res
    } else {
        Json(AuthOk { username, token: Some(token) }).into_response()
    }
}

async fn register(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> Response {
    if !csrf_ok(&s, &headers) {
        return api_error(StatusCode::FORBIDDEN, "missing request header or bad origin");
    }
    if !s.limits.register.check(&addr.ip().to_string()) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "too many registrations from this address; try later");
    }
    if let Err(e) = validate_name(&body.username) {
        return api_error(StatusCode::BAD_REQUEST, e);
    }
    let len = body.password.chars().count();
    if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&len) {
        return api_error(StatusCode::BAD_REQUEST, "password must be 10-128 characters");
    }
    if body.password.eq_ignore_ascii_case(&body.username) {
        return api_error(StatusCode::BAD_REQUEST, "password must differ from the username");
    }
    let hash = match hash_password(body.password).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "password hashing failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "registration failed");
        }
    };
    match s.db.create_account(&body.username, &hash).await {
        Ok(id) => {
            tracing::info!(account = %id, "account registered");
            issue_session(&s, id, body.username, client_kind(&body.client)).await
        }
        Err(DbError::Conflict) => api_error(StatusCode::CONFLICT, "that name is taken"),
        Err(e) => {
            tracing::error!(error = %e, "registration failed");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "registration failed")
        }
    }
}

async fn login(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> Response {
    if !csrf_ok(&s, &headers) {
        return api_error(StatusCode::FORBIDDEN, "missing request header or bad origin");
    }
    let user_key = body.username.to_ascii_lowercase();
    if !s.limits.login_ip.check(&addr.ip().to_string()) || !s.limits.login_user.check(&user_key) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "too many attempts; wait a minute");
    }
    if body.password.len() > MAX_PASSWORD * 4 {
        return api_error(StatusCode::UNAUTHORIZED, "wrong name or password");
    }
    let account = match s.db.account_by_name(&body.username).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = %e, "login lookup failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "login failed");
        }
    };
    let Some(account) = account else {
        // Burn the same CPU as a real check so response time does not reveal
        // which names exist.
        let _ = verify_password(body.password, s.dummy_hash.clone()).await;
        return api_error(StatusCode::UNAUTHORIZED, "wrong name or password");
    };
    let now = chrono::Utc::now();
    if account.locked_until.is_some_and(|t| t > now) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "account temporarily locked after failed attempts");
    }
    if !verify_password(body.password, account.password_hash.clone()).await {
        let _ = s.db.login_failed(account.id, LOCKOUT_THRESHOLD, LOCKOUT).await;
        tracing::info!(account = %account.id, "failed login");
        return api_error(StatusCode::UNAUTHORIZED, "wrong name or password");
    }
    if account.suspended_until.is_some_and(|t| t > now) {
        return api_error(StatusCode::FORBIDDEN, "this account is suspended");
    }
    let _ = s.db.login_succeeded(account.id).await;
    issue_session(&s, account.id, account.username, client_kind(&body.client)).await
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if !csrf_ok(&s, &headers) {
        return api_error(StatusCode::FORBIDDEN, "missing request header or bad origin");
    }
    if let Some(token) = bearer_token(&headers).or_else(|| cookie_token(&headers)) {
        let _ = s.db.revoke_session(&token).await;
    }
    let mut res = Json(json!({ "ok": true })).into_response();
    res.headers_mut().insert(header::SET_COOKIE, session_cookie(&s, "", 0));
    res
}

pub async fn authenticate(s: &AppState, headers: &HeaderMap) -> Option<SessionInfo> {
    let token = bearer_token(headers).or_else(|| cookie_token(headers))?;
    if token.len() > 128 {
        return None;
    }
    s.db.session(&token).await.ok().flatten()
}

async fn me(State(s): State<AppState>, headers: HeaderMap) -> Response {
    match authenticate(&s, &headers).await {
        Some(info) => Json(json!({ "username": info.username })).into_response(),
        None => api_error(StatusCode::UNAUTHORIZED, "not signed in"),
    }
}

/// WebSocket entry. Browsers always send `Origin`, so a request carrying one
/// must come from an allowed origin and authenticates with the cookie.
/// Native clients send no `Origin` and must present a bearer token; cookies
/// are ignored for them, which closes cross-site WebSocket hijacking.
async fn ws(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let origin = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok());
    let token = match origin {
        Some(o) => {
            if !s.config.allowed_origins.iter().any(|a| a == o) {
                tracing::warn!(origin = o, "websocket from disallowed origin");
                return api_error(StatusCode::FORBIDDEN, "origin not allowed");
            }
            cookie_token(&headers)
        }
        None => bearer_token(&headers),
    };
    if !s.limits.ws_connect.check(&addr.ip().to_string()) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "too many connections");
    }
    let Some(token) = token.filter(|t| t.len() <= 128) else {
        return api_error(StatusCode::UNAUTHORIZED, "not signed in");
    };
    let Some(session) = s.db.session(&token).await.ok().flatten() else {
        return api_error(StatusCode::UNAUTHORIZED, "session expired");
    };
    upgrade
        .max_message_size(MAX_CLIENT_FRAME_BYTES)
        .max_frame_size(MAX_CLIENT_FRAME_BYTES)
        .on_upgrade(move |socket| crate::ws::serve(socket, s, session, addr))
}
