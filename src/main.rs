//! Entry point: parse args → resolve + decrypt (fail-closed) → serve the API and
//! refresh on a timer until interrupted.
//!
//! Like `apps/proxy`, this holds a token and produces plaintext, so it fails
//! **closed**: a missing/bad token, an unreachable API, or a layer that won't
//! decrypt refuses to start rather than serve an empty or partial store.
//!
//! `--cache-dir` softens exactly one of those cases — an API that cannot be
//! *reached* — by starting from the last-known-good response on disk. Without
//! it, a pod rescheduled mid-outage cannot complete its first load, never
//! binds, and every ExternalSecret it backs stops syncing.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use seekrit_sdk_server::lkg::{resolve_live, Lkg};
use seekrit_sdk_server::refresh;
use seekrit_sdk_server::resolve::DEFAULT_API_URL;
use seekrit_sdk_server::server::{router, AppState};
use tracing::{error, info, warn};

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
        --cache                  keep a last-known-good copy of the encrypted
                                 response and start from it when the API is
                                 unreachable (off by default)
        --cache-dir <path>       where to keep it; implies --cache (default:
                                 SEEKRIT_SDK_CACHE_DIR, else
                                 $XDG_CACHE_HOME/seekrit). Point this at a
                                 volume — an in-pod path is lost on reschedule.
        --cache-max-age <d>      how stale that copy may be and still be used,
                                 e.g. 15m, 24h, 7d (default: 24h)
    -h, --help                   show this help
    -V, --version                show the version

LAST-KNOWN-GOOD CACHE:
    Only the *encrypted* response is stored (0600, in a 0700 directory) —
    decrypting it still needs this token. Startup resolves live first and only
    falls back when the API cannot be reached; a refused resolve (401/403)
    drops the entry and still fails closed.

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
    cache: bool,
    cache_dir: Option<String>,
    cache_max_age: Option<String>,
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

    // Read before the fields below are consumed. A typo'd duration is a usage
    // error, so it is caught here rather than at the first cache write.
    let cache_config = match cache_settings(&args) {
        Ok(c) => c,
        Err(e) => {
            error!("{e}");
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

    // The last-known-good cache, when asked for. Opening it can only fail on a
    // directory we cannot determine — a warning, since the live resolve below
    // may well succeed and make the cache moot.
    let lkg =
        cache_config.and_then(
            |(dir, max_age)| match Lkg::open(dir, max_age, &api_url, &token) {
                Ok(handle) => Some(handle),
                Err(e) => {
                    warn!("cache disabled: {e}");
                    None
                }
            },
        );

    // Fail-closed: resolve + decrypt up front. If this fails, don't bind —
    // unless the cache is on and the API was merely unreachable.
    let store = match resolve_live(&client, &api_url, &token, lkg.as_ref()).await {
        Ok(s) => s,
        Err(e) => match lkg.as_ref().and_then(|l| l.fallback(&e, &token)) {
            Some(cached) => cached,
            None => {
                error!("{e}");
                return 1;
            }
        },
    };
    if store.is_empty() {
        info!("no secrets resolved for this token — /v1/secret/* will return 404");
    } else {
        info!(secrets = store.len(), "resolved secrets");
    }

    let store = Arc::new(ArcSwap::from_pointee(store));
    let metrics = Arc::new(seekrit_sdk_server::telemetry::Metrics::new());

    // Refresh on a timer; failures keep serving the last-good snapshot and
    // retry on a short backoff until the API answers again.
    tokio::spawn(refresh::run(
        store.clone(),
        client.clone(),
        api_url.clone(),
        token.clone(),
        refresh_interval,
        metrics.clone(),
        lkg,
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

/// Resolve the cache flags: `(directory, max age)`, or `None` when off.
///
/// Any of `--cache`, `--cache-dir`, or their env equivalents opts in — naming a
/// directory is already an explicit request for one, so it need not be paired
/// with the bare flag.
type CacheSettings = Option<(Option<String>, Duration)>;

fn cache_settings(args: &Args) -> Result<CacheSettings, String> {
    let dir = args
        .cache_dir
        .clone()
        .or_else(|| env_nonempty("SEEKRIT_SDK_CACHE_DIR"));
    let max_age_raw = args
        .cache_max_age
        .clone()
        .or_else(|| env_nonempty("SEEKRIT_SDK_CACHE_MAX_AGE"));
    let enabled = args.cache
        || dir.is_some()
        || env_nonempty("SEEKRIT_SDK_CACHE").is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        });
    if !enabled {
        return Ok(None);
    }
    let max_age = match max_age_raw {
        Some(raw) => seekrit_cache::parse_duration(&raw)
            .map_err(|e| format!("invalid --cache-max-age: {e}"))?,
        None => seekrit_cache::DEFAULT_MAX_AGE,
    };
    Ok(Some((dir, max_age)))
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
        cache: false,
        cache_dir: None,
        cache_max_age: None,
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
            "--cache" => args.cache = true,
            "--cache-dir" => args.cache_dir = Some(take(&name, inline, &mut it)?),
            "--cache-max-age" => args.cache_max_age = Some(take(&name, inline, &mut it)?),
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

    fn args() -> Args {
        Args {
            listen: None,
            token: None,
            api_key: None,
            api_url: None,
            refresh_interval: None,
            cache: false,
            cache_dir: None,
            cache_max_age: None,
        }
    }

    #[test]
    fn cache_is_off_unless_asked_for() {
        assert!(cache_settings(&args()).unwrap().is_none());
    }

    #[test]
    fn naming_a_directory_opts_in() {
        let a = Args {
            cache_dir: Some("/var/cache/seekrit".into()),
            ..args()
        };
        let (dir, max_age) = cache_settings(&a).unwrap().expect("enabled");
        assert_eq!(dir.as_deref(), Some("/var/cache/seekrit"));
        assert_eq!(max_age, seekrit_cache::DEFAULT_MAX_AGE);
    }

    #[test]
    fn bare_flag_uses_the_default_directory() {
        let a = Args {
            cache: true,
            cache_max_age: Some("15m".into()),
            ..args()
        };
        let (dir, max_age) = cache_settings(&a).unwrap().expect("enabled");
        assert!(dir.is_none());
        assert_eq!(max_age, Duration::from_secs(900));
    }

    #[test]
    fn a_bad_max_age_is_a_usage_error() {
        let a = Args {
            cache: true,
            cache_max_age: Some("whenever".into()),
            ..args()
        };
        assert!(cache_settings(&a).is_err());
    }

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
