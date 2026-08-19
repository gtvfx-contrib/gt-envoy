//! Opt-in usage telemetry for `envoy`.
//!
//! This module provides a small, pluggable telemetry facade: callers emit
//! named [`TelemetryEvent`]s with arbitrary attributes via [`track`] (or
//! [`track_timed`] for a real, non-zero-duration span), and a swappable
//! [`TelemetrySink`] decides what happens to them. The default sink
//! ([`NullSink`]) silently discards every event, so telemetry is disabled
//! unless a caller explicitly installs a real sink via [`set_sink`] --
//! matching envoy's closed-by-default philosophy elsewhere (e.g. the closed
//! subprocess environment).
//!
//! Two ways to opt in coexist:
//! - **Explicit** (unchanged): a Rust or Python caller installs a sink
//!   directly, e.g. via the `envoy.enable_telemetry(endpoint)` Python
//!   binding.
//! - **Automatic** (new): [`command_run::record_command_run`] resolves
//!   configuration from the environment (see [`config`]) and picks the
//!   right sink itself for each envoy-cli invocation -- this is what powers
//!   the `envoy.command.run` contract without every call site needing to
//!   know about endpoints, transports, redaction, or the spool.
//!
//! # Design
//!
//! Rather than have envoy speak directly to any particular backend (Kibana,
//! Grafana, a data warehouse, ...), the built-in [`OtlpSink`] exports events
//! as [OpenTelemetry](https://opentelemetry.io/) spans over OTLP/HTTP, and
//! the built-in [`file_drop::FileDropSink`] writes the same information as
//! OTLP-JSON files for later ingestion. Actual fan-out to a specific
//! backend happens entirely outside of envoy, in an OpenTelemetry Collector
//! (or any OTLP-compatible receiver) that the operator configures
//! independently -- envoy never needs backend-specific code.
//!
//! ```no_run
//! use envoy_core::telemetry::{self, OtlpSink, TelemetryValue};
//! use std::collections::HashMap;
//!
//! // Opt in to exporting telemetry to a collector.
//! let sink = OtlpSink::new("http://localhost:4318/v1/traces").expect("sink should build");
//! telemetry::set_sink(Box::new(sink));
//!
//! // Record a custom event; a no-op until a sink is installed.
//! let mut attrs = HashMap::new();
//! attrs.insert("command".to_string(), TelemetryValue::Str("unreal".to_string()));
//! attrs.insert("success".to_string(), TelemetryValue::Bool(true));
//! telemetry::track("command_run", attrs);
//! ```

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use opentelemetry::trace::{Span, Tracer, TracerProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod command_run;
pub mod config;
pub mod file_drop;
pub mod install_id;
pub mod redact;
pub mod schema;
pub mod spool;

pub use command_run::record_command_run;
pub use config::{
    resolve_telemetry_config, TelemetryConfig, TelemetryConfigSource, TelemetryTransport,
};
pub use file_drop::FileDropSink;
pub use install_id::installation_id;
pub use schema::{CommandKind, CommandRunContext, ErrorCategory};

/// Error type for telemetry sink construction and export.
#[derive(Debug, Error)]
pub enum TelemetryError {
    /// Failed to build the OTLP exporter (e.g. an invalid endpoint URL).
    #[error("failed to build OTLP exporter for endpoint {endpoint}: {message}")]
    ExporterBuild { endpoint: String, message: String },
}

/// A single attribute value attached to a [`TelemetryEvent`].
///
/// Kept intentionally small (string/bool/int/float) since this is the
/// common subset every telemetry backend and every Python value we expect
/// to receive (`str`, `bool`, `int`, `float`) can represent without loss.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TelemetryValue {
    Str(String),
    Bool(bool),
    Int(i64),
    Float(f64),
}

impl From<&TelemetryValue> for opentelemetry::Value {
    fn from(value: &TelemetryValue) -> Self {
        match value {
            TelemetryValue::Str(text) => opentelemetry::Value::String(text.clone().into()),
            TelemetryValue::Bool(flag) => opentelemetry::Value::Bool(*flag),
            TelemetryValue::Int(number) => opentelemetry::Value::I64(*number),
            TelemetryValue::Float(number) => opentelemetry::Value::F64(*number),
        }
    }
}

/// A named usage event with arbitrary attributes, ready to hand to a
/// [`TelemetrySink`].
#[derive(Clone, Debug)]
pub struct TelemetryEvent {
    /// Event name (e.g. `"command_run"`).
    pub name: String,
    /// Arbitrary attributes describing the event (e.g. `command`,
    /// `duration_ms`, `success`).
    pub attributes: HashMap<String, TelemetryValue>,
    /// When the underlying operation started. Equal to `timestamp` for a
    /// simple point event recorded via [`track`], giving it an honest
    /// zero duration; distinct for a timed span recorded via
    /// [`track_timed`], giving the exported span real, non-zero duration
    /// instead of approximating it via a manual `duration_ms` attribute.
    pub start_time: SystemTime,
    /// When the event was recorded / the operation ended.
    pub timestamp: SystemTime,
}

/// Receives [`TelemetryEvent`]s recorded via [`track`]/[`track_timed`].
///
/// Implementations must be `Send + Sync` since the active sink is stored in
/// global state shared across threads.
pub trait TelemetrySink: Send + Sync {
    /// Record a single event, returning whether it was accepted for
    /// delivery.
    ///
    /// Fire-and-forget for transports that batch/export asynchronously
    /// (e.g. [`OtlpSink`]) -- for those, `true` means "queued for export",
    /// not "confirmed delivered over the network"; call [`Self::flush`]
    /// afterward to get a real confirmation before the process exits. For
    /// transports whose writes are synchronous (e.g.
    /// [`file_drop::FileDropSink`]), `true`/`false` is already the real,
    /// final answer.
    ///
    /// Implementations must not panic; a misbehaving telemetry backend
    /// must never be allowed to crash the host process.
    fn record(&self, event: &TelemetryEvent) -> bool;

    /// Flush any buffered work within a bounded time budget and report
    /// whether it was confirmed delivered.
    ///
    /// Called once per process, after all `record()` calls for this
    /// invocation, to get a real network-level confirmation for batched
    /// transports and decide whether anything needs to be spooled (see
    /// [`command_run`]). Default: always confirmed, which is correct for
    /// [`NullSink`] (nothing to deliver) and for sinks whose `record()`
    /// result is already the real, final answer.
    fn flush(&self, budget: Duration) -> bool {
        let _ = budget;
        true
    }
}

/// The default sink: silently discards every event.
///
/// This is what's active until a caller opts in via [`set_sink`], keeping
/// telemetry disabled by default.
#[derive(Debug, Default)]
pub struct NullSink;

impl TelemetrySink for NullSink {
    fn record(&self, _event: &TelemetryEvent) -> bool {
        true
    }
}

/// Exports events as OpenTelemetry spans over OTLP/HTTP.
///
/// Each event becomes a span named after the event, spanning
/// `event.start_time` to `event.timestamp` (real duration, not a manual
/// attribute), with the event's attributes attached as span attributes.
/// Uses a batched exporter internally so individual [`track`] calls do not
/// block on network I/O.
pub struct OtlpSink {
    tracer: opentelemetry_sdk::trace::Tracer,
    provider: SdkTracerProvider,
}

impl OtlpSink {
    /// Build a new sink exporting to `endpoint` (e.g.
    /// `"http://localhost:4318/v1/traces"`, the default OTLP/HTTP path for
    /// an OpenTelemetry Collector).
    ///
    /// Uses the HTTP transport specifically (not gRPC) so this does not
    /// require the caller to already be running inside a `tokio` runtime,
    /// which matters since envoy is primarily a short-lived, synchronous
    /// CLI tool.
    pub fn new(endpoint: &str) -> Result<Self, TelemetryError> {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .map_err(|error| TelemetryError::ExporterBuild {
                endpoint: endpoint.to_string(),
                message: error.to_string(),
            })?;

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer("envoy");

        Ok(Self { tracer, provider })
    }

    /// Force any buffered spans to be exported immediately, and shut down
    /// the underlying provider.
    ///
    /// envoy is typically a short-lived process, so callers that enable
    /// telemetry should call this (or drop the sink via [`disable`], which
    /// does not flush -- prefer an explicit `shutdown()` call before
    /// process exit) to avoid losing events that were still batched when
    /// the process exited. Prefer [`TelemetrySink::flush`] (via the trait)
    /// for the bounded, repeatable variant used by the automatic
    /// [`command_run`] dispatch path.
    pub fn shutdown(&self) {
        let _ = self.provider.shutdown();
    }
}

impl TelemetrySink for OtlpSink {
    fn record(&self, event: &TelemetryEvent) -> bool {
        let attributes: Vec<KeyValue> = event
            .attributes
            .iter()
            .map(|(key, value)| KeyValue::new(key.clone(), value))
            .collect();

        let mut span = self
            .tracer
            .span_builder(event.name.clone())
            .with_start_time(event.start_time)
            .with_attributes(attributes)
            .start(&self.tracer);
        span.end_with_timestamp(event.timestamp);

        // Queuing into the batch processor essentially always succeeds
        // synchronously; the real network-level outcome is only knowable
        // via `flush`, which does not block this call.
        true
    }

    fn flush(&self, _budget: Duration) -> bool {
        // `force_flush` is safe to call repeatedly (unlike `shutdown`,
        // which permanently marks the provider shut down), so this can run
        // once per delivered record (current event and any spooled
        // retries) without needing to reserve a final call for process
        // exit.
        self.provider.force_flush().is_ok()
    }
}

/// Global sink storage, defaulting to [`NullSink`] on first access.
static SINK: OnceLock<RwLock<Box<dyn TelemetrySink>>> = OnceLock::new();

fn sink_lock() -> &'static RwLock<Box<dyn TelemetrySink>> {
    SINK.get_or_init(|| RwLock::new(Box::new(NullSink)))
}

/// Install `sink` as the active telemetry sink, replacing whatever was
/// active before (including the default [`NullSink`]).
pub fn set_sink(sink: Box<dyn TelemetrySink>) {
    let lock = sink_lock();
    let mut guard = lock
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = sink;
}

/// Revert to the default no-op [`NullSink`], disabling telemetry export.
pub fn disable() {
    set_sink(Box::new(NullSink));
}

/// Return whether a sink other than the default [`NullSink`] is active.
///
/// This is a best-effort, type-based check (it does not distinguish a
/// custom no-op sink from `NullSink`), intended for simple "is telemetry
/// currently enabled" queries such as the Python `isTelemetryEnabled()`
/// binding.
pub fn is_enabled() -> bool {
    // There is no portable way to downcast `Box<dyn TelemetrySink>` back to
    // `NullSink` without adding an `Any` bound to the trait, so track this
    // alongside the sink itself instead of trying to inspect it.
    ENABLED.load(std::sync::atomic::Ordering::SeqCst)
}

static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Record `name` with `attributes` via the currently active sink, as a
/// simple point event (zero duration -- `start_time` equals `timestamp`).
///
/// No-ops (aside from the trivial cost of constructing the event) when
/// telemetry is disabled, since the default [`NullSink`] discards it.
pub fn track(name: &str, attributes: HashMap<String, TelemetryValue>) {
    let now = SystemTime::now();
    let event = TelemetryEvent {
        name: name.to_string(),
        attributes,
        start_time: now,
        timestamp: now,
    };

    let lock = sink_lock();
    let guard = lock.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.record(&event);
}

/// Record `name` with `attributes` via the currently active sink as a
/// **timed span**: `start_time` to `end_time` becomes the span's real
/// duration, rather than a manual `duration_ms` attribute.
pub fn track_timed(
    name: &str,
    attributes: HashMap<String, TelemetryValue>,
    start_time: SystemTime,
    end_time: SystemTime,
) {
    let event = TelemetryEvent {
        name: name.to_string(),
        attributes,
        start_time,
        timestamp: end_time,
    };

    let lock = sink_lock();
    let guard = lock.read().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.record(&event);
}

/// Install `sink` and mark telemetry as enabled for [`is_enabled`].
///
/// This is the function Python's `enable_telemetry()` binding calls; kept
/// separate from [`set_sink`] so Rust callers that want a custom sink
/// without flipping the "is telemetry enabled" flag can use [`set_sink`]
/// directly (e.g. tests).
pub fn enable(sink: Box<dyn TelemetrySink>) {
    set_sink(sink);
    ENABLED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Disable telemetry and revert to [`NullSink`], clearing the
/// [`is_enabled`] flag.
pub fn disable_and_clear_flag() {
    disable();
    ENABLED.store(false, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Default)]
    struct RecordingSink {
        events: Arc<Mutex<Vec<TelemetryEvent>>>,
    }

    impl TelemetrySink for RecordingSink {
        fn record(&self, event: &TelemetryEvent) -> bool {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(event.clone());
            true
        }
    }

    // Telemetry sink state is process-global, so tests that install a sink
    // must not run concurrently with each other or with tests asserting the
    // default no-op behavior. Use a simple lock to serialize them, mirroring
    // the `with_env_lock` pattern used for environment-variable tests
    // elsewhere in this crate.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn null_sink_is_the_default_and_discards_events() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        disable_and_clear_flag();

        assert!(!is_enabled());
        // Should not panic even though nothing is listening.
        track("noop_event", HashMap::new());
    }

    #[test]
    fn set_sink_and_track_delivers_events_with_attributes() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let events = Arc::new(Mutex::new(Vec::new()));
        enable(Box::new(RecordingSink {
            events: events.clone(),
        }));

        assert!(is_enabled());

        let mut attrs = HashMap::new();
        attrs.insert(
            "command".to_string(),
            TelemetryValue::Str("unreal".to_string()),
        );
        attrs.insert("duration_ms".to_string(), TelemetryValue::Int(1500));
        attrs.insert("success".to_string(), TelemetryValue::Bool(true));
        track("command_run", attrs.clone());

        let recorded = events.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].name, "command_run");
        assert_eq!(recorded[0].attributes, attrs);
        assert_eq!(recorded[0].start_time, recorded[0].timestamp);

        drop(recorded);
        disable_and_clear_flag();
    }

    #[test]
    fn track_timed_preserves_distinct_start_and_end_times() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let events = Arc::new(Mutex::new(Vec::new()));
        enable(Box::new(RecordingSink {
            events: events.clone(),
        }));

        let start = SystemTime::now();
        let end = start + Duration::from_millis(1500);
        track_timed("command_run", HashMap::new(), start, end);

        let recorded = events.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(recorded[0].start_time, start);
        assert_eq!(recorded[0].timestamp, end);
        assert!(recorded[0].timestamp > recorded[0].start_time);

        drop(recorded);
        disable_and_clear_flag();
    }

    #[test]
    fn disable_reverts_to_discarding_events() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let events = Arc::new(Mutex::new(Vec::new()));
        enable(Box::new(RecordingSink {
            events: events.clone(),
        }));
        disable_and_clear_flag();

        assert!(!is_enabled());
        track("should_be_dropped", HashMap::new());

        assert!(events.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    }

    #[test]
    fn telemetry_value_converts_to_opentelemetry_value() {
        let str_value: opentelemetry::Value = (&TelemetryValue::Str("x".to_string())).into();
        assert_eq!(str_value, opentelemetry::Value::String("x".into()));

        let bool_value: opentelemetry::Value = (&TelemetryValue::Bool(true)).into();
        assert_eq!(bool_value, opentelemetry::Value::Bool(true));

        let int_value: opentelemetry::Value = (&TelemetryValue::Int(42)).into();
        assert_eq!(int_value, opentelemetry::Value::I64(42));

        let float_value: opentelemetry::Value = (&TelemetryValue::Float(1.5)).into();
        assert_eq!(float_value, opentelemetry::Value::F64(1.5));
    }

    #[test]
    fn telemetry_value_round_trips_through_json() {
        let values = vec![
            TelemetryValue::Str("hello".to_string()),
            TelemetryValue::Bool(true),
            TelemetryValue::Int(42),
            TelemetryValue::Float(1.5),
        ];
        for value in values {
            let json = serde_json::to_string(&value).expect("should serialize");
            let round_tripped: TelemetryValue =
                serde_json::from_str(&json).expect("should deserialize");
            assert_eq!(value, round_tripped);
        }
    }

    #[test]
    fn null_sink_flush_defaults_to_true() {
        assert!(NullSink.flush(Duration::from_millis(10)));
    }
}
