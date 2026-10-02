//HTTP routes, JSON errors, the per-request JSONL line and the CORS layer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

use crate::auth::{self, AuthError};
use crate::burn::burn;
use crate::state::AppState;

#[derive(Clone)]
struct LevelName(String);

#[derive(Serialize)]
struct Report<'a> {
    level: &'a str,
    pod: &'a str,
    cpu_ms: u64,
    mem_mib: u64,
    elapsed_ms: u64,
}

pub fn router(state: AppState) -> Router {
    let cors = cors(&state.config.cors_allowed_origins);
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/stress", get(levels))
        .route("/api/v1/stress/{level}", post(stress))
        .with_state(Arc::new(state))
        .layer(cors)
        .layer(middleware::from_fn(log_request))
}

async fn healthz() -> Response {
    Json(json!({"status": "ok"})).into_response()
}

async fn levels(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match auth::check(&state, &headers).await {
        Ok(()) => Json(&state.levels).into_response(),
        Err(error) => auth_failure(error),
    }
}

async fn stress(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    let mut response = run(&state, &headers, &name).await;
    response.extensions_mut().insert(LevelName(name));
    response
}

async fn run(state: &AppState, headers: &HeaderMap, name: &str) -> Response {
    if let Err(error) = auth::check(state, headers).await {
        return auth_failure(error);
    }
    let Some(level) = state.levels.find(name) else {
        return failure(StatusCode::NOT_FOUND, "unknown level");
    };
    let Ok(permit) = state.burns.clone().try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "busy");
    };
    let (cpu_ms, mem_mib) = (level.cpu_ms, level.mem_mib);
    let burned = tokio::task::spawn_blocking(move || {
        let elapsed_ms = burn(cpu_ms, mem_mib);
        drop(permit);
        elapsed_ms
    })
    .await;
    match burned {
        Ok(elapsed_ms) => Json(Report {
            level: name,
            pod: state.pod(),
            cpu_ms,
            mem_mib,
            elapsed_ms,
        })
        .into_response(),
        Err(_) => failure(StatusCode::INTERNAL_SERVER_ERROR, "burn failed"),
    }
}

fn auth_failure(error: AuthError) -> Response {
    match error {
        AuthError::Invalid => failure(StatusCode::UNAUTHORIZED, "invalid token"),
        AuthError::Unavailable => failure(StatusCode::SERVICE_UNAVAILABLE, "auth unavailable"),
    }
}

fn failure(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

async fn log_request(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let level = response
        .extensions()
        .get::<LevelName>()
        .map(|LevelName(name)| name.as_str());
    info!(
        %method,
        %path,
        status = response.status().as_u16(),
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        level,
        "request"
    );
    response
}

fn cors(origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.iter().cloned()))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
        .max_age(Duration::from_secs(600))
}
