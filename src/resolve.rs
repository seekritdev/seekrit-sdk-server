//! The resolve network call: `GET /v1/resolve` with the service token as a
//! bearer credential. It deserializes into the shared [`ResolveResponse`] type;
//! the server returns ciphertext + wrapped DEKs only — never plaintext. Called
//! once at startup (fail-closed) and again on every refresh tick.

use seekrit_core::resolve::ResolveResponse;

use crate::secrets::StartupError;

/// The default public API, overridable via `--api-url` / `SEEKRIT_API_URL`.
pub const DEFAULT_API_URL: &str = "https://api.seekrit.dev";

pub async fn fetch(
    client: &reqwest::Client,
    api_url: &str,
    token: &str,
) -> Result<ResolveResponse, StartupError> {
    let base = api_url.trim_end_matches('/');
    let url = format!("{base}/v1/resolve");

    let resp = client
        .get(&url)
        .bearer_auth(token)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| StartupError::Resolve(e.to_string()))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| StartupError::Resolve(e.to_string()))?;

    if !status.is_success() {
        return Err(StartupError::Resolve(format!(
            "HTTP {} from {url}: {}",
            status.as_u16(),
            snippet(&body)
        )));
    }

    serde_json::from_str(&body)
        .map_err(|e| StartupError::Resolve(format!("could not parse resolve response: {e}")))
}

/// A short, single-line snippet of an error body for logs (never a secret;
/// resolve error bodies are the API's JSON error envelope).
fn snippet(body: &str) -> String {
    let one_line: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.len() > 200 {
        format!("{}…", &one_line[..200])
    } else {
        one_line
    }
}
