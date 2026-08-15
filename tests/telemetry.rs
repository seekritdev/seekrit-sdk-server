//! Zero-knowledge enforcement for telemetry.
//!
//! This sidecar's whole job is handing plaintext to ESO: `GET /v1/secret/{name}`
//! puts the decrypted value straight in the response body. The value is
//! therefore in scope in the same handler that records the span, so this test
//! drives the real route and asserts the exported spans carry the **name** and
//! never the **value**.

use std::sync::Arc;

use arc_swap::ArcSwap;
use seekrit_sdk_server::secrets::SecretStore;
use seekrit_sdk_server::server::{router, AppState};
use seekrit_telemetry::testing::Capture;

const API_KEY: &str = "test-key";
const SECRET_NAME: &str = "DATABASE_URL";
/// Distinctive enough that a substring match cannot be a coincidence.
const SECRET_VALUE: &str = "postgres://u:CANARY-7c1d4e-pw@db/app";

async fn spawn() -> String {
    let store = SecretStore::from_values([(SECRET_NAME.to_string(), SECRET_VALUE.to_string())]);
    let state = AppState {
        store: Arc::new(ArcSwap::from_pointee(store)),
        api_key: Arc::new(API_KEY.to_string()),
        metrics: Arc::new(seekrit_sdk_server::telemetry::Metrics::new()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });
    format!("http://{addr}")
}

/// Serving a secret records its name, never its value.
#[tokio::test]
async fn served_value_never_reaches_telemetry() {
    let capture = Capture::install();
    let base = spawn().await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/v1/secret/{SECRET_NAME}"))
        .header("authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .expect("request should be answered");
    assert!(resp.status().is_success());
    // The value really did travel — so the test is exercising the leak path.
    assert!(resp.text().await.unwrap().contains("CANARY-7c1d4e-pw"));

    capture.assert_absent(&[SECRET_VALUE, "CANARY-7c1d4e-pw", "CANARY"]);

    let emitted = capture.emitted_strings();
    assert!(
        emitted.iter().any(|s| s.contains(SECRET_NAME)),
        "the secret NAME should be recorded: {emitted:?}"
    );
}

/// `/v1/secrets` returns *every* value at once — the highest-volume leak
/// opportunity. It must record a count and nothing else.
#[tokio::test]
async fn bulk_read_records_only_a_count() {
    let capture = Capture::install();
    let base = spawn().await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/v1/secrets"))
        .header("authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .expect("request should be answered");
    assert!(resp.status().is_success());

    capture.assert_absent(&[SECRET_VALUE, "CANARY"]);

    let emitted = capture.emitted_strings();
    assert!(
        emitted
            .iter()
            .any(|s| s.contains(seekrit_telemetry::attr::SECRET_COUNT)),
        "the bulk read should record a count: {emitted:?}"
    );
}

/// A rejected caller must not have their presented bearer key recorded — it is
/// a credential, and 401s are exactly where one is tempting to log.
#[tokio::test]
async fn rejected_api_key_is_never_recorded() {
    let capture = Capture::install();
    let base = spawn().await;

    let wrong_key = "WRONGKEY-3b9f2a-should-not-be-exported";
    let resp = reqwest::Client::new()
        .get(format!("{base}/v1/secret/{SECRET_NAME}"))
        .header("authorization", format!("Bearer {wrong_key}"))
        .send()
        .await
        .expect("request should be answered");
    assert_eq!(resp.status(), 401);

    capture.assert_absent(&[wrong_key, "WRONGKEY", API_KEY, SECRET_VALUE]);

    let emitted = capture.emitted_strings();
    assert!(
        emitted.iter().any(|s| s.contains("401")),
        "the 401 status should be recorded: {emitted:?}"
    );
}
