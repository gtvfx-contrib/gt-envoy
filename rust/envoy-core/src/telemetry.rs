//! Opt-in usage telemetry for `envoy`.
//!
//! This module provides a small, pluggable telemetry facade: callers emit
//! named [`TelemetryEvent`]s with arbitrary attributes via [`track`], and a
//! swappable [`TelemetrySink`] decides what happens to them. The default
//! sink ([`NullSink`]) silently discards every event, so telemetry is
//! disabled unless a caller explicitly installs a real sink via
//! [`set_sink`] -- matching envoy's closed-by-default philosophy elsewhere
//! (e.g. the closed subprocess environment).
//!
//! # Design
//!
//! Rather than have envoy speak directly to any particular backend (Kibana,
//! Grafana, a data warehouse, ...), the built-in [`OtlpSink`] exports events
//! as [OpenTelemetry](https://opentelemetry.io/) spans over OTLP/HTTP. Actual
//! fan-out to a specific backend then happens entirely outside of envoy, in
//! an OpenTelemetry Collector (or any OTLP-compatible receiver) that the
//! operator configures independently -- envoy never needs backend-specific
//! code. This mirrors the same architecture used by
//! [Lore](https://github.com/EpicGames/lore)'s own `lore-telemetry` crate,
//! which is built on the identical `opentelemetry` + `opentelemetry_sdk`
//! stack.
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
use std::time::SystemTime;

use opentelemetry::trace::{TraceContextExt, Tracer, TracerProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use thiserror::Error;

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
#[derive(Clone, Debug, PartialEq)]
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
    /// When the event was recorded.
    pub timestamp: SystemTime,
}

/// Receives [`TelemetryEvent`]s recorded via [`track`].
///
/// Implementations must be `Send + Sync` since the active sink is stored in
/// global state shared across threads.
pub trait TelemetrySink: Send + Sync {
    /// Record a single event. Implementations should not panic; a
    /// misbehaving telemetry backend must never be allowed to crash the
    /// host process.
    fn record(&self, event: &TelemetryEvent);
}

/// The default sink: silently discards every event.
///
/// This is what's active until a caller opts in via [`set_sink`], keeping
/// telemetry disabled by default.
#[derive(Debug, Default)]
pub struct NullSink;

impl TelemetrySink for NullSink {
    fn record(&self, _event: &TelemetryEvent) {}
}

/// Exports events as OpenTelemetry spans over OTLP/HTTP.
///
/// Each event becomes a zero-duration span named after the event, with the
/// event's attributes attached as span attributes. Uses a batched exporter
/// internally so individual [`track`] calls do not block on network I/O.
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

    /// Force any buffered spans to be exported immediately.
    ///
    /// envoy is typically a short-lived process, so callers that enable
    /// telemetry should call this (or drop the sink via [`disable`], which
    /// does not flush -- prefer an explicit `shutdown()` call before
    /// process exit) to avoid losing events that were still batched when
    /// the process exited.
    pub fn shutdown(&self) {
        let _ = self.provider.shutdown();
    }
}

impl TelemetrySink for OtlpSink {
    fn record(&self, event: &TelemetryEvent) {
        let attributes: Vec<KeyValue> = event
            .attributes
            .iter()
            .map(|(key, value)| KeyValue::new(key.clone(), value))
            .collect();

        self.tracer.in_span(event.name.clone(), |context| {
            context.span().set_attributes(attributes);
        });
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

/// Record `name` with `attributes` via the currently active sink.
///
/// No-ops (aside from the trivial cost of constructing the event) when
/// telemetry is disabled, since the default [`NullSink`] discards it.
pub fn track(name: &str, attributes: HashMap<String, TelemetryValue>) {
    let event = TelemetryEvent {
        name: name.to_string(),
        attributes,
        timestamp: SystemTime::now(),
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
        fn record(&self, event: &TelemetryEvent) {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(event.clone());
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
}
