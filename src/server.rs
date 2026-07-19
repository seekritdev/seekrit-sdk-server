//! The HTTP API the External Secrets Operator's webhook provider calls.
//!
//! Three routes:
//! - `GET /v1/secret/{name}` → `{"value": "<plaintext>"}` (200) or `404` if the
//!   name isn't in the resolved environment. ESO reads the value with
//!   `result.jsonPath: "$.value"`; a 404 lets it honor its `deletionPolicy`.
//! - `GET /v1/secrets` → `{"data": {NAME: value, …}}` — the whole environment,
//!   for `target.template` pulls and the future native provider.
//! - `GET /healthz` → `200`, unauthenticated, for liveness/readiness probes.
//!
//! `/v1/secret*` are gated by a static bearer key (constant-time compared). The
//! sidecar decrypts *everything* the service token can reach, so this endpoint
//! must not be open to arbitrary in-cluster callers — the key is the second
//! line of defense behind the Service's network reachability.

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

use crate::secrets::SecretStore;

/// Shared, cheaply-cloned handler state.
#[derive(Clone)]
pub struct AppState {
    /// The current decrypted snapshot; swapped wholesale by the refresh task.
    pub store: Arc<ArcSwap<SecretStore>>,
    /// The bearer key callers must present on `/v1/secret*`.
    pub api_key: Arc<String>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/secret/:name", get(get_secret))
        .route("/v1/secrets", get(get_all))
        .with_state(state)
}

async fn healthz() -> StatusCode {
    // The server only binds after the first successful load (fail-closed), so if
    // we're serving at all, we're ready.
    StatusCode::OK
}

async fn get_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if let Some(rej) = unauthorized(&state, &headers) {
        return rej;
    }
    match state.store.load().get(&name) {
        Some(value) => Json(json!({ "value": value })).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "name": name })),
        )
            .into_response(),
    }
}

async fn get_all(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(rej) = unauthorized(&state, &headers) {
        return rej;
    }
    Json(json!({ "data": state.store.load().to_map() })).into_response()
}

/// Guard for `/v1/secret*`: checks the `Authorization: Bearer <key>` header
/// against the configured key. Returns `Some(401)` (the response to send) when
/// it's missing or wrong, or `None` when the caller is authorized. (A guard
/// returning the early response beats `Result<(), Response>` — axum's `Response`
/// is large, and a big `Err` variant would bloat every `Ok` too.)
fn unauthorized(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let presented = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match presented {
        Some(key) if constant_time_eq(key.as_bytes(), state.api_key.as_bytes()) => None,
        _ => Some(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "unauthorized" })),
            )
                .into_response(),
        ),
    }
}

/// Length-then-content comparison that doesn't short-circuit on the first
/// differing byte. Length is allowed to leak (keys are fixed-length anyway).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}
