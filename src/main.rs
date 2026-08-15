//! Entry point: parse args → resolve + decrypt (fail-closed) → serve the API and
//! refresh on a timer until interrupted.
//!
//! Like `apps/proxy`, this holds a token and produces plaintext, so it fails
//! **closed**: a missing/bad token, an unreachable API, or a layer that won't
//! decrypt refuses to start rather than serve an empty or partial store.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use seekrit_sdk_server::resolve::DEFAULT_API_URL;
use seekrit_sdk_server::server::{router, AppState};
use seekrit_sdk_server::{refresh, secrets};
use tracing::{error, info};

const DEFAULT_LISTEN: &str = "0.0.0.0:8080";
const DEFAULT_REFRESH: Duration = Duration::from_secs(60);

const HELP: &str = "\
seekrit-sdk-server — resolve and decrypt seekrit secrets in-cluster, then serve
them over a tiny authed HTTP API for the External Secrets Operator's webhook
provider. Decryption happens locally; the seekrit API never sees plaintext.

USAGE:
    seekrit-sdk-server [OPTIONS]

OPTIONS:
        --listen <addr>          bind address (default: 0.0.0.0:8080)
    -t, --token <skt_...>        service token (default: SEEKRIT_TOKEN)
        --api-key <key>          bearer key required on /v1/secret* (default:
                                 SEEKRIT_SDK_API_KEY)
        --api-url <url>          API base URL (default: SEEKRIT_API_URL or
                                 https://api.seekrit.dev)
        --refresh-interval <d>   re-resolve cadence, e.g. 60s, 5m, 1h (default: 60s)
    -h, --help                   show this help
    -V, --version                show the version

API:
    GET /v1/secret/<name>   -> 200 {\"value\":\"…\"} | 404       (Bearer <api-key>)
    GET /v1/secrets         -> 200 {\"data\":{…}}               (Bearer <api-key>)
    GET /healthz            -> 200                              (no auth)
";

struct Args {
    listen: Option<String>,
    token: Option<String>,
    api_key: Option<String>,
    api_url: Option<String>,
    refresh_interval: Option<String>,
}

enum Parsed {
    Help,
    Version,
    Run(Args),
}

#[tokio::main]
async fn main() {
    std::process::exit(run().await);
}

/// Set telemetry up, run the server, then flush.
///
/// `serve` has many early-return paths; wrapping it keeps the flush in exactly
/// one place, so a new `return` can't silently drop buffered spans. (The
/// `std::process::exit` in `main` runs no destructors, so an explicit shutdown
/// is the only thing that guarantees delivery.)
async fn run() -> i32 {
    let telemetry = seekrit_telemetry::init("seekrit-sdk-server", env!("CARGO_PKG_VERSION"));
    seekrit_telemetry::install_subscriber(&telemetry, "info");

    let code = serve().await;

    telemetry.shutdown();
    code
}

async fn serve() -> i32 {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(argv) {
        Ok(Parsed::Help) => {
            print!("{HELP}");
            return 0;
        }
        Ok(Parsed::Version) => {
            println!("seekrit-sdk-server {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Ok(Parsed::Run(a)) => a,
        Err(msg) => {
            eprintln!("seekrit-sdk-server: {msg}\n\n{HELP}");
            return 2;
        }
    };

    let listen: SocketAddr = match args
        .listen
        .or_else(|| env_nonempty("SEEKRIT_SDK_LISTEN"))
        .unwrap_or_else(|| DEFAULT_LISTEN.to_string())
        .parse()
    {
        Ok(addr) => addr,
        Err(e) => {
            error!("invalid --listen address: {e}");
            return 2;
        }
    };

    // Credentials come from flags/env only.
    let token = match args.token.or_else(|| env_nonempty("SEEKRIT_TOKEN")) {
        Some(t) => t,
        None => {
            error!("no service token — pass --token skt_… or set SEEKRIT_TOKEN");
            return 2;
        }
    };
    // The API key gates the decrypt endpoints; refuse to start without one so the
    // sidecar is never an open, unauthenticated secret oracle.
    let api_key = match args.api_key.or_else(|| env_nonempty("SEEKRIT_SDK_API_KEY")) {
        Some(k) => k,
        None => {
            error!("no API key — pass --api-key or set SEEKRIT_SDK_API_KEY (gates /v1/secret*)");
            return 2;
        }
    };
    let api_url = args
        .api_url
        .or_else(|| env_nonempty("SEEKRIT_API_URL"))
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());
    let refresh_interval = match args
        .refresh_interval
        .or_else(|| env_nonempty("SEEKRIT_SDK_REFRESH_INTERVAL"))
    {
        Some(s) => match parse_duration(&s) {
            Ok(d) => d,
            Err(e) => {
                error!("invalid --refresh-interval: {e}");
                return 2;
            }
        },
        None => DEFAULT_REFRESH,
    };

    let client = match reqwest::Client::builder()
        .user_agent(concat!("seekrit-sdk-server/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            error!("could not build HTTP client: {e}");
            return 1;
        }
    };

    // Fail-closed: resolve + decrypt up front. If this fails, don't bind.
    let store = match secrets::load(&client, &api_url, &token).await {
        Ok(s) => s,
        Err(e) => {
            error!("{e}");
            return 1;
        }
    };
    if store.is_empty() {
        info!("no secrets resolved for this token — /v1/secret/* will return 404");
    } else {
        info!(secrets = store.len(), "resolved secrets");
    }

    let store = Arc::new(ArcSwap::from_pointee(store));
    let metrics = Arc::new(seekrit_sdk_server::telemetry::Metrics::new());

    // Refresh on a timer; failures keep serving the last-good snapshot.
    tokio::spawn(refresh::run(
        store.clone(),
        client.clone(),
        api_url.clone(),
        token.clone(),
        refresh_interval,
        metrics.clone(),
    ));

    let listener = match tokio::net::TcpListener::bind(listen).await {
        Ok(l) => l,
        Err(e) => {
            error!("could not bind {listen}: {e}");
            return 1;
        }
    };
    info!(%listen, refresh_interval = ?refresh_interval, "seekrit-sdk-server listening");

    let state = AppState {
        store,
        api_key: Arc::new(api_key),
        metrics,
    };
    if let Err(e) = axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown())
        .await
    {
        error!("server error: {e}");
        return 1;
    }
    0
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    info!("shutting down");
}

fn parse_args(argv: Vec<String>) -> Result<Parsed, String> {
    let mut args = Args {
        listen: None,
        token: None,
        api_key: None,
        api_url: None,
        refresh_interval: None,
    };
    let mut it = argv.into_iter();
    while let Some(arg) = it.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg, None),
        };
        match name.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            "--listen" => args.listen = Some(take(&name, inline, &mut it)?),
            "-t" | "--token" => args.token = Some(take(&name, inline, &mut it)?),
            "--api-key" => args.api_key = Some(take(&name, inline, &mut it)?),
            "--api-url" => args.api_url = Some(take(&name, inline, &mut it)?),
            "--refresh-interval" => args.refresh_interval = Some(take(&name, inline, &mut it)?),
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(Parsed::Run(args))
}

fn take<I: Iterator<Item = String>>(
    flag: &str,
    inline: Option<String>,
    it: &mut I,
) -> Result<String, String> {
    if let Some(v) = inline {
        return Ok(v);
    }
    it.next().ok_or_else(|| format!("{flag} requires a value"))
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Parse a small duration like `30s`, `5m`, `1h`, or a bare seconds count.
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (value, unit_secs) = match s.strip_suffix(['s', 'm', 'h']) {
        Some(rest) => {
            let mult = match s.as_bytes()[s.len() - 1] {
                b's' => 1,
                b'm' => 60,
                b'h' => 3600,
                _ => unreachable!(),
            };
            (rest, mult)
        }
        None => (s, 1),
    };
    let n: u64 = value
        .trim()
        .parse()
        .map_err(|_| format!("not a duration: {s:?} (try 60s, 5m, 1h)"))?;
    if n == 0 {
        return Err("must be greater than zero".to_string());
    }
    Ok(Duration::from_secs(n * unit_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("90").unwrap(), Duration::from_secs(90));
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("nope").is_err());
    }
}
