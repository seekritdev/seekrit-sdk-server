//! The opt-in last-known-good cache, shared by startup (`main.rs`) and the
//! refresh loop ([`crate::refresh`]).
//!
//! Without it, the sidecar's in-memory last-good snapshot dies with the pod: a
//! reschedule during a seekrit outage leaves it unable to complete its
//! fail-closed first load, so it never binds and every ExternalSecret sync
//! stops. With `--cache-dir` pointed at a volume, that same restart comes back
//! on the cached response and keeps serving while it retries.
//!
//! Only ciphertext is stored — see the `seekrit-cache` crate docs.

use std::time::Duration;

use seekrit_cache::{Cache, CacheKey, Lookup};
use tracing::warn;

use crate::resolve::{self, ResolveFailure};
use crate::secrets::{self, SecretStore};

/// An opened cache, bound to this sidecar's exact resolve request.
pub struct Lkg {
    cache: Cache,
    key: CacheKey,
}

impl Lkg {
    /// Open the cache at `dir` (or the platform default). The sidecar resolves
    /// the plain bound environment — no branch, no group overrides — so the key
    /// is just the API URL and the token.
    pub fn open(
        dir: Option<String>,
        max_age: Duration,
        api_url: &str,
        token: &str,
    ) -> Result<Lkg, seekrit_cache::CacheError> {
        Ok(Lkg {
            cache: Cache::with_optional_dir(dir, max_age)?,
            key: CacheKey::new(api_url, token, None, &[]),
        })
    }

    /// Record a freshly-fetched response. Best-effort: a cache we could not
    /// write is never a reason to discard a resolve that worked.
    pub fn record(&self, body: &str) {
        if let Err(e) = self.cache.write(&self.key, body) {
            warn!("could not update the cache: {e}");
        }
    }

    /// Drop the entry. Called when the API refuses this token, so a later
    /// restart fails closed instead of quietly reviving withdrawn secrets.
    pub fn invalidate(&self) {
        self.cache.invalidate(&self.key);
    }

    /// The cached store to fall back to, if the cache may stand in for `err`.
    pub fn fallback(&self, err: &ResolveFailure, token: &str) -> Option<SecretStore> {
        if !err.may_fall_back() {
            self.invalidate();
            return None;
        }
        match self.cache.read(&self.key) {
            Lookup::Hit(entry) => match secrets::decode(&entry.body, token) {
                Ok(store) => {
                    warn!(
                        secrets = store.len(),
                        "{err} — starting on cached secrets fetched {} ago; will keep retrying",
                        seekrit_cache::humanize(entry.age)
                    );
                    Some(store)
                }
                Err(e) => {
                    warn!("cached secrets could not be decrypted: {e}");
                    None
                }
            },
            Lookup::Missing => None,
            Lookup::Expired { age } => {
                warn!(
                    "cached secrets are {} old, past --cache-max-age",
                    seekrit_cache::humanize(age)
                );
                None
            }
            Lookup::Unusable(why) => {
                warn!("ignoring the cached secrets: {why}");
                None
            }
        }
    }
}

/// Resolve live and decrypt, refreshing the cached copy on success.
///
/// A response that will not decrypt is deliberately **not** cached: storing a
/// payload we already know is unusable would only guarantee a broken fallback.
pub async fn resolve_live(
    client: &reqwest::Client,
    api_url: &str,
    token: &str,
    lkg: Option<&Lkg>,
) -> Result<SecretStore, ResolveFailure> {
    let body = resolve::fetch_body(client, api_url, token).await?;
    let store = secrets::decode(&body, token)
        // A payload we cannot decrypt is an answer we cannot use; the cache
        // must not paper over it.
        .map_err(|e| ResolveFailure::Refused(e.to_string()))?;
    if let Some(lkg) = lkg {
        lkg.record(&body);
    }
    Ok(store)
}
