//! `seekrit-sdk-server`: a small, long-lived resolver that runs inside a
//! customer's cluster, holds a seekrit service token, and turns the
//! zero-knowledge `GET /v1/resolve` response into plaintext **locally** (via the
//! shared `seekrit-core` crypto). It caches the decrypted environment in memory,
//! refreshes it on a timer, and serves it over a tiny bearer-authed HTTP API.
//!
//! It exists so the External Secrets Operator's built-in *webhook* provider can
//! sync seekrit secrets into Kubernetes on stock, unmodified ESO — the API never
//! sees plaintext, and decryption stays on the client exactly as for `apps/run`
//! and `apps/proxy`.
//!
//! The library half is split from `main.rs` so the API handlers and the decrypt
//! loader can be exercised in tests (see `tests/server.rs`).

pub mod refresh;
pub mod resolve;
pub mod secrets;
pub mod server;
