//! The in-memory secret store and the resolve → decrypt loader.
//!
//! Plaintext lives only here, in this process — the same trust boundary
//! `apps/run` and `apps/proxy` occupy. Values are wrapped in `Zeroizing<String>`
//! so they are scrubbed from memory on drop. The **first** load (at startup) is
//! fail-closed: if the token is bad, the API is unreachable, or any layer fails
//! to decrypt, the process refuses to start rather than serve an empty/partial
//! store. Subsequent refreshes are best-effort — see [`crate::refresh`].

use std::collections::{BTreeMap, HashMap};

use seekrit_core::crypto::{secret_aad, unwrap_dek, TokenKey};
use seekrit_core::interpolate::interpolate_secrets;
use zeroize::Zeroizing;

/// A fatal startup failure. Printed to stderr; the process then exits non-zero.
#[derive(Debug)]
pub enum StartupError {
    Token(String),
    Resolve(String),
    Decrypt(String),
    /// A `${OTHER_SECRET}` reference could not be expanded (a cycle, or an
    /// unbounded expansion) — fail closed rather than serve a wrong value.
    Reference(String),
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartupError::Token(m) => write!(f, "invalid service token: {m}"),
            StartupError::Resolve(m) => write!(f, "could not resolve secrets: {m}"),
            StartupError::Decrypt(m) => write!(f, "could not decrypt secrets: {m}"),
            StartupError::Reference(m) => write!(f, "could not expand secret references: {m}"),
        }
    }
}

impl std::error::Error for StartupError {}

/// Resolved secret name → decrypted value (zeroized on drop).
pub struct SecretStore {
    map: HashMap<String, Zeroizing<String>>,
}

impl SecretStore {
    /// Build a store from already-resolved name/value pairs, verbatim — no
    /// reference expansion (that happens in [`load`], over the merged set). This
    /// is also the seam integration tests construct through.
    pub fn from_values<I: IntoIterator<Item = (String, String)>>(pairs: I) -> Self {
        SecretStore {
            map: pairs
                .into_iter()
                .map(|(k, v)| (k, Zeroizing::new(v)))
                .collect(),
        }
    }

    /// The decrypted value for `name`, if present.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(|z| z.as_str())
    }

    /// Every name → value pair, for the whole-environment endpoint. Clones the
    /// plaintext into the returned map — unavoidable when serializing a response
    /// body, which is itself plaintext. Sorted for a stable response.
    pub fn to_map(&self) -> BTreeMap<String, String> {
        self.map
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().to_string()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The resolved names (for a startup/refresh summary log — never the values).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

/// Fetch `/v1/resolve` and decrypt every layer into a [`SecretStore`], applying
/// the same precedence as `seekrit-run`: groups first, then the app env on top,
/// then `${OTHER_SECRET}` reference expansion over the merged set — so ESO syncs
/// the same value a `seekrit run` process would see.
pub async fn load(
    client: &reqwest::Client,
    api_url: &str,
    token: &str,
) -> Result<SecretStore, StartupError> {
    let body = crate::resolve::fetch_body(client, api_url, token).await?;
    decode(&body, token)
}

/// Decrypt a resolve response body into a [`SecretStore`]. Split out from
/// [`load`] so the same code path serves a live response and a cached one —
/// there is no second decryption path to keep in step.
pub fn decode(body: &str, token: &str) -> Result<SecretStore, StartupError> {
    // Recover the token's private key first (cheap, fails fast before network).
    let key = TokenKey::parse(token).map_err(|e| StartupError::Token(e.to_string()))?;

    let resolved = crate::resolve::parse(body)?;

    // Merged plaintext, before reference expansion. Held in plain `String`s only
    // for the length of this function (`decrypt_secret` returns one anyway);
    // everything that outlives it goes into the `Zeroizing` store below.
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    // Layers arrive lowest precedence first (groups → app); later writes win.
    for layer in &resolved.layers {
        let dek = unwrap_dek(&layer.wrapped_dek, &key)
            .map_err(|e| StartupError::Decrypt(e.to_string()))?;
        for secret in &layer.secrets {
            let aad = secret_aad(&layer.environment_id, &secret.name);
            let value = dek
                .decrypt_secret(&secret.ciphertext, &aad)
                .map_err(|e| StartupError::Decrypt(e.to_string()))?;
            merged.insert(secret.name.clone(), value);
        }
        // `dek` zeroizes itself as the loop iteration ends.
    }

    let expanded =
        interpolate_secrets(&merged).map_err(|e| StartupError::Reference(e.to_string()))?;
    let map: HashMap<String, Zeroizing<String>> = expanded
        .values
        .into_iter()
        .map(|(name, value)| (name, Zeroizing::new(value)))
        .collect();

    Ok(SecretStore { map })
}
