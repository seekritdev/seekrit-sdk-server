//! `seekrit-core`: the transport-free heart of every seekrit machine client.
//!
//! A service token carries its own P-256 private key. Given the token and a
//! `/v1/resolve` response, this crate:
//!   1. recovers the token's key ([`crypto::TokenKey`]),
//!   2. unwraps each environment DEK ([`crypto::unwrap_dek`]), and
//!   3. decrypts each secret ([`crypto::Dek::decrypt_secret`]).
//!
//! It performs **no I/O**: fetching the resolve response is the consuming app's
//! job (`apps/run` uses blocking `ureq`; `apps/proxy` uses async `reqwest`), so
//! this crate stays tiny and reusable. The crypto is bit-compatible with
//! `packages/crypto` (WebCrypto) and proven so by the vector test in `apps/run`.

pub mod b64;
pub mod crypto;
pub mod error;
pub mod resolve;

pub use error::{CoreError, CoreResult};
