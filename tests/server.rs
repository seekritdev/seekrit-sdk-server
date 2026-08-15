//! Integration tests for the HTTP API, exercised over real HTTP (like
//! `apps/proxy`'s tests). The store is built from the `from_values` seam, so
//! these cover routing + auth + response shapes without needing the API or any
//! crypto — decrypt correctness is covered by `crates/seekrit-core` and the
//! `apps/run` vectors.

use std::sync::Arc;

use arc_swap::ArcSwap;
use seekrit_sdk_server::secrets::SecretStore;
use seekrit_sdk_server::server::{router, AppState};

const API_KEY: &str = "test-key";

/// Bind an ephemeral port, serve the router, and return the base URL.
async fn spawn() -> String {
    let store = SecretStore::from_values([
        (
            "DATABASE_URL".to_string(),
            "postgres://u:p@db/app".to_string(),
        ),
        ("API_TOKEN".to_string(), "sk-live-123".to_string()),
    ]);
    let state = AppState {
        store: Arc::new(ArcSwap::from_pointee(store)),
        api_key: Arc::new(API_KEY.to_string()),
        // No exporter is configured in tests, so these instruments are no-ops.
        metrics: Arc::new(seekrit_sdk_server::telemetry::Metrics::new()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });
    format!("http://{addr}")
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

#[tokio::test]
async fn get_secret_returns_value() {
    let base = spawn().await;
    let resp = client()
        .get(format!("{base}/v1/secret/DATABASE_URL"))
        .bearer_auth(API_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["value"], "postgres://u:p@db/app");
}

#[tokio::test]
async fn unknown_secret_is_404() {
    let base = spawn().await;
    let resp = client()
        .get(format!("{base}/v1/secret/NOPE"))
        .bearer_auth(API_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn missing_auth_is_401() {
    let base = spawn().await;
    let resp = client()
        .get(format!("{base}/v1/secret/DATABASE_URL"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn wrong_auth_is_401() {
    let base = spawn().await;
    let resp = client()
        .get(format!("{base}/v1/secret/DATABASE_URL"))
        .bearer_auth("wrong-key")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn get_all_returns_map() {
    let base = spawn().await;
    let resp = client()
        .get(format!("{base}/v1/secrets"))
        .bearer_auth(API_KEY)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["DATABASE_URL"], "postgres://u:p@db/app");
    assert_eq!(body["data"]["API_TOKEN"], "sk-live-123");
}

#[tokio::test]
async fn healthz_needs_no_auth() {
    let base = spawn().await;
    let resp = client()
        .get(format!("{base}/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}
