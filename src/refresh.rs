//! The background refresh loop.
//!
//! Long-lived, unlike `apps/run` (one-shot) and `apps/proxy` (resolve once at
//! startup, never again). Secrets rotate, so the sidecar re-resolves on a timer
//! and swaps the whole snapshot atomically. A failed refresh is **non-fatal**:
//! we keep serving the last-good snapshot and log a warning, so a transient API
//! blip never takes the endpoint down (ESO would otherwise fail its syncs). The
//! *first* load is still fail-closed — see [`crate::secrets::load`].

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tracing::{info, warn};

use crate::secrets::{self, SecretStore};

/// Re-resolve every `interval`, replacing the shared snapshot on success.
pub async fn run(
    store: Arc<ArcSwap<SecretStore>>,
    client: reqwest::Client,
    api_url: String,
    token: String,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    // If a refresh runs long, don't fire a burst of catch-up ticks afterward.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick is immediate; we already loaded at startup, so skip it.
    ticker.tick().await;

    loop {
        ticker.tick().await;
        match secrets::load(&client, &api_url, &token).await {
            Ok(next) => {
                info!(secrets = next.len(), "refreshed secrets");
                store.store(Arc::new(next));
            }
            Err(e) => warn!("refresh failed, serving last-good snapshot: {e}"),
        }
    }
}
