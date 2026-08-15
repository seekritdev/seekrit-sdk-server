//! The background refresh loop.
//!
//! Long-lived, unlike `apps/run` (one-shot) and `apps/proxy` (resolve once at
//! startup, then only if it started degraded). Secrets rotate, so the sidecar
//! re-resolves on a timer and swaps the whole snapshot atomically. A failed
//! refresh is **non-fatal**: we keep serving the last-good snapshot and log a
//! warning, so a transient API blip never takes the endpoint down (ESO would
//! otherwise fail its syncs).
//!
//! While degraded the cadence tightens: instead of waiting out the full
//! interval, the loop retries on a short doubling backoff capped at that
//! interval, so a recovered network is picked up in seconds rather than
//! minutes. On success it returns to the normal interval.
//!
//! The *first* load is still fail-closed unless the last-known-good cache is
//! enabled — see [`crate::secrets::load`] and [`crate::lkg`].

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tracing::{error, info, warn};

use crate::lkg::{resolve_live, Lkg};
use crate::secrets::SecretStore;
use crate::telemetry::Metrics;

/// First retry after a failed refresh. Deliberately much shorter than any
/// sensible refresh interval: the sooner we notice the API is back, the shorter
/// the window in which we serve secrets that may have rotated.
const RETRY_BASE: Duration = Duration::from_secs(5);

/// Re-resolve every `interval`, replacing the shared snapshot on success.
pub async fn run(
    store: Arc<ArcSwap<SecretStore>>,
    client: reqwest::Client,
    api_url: String,
    token: String,
    interval: Duration,
    metrics: Arc<Metrics>,
    lkg: Option<Lkg>,
) {
    // `Some(d)` while degraded: the shortened delay before the next attempt.
    let mut backoff: Option<Duration> = None;

    loop {
        tokio::time::sleep(backoff.unwrap_or(interval)).await;
        // A refresh is the sidecar's liveness signal for rotations: alert on
        // `outcome="error"` here and you learn the snapshot is going stale long
        // before ESO starts syncing something wrong.
        let span = tracing::info_span!("refresh_secrets");
        let _enter = span.enter();

        match resolve_live(&client, &api_url, &token, lkg.as_ref()).await {
            Ok(next) => {
                if backoff.is_some() {
                    info!("reconnected to the seekrit API");
                }
                info!(secrets = next.len(), "refreshed secrets");
                metrics.record_refresh("ok", Some(next.len()));
                store.store(Arc::new(next));
                backoff = None;
            }
            Err(e) if !e.may_fall_back() => {
                // The API is reachable and refusing us. Drop the cached entry so
                // a restart fails closed rather than reviving withdrawn secrets;
                // the in-memory snapshot is left alone deliberately, since a
                // misapplied policy should not take a whole cluster's syncs down
                // without an operator in the loop.
                if let Some(lkg) = lkg.as_ref() {
                    lkg.invalidate();
                }
                error!("the seekrit API refused this token ({e}); still serving the last-good snapshot in memory — rotate or re-grant the token, then restart");
                metrics.record_refresh("error", None);
                backoff = Some(next_backoff(backoff, interval));
            }
            Err(e) => {
                let delay = next_backoff(backoff, interval);
                warn!(
                    "refresh failed, serving last-good snapshot: {e}; retrying in {}",
                    seekrit_cache::humanize(delay)
                );
                metrics.record_refresh("error", None);
                backoff = Some(delay);
            }
        }
    }
}

/// Double the current backoff, starting at [`RETRY_BASE`], never exceeding the
/// configured refresh interval — past that we are just doing the normal poll.
fn next_backoff(current: Option<Duration>, interval: Duration) -> Duration {
    match current {
        Some(d) => (d * 2).min(interval),
        None => RETRY_BASE.min(interval),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_starts_short_and_doubles_up_to_the_interval() {
        let interval = Duration::from_secs(60);
        // First failure retries far sooner than the normal cadence.
        let first = next_backoff(None, interval);
        assert_eq!(first, RETRY_BASE);
        assert_eq!(next_backoff(Some(first), interval), Duration::from_secs(10));
        assert_eq!(
            next_backoff(Some(Duration::from_secs(40)), interval),
            interval,
            "backoff should cap at the refresh interval"
        );
        assert_eq!(next_backoff(Some(interval), interval), interval);
    }

    #[test]
    fn a_short_interval_is_never_lengthened_by_the_backoff() {
        // With a 1s refresh interval, retrying every 5s would be slower than
        // just refreshing — the cap keeps the tighter cadence.
        let interval = Duration::from_secs(1);
        assert_eq!(next_backoff(None, interval), interval);
        assert_eq!(next_backoff(Some(interval), interval), interval);
    }
}
