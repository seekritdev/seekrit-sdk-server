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
//!
//! [`kms`] adds the same-family client-side KMS envelope operations (`ce1`/`dk1`
//! encrypt, decrypt, and data-key generation) that back the AWS-KMS-compatible
//! gateway in `apps/kms`; its cross-impl vectors live there.

pub mod b64;
pub mod crypto;
pub mod error;
pub mod kms;
pub mod resolve;

pub use error::{CoreError, CoreResult};
