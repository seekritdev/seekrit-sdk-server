//! Instruments for the sidecar.
//!
//! Deliberately few, and deliberately all *counts and outcomes*: the two
//! questions an operator actually has about this process are "is ESO reading
//! what it expects?" and "is the snapshot still fresh?". Neither needs a secret
//! value, and none is recorded — see the zero-knowledge note in
//! `seekrit-telemetry`, and `tests/telemetry.rs` which enforces it.

// Via the re-export, so this binary and the telemetry crate can never end up on
// two incompatible copies of the OpenTelemetry API.
use seekrit_telemetry::opentelemetry::metrics::{Counter, Histogram};
use seekrit_telemetry::opentelemetry::KeyValue;

pub struct Metrics {
    /// Reads served from `/v1/secret*`, by outcome (`hit`/`miss`/`unauthorized`).
    secret_reads: Counter<u64>,
    /// Refresh attempts, by outcome (`ok`/`error`).
    refreshes: Counter<u64>,
    /// Secrets in the snapshot after a successful refresh.
    snapshot_size: Histogram<u64>,
}

impl Metrics {
    pub fn new() -> Self {
        let meter = seekrit_telemetry::meter("seekrit-sdk-server");
        Metrics {
            secret_reads: meter
                .u64_counter("seekrit.sdk_server.secret_reads")
                .with_description("Secret reads served, by outcome.")
                .build(),
            refreshes: meter
                .u64_counter("seekrit.sdk_server.refreshes")
                .with_description("Re-resolve attempts against the seekrit API, by outcome.")
                .build(),
            snapshot_size: meter
                .u64_histogram("seekrit.sdk_server.snapshot_size")
                .with_description("Number of secrets in the snapshot after a refresh.")
                .build(),
        }
    }

    pub fn record_secret_read(&self, outcome: &'static str) {
        self.secret_reads
            .add(1, &[KeyValue::new("outcome", outcome)]);
    }

    /// A refresh finished. `size` is `None` when it failed and the previous
    /// snapshot is still being served.
    pub fn record_refresh(&self, outcome: &'static str, size: Option<usize>) {
        self.refreshes.add(1, &[KeyValue::new("outcome", outcome)]);
        if let Some(n) = size {
            self.snapshot_size.record(n as u64, &[]);
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
