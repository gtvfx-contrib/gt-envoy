//! Telemetry configuration resolution.
//!
//! Resolves the small set of environment variables that control envoy's
//! opt-in usage telemetry, and detects which transport (direct OTLP/HTTP or
//! atomic file-drop) a resolved endpoint implies.
//!
//! Precedence, for every recognized key: envoy-cli's own process environment
//! (however the user's shell/session happens to be configured when they
//! invoke `envoy`) is consulted first, then the selected stack's merged
//! `global_env.json` (passed in as `bundle_env`). This lets a studio set a
//! fleet-wide default centrally (e.g. in the `gt/envoy` bundle's
//! `global_env.json`) while still letting an individual user's own session
//! environment override it.
//!
//! `bundle_env` here is deliberately just the bundle-resolved environment
//! used to decide whether and where to export telemetry -- never the
//! constructed, closed subprocess environment built for the wrapped command.
//! The two must not be conflated given how central closed-mode environment
//! isolation is elsewhere in envoy.

use std::collections::HashMap;
use std::env;

/// Primary destination setting for envoy's own telemetry exporter.
pub const TELEMETRY_ENDPOINT_VAR: &str = "ENVOY_TELEMETRY_ENDPOINT";

/// Opt-out switch. A value of `false` (case-insensitive) always disables
/// telemetry regardless of any other setting, no matter which recognized
/// source it was read from.
pub const TELEMETRY_ENABLED_VAR: &str = "ENVOY_TELEMETRY_ENABLED";

/// Comma-separated list of additional sensitive flag names to redact, on top
/// of the built-in list (see [`crate::telemetry::redact`]).
pub const TELEMETRY_REDACT_ARGS_VAR: &str = "ENVOY_TELEMETRY_REDACT_ARGS";

/// Standard OpenTelemetry traces-specific endpoint, consulted only when
/// [`TELEMETRY_ENDPOINT_VAR`] is unset. Takes priority over
/// [`OTEL_ENDPOINT_VAR`], matching upstream OTel SDK precedence.
pub const OTEL_TRACES_ENDPOINT_VAR: &str = "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT";

/// Standard OpenTelemetry generic endpoint fallback.
pub const OTEL_ENDPOINT_VAR: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// Standard OpenTelemetry headers variable. Also used to carry an optional
/// shared bearer token for a studio Collector's OTLP receiver.
pub const OTEL_HEADERS_VAR: &str = "OTEL_EXPORTER_OTLP_HEADERS";

/// Standard OpenTelemetry export timeout variable.
pub const OTEL_TIMEOUT_VAR: &str = "OTEL_EXPORTER_OTLP_TIMEOUT";

/// Standard OpenTelemetry service-name variable.
pub const OTEL_SERVICE_NAME_VAR: &str = "OTEL_SERVICE_NAME";

/// Standard OpenTelemetry resource-attributes variable.
pub const OTEL_RESOURCE_ATTRIBUTES_VAR: &str = "OTEL_RESOURCE_ATTRIBUTES";

/// Which transport a resolved telemetry endpoint implies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelemetryTransport {
    /// Direct OTLP/HTTP export to an always-on Collector.
    Http,
    /// Atomic file-drop export to a filesystem/network path; no listening
    /// service required at that path.
    FileDrop,
}

impl TelemetryTransport {
    /// Short machine-readable name, used as a span attribute value and in
    /// `--diagnose` output.
    pub fn as_str(&self) -> &'static str {
        match self {
            TelemetryTransport::Http => "http",
            TelemetryTransport::FileDrop => "file-drop",
        }
    }
}

/// Where a resolved [`TelemetryConfig`] came from, surfaced via
/// `--diagnose` so operators can tell a fleet-wide default from a personal
/// override at a glance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelemetryConfigSource {
    /// Resolved from envoy-cli's own process environment.
    ProcessEnv,
    /// Resolved from the selected stack's merged bundle environment.
    BundleEnv,
    /// Resolved from a standard `OTEL_EXPORTER_OTLP_*` fallback variable,
    /// rather than `ENVOY_TELEMETRY_ENDPOINT`.
    OtelFallback,
}

impl TelemetryConfigSource {
    /// Short human-readable label for `--diagnose` output.
    pub fn as_str(&self) -> &'static str {
        match self {
            TelemetryConfigSource::ProcessEnv => "process environment",
            TelemetryConfigSource::BundleEnv => "bundle global_env.json",
            TelemetryConfigSource::OtelFallback => "OTEL_* fallback",
        }
    }
}

/// Fully-resolved telemetry configuration for the current invocation.
///
/// Returned by [`resolve_telemetry_config`]; `None` from that function means
/// telemetry is disabled and no `TelemetryConfig` exists at all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelemetryConfig {
    /// The resolved destination value, exactly as read from the
    /// environment (not yet sanitized).
    pub endpoint: String,
    /// Transport implied by `endpoint`'s shape.
    pub transport: TelemetryTransport,
    /// Which recognized source `endpoint` was read from.
    pub source: TelemetryConfigSource,
    /// Raw `OTEL_EXPORTER_OTLP_HEADERS` value, if any. May carry a shared
    /// bearer token for a studio Collector; never logged or displayed by
    /// `--diagnose`.
    pub headers: Option<String>,
    /// Raw `OTEL_EXPORTER_OTLP_TIMEOUT` value, if any.
    pub timeout: Option<String>,
    /// Raw `OTEL_SERVICE_NAME` value, if any.
    pub service_name: Option<String>,
    /// Raw `OTEL_RESOURCE_ATTRIBUTES` value, if any.
    pub resource_attributes: Option<String>,
    /// Additional sensitive flag names from [`TELEMETRY_REDACT_ARGS_VAR`],
    /// on top of the built-in redaction list.
    pub extra_redact_args: Vec<String>,
}

impl TelemetryConfig {
    /// Return `endpoint` with any embedded credentials stripped, safe to
    /// display in `--diagnose` output or logs.
    ///
    /// File-drop paths never carry embedded credentials, so this only does
    /// work for the `http`/`https` transport: it strips a `user:pass@`
    /// (or bare `user@`) prefix immediately after the scheme, if present.
    /// Headers are never part of `endpoint` in the first place, so this
    /// never needs to (and cannot) strip them.
    pub fn sanitized_endpoint(&self) -> String {
        if self.transport != TelemetryTransport::Http {
            return self.endpoint.clone();
        }

        if let Some(scheme_end) = self.endpoint.find("://") {
            let (scheme, rest) = self.endpoint.split_at(scheme_end + 3);
            // Only look for userinfo (`user:pass@`/`user@`) within the
            // authority component -- i.e. before the first `/`, `?`, or
            // `#` -- so an `@` occurring later, in the path, query, or
            // fragment, isn't mistaken for embedded credentials and
            // incorrectly stripped.
            let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
            if let Some(at_index) = rest[..authority_end].find('@') {
                return format!("{scheme}{}", &rest[at_index + 1..]);
            }
        }

        self.endpoint.clone()
    }
}

/// Detect which transport `endpoint`'s shape implies.
///
/// A value starting with `http://` or `https://` (case-insensitive) selects
/// direct OTLP/HTTP export. Any other value -- a UNC path, a mapped drive,
/// or a mount point -- is treated as a filesystem/network path and selects
/// file-drop export.
pub fn detect_transport(endpoint: &str) -> TelemetryTransport {
    let lower = endpoint.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        TelemetryTransport::Http
    } else {
        TelemetryTransport::FileDrop
    }
}

/// Resolve `name` from envoy-cli's own process environment first, then from
/// `bundle_env`. Empty-string values are treated the same as unset, matching
/// the rest of envoy-core's environment-resolution conventions.
fn resolve_key(
    name: &str,
    bundle_env: Option<&HashMap<String, String>>,
) -> Option<(String, TelemetryConfigSource)> {
    if let Ok(value) = env::var(name) {
        if !value.is_empty() {
            return Some((value, TelemetryConfigSource::ProcessEnv));
        }
    }

    if let Some(env_map) = bundle_env {
        if let Some(value) = env_map.get(name) {
            if !value.is_empty() {
                return Some((value.clone(), TelemetryConfigSource::BundleEnv));
            }
        }
    }

    None
}

/// Return `true` if the resolved `ENVOY_TELEMETRY_ENABLED` value is
/// `false` (case-insensitive), from either recognized source. This check is
/// intentionally independent of whether an endpoint is configured at all --
/// it always wins, preserving an individual opt-out.
fn is_explicitly_disabled(bundle_env: Option<&HashMap<String, String>>) -> bool {
    resolve_key(TELEMETRY_ENABLED_VAR, bundle_env)
        .map(|(value, _source)| value.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

fn resolve_redact_args(bundle_env: Option<&HashMap<String, String>>) -> Vec<String> {
    resolve_key(TELEMETRY_REDACT_ARGS_VAR, bundle_env)
        .map(|(value, _source)| {
            value
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve the effective telemetry configuration for the current
/// invocation, or `None` if telemetry is disabled or unconfigured.
///
/// `bundle_env` is the selected stack's merged bundle environment (e.g. from
/// `global_env.json`), used only as a fallback source for the recognized
/// telemetry keys below -- never mutated, and no other keys are read from
/// it.
pub fn resolve_telemetry_config(
    bundle_env: Option<&HashMap<String, String>>,
) -> Option<TelemetryConfig> {
    if is_explicitly_disabled(bundle_env) {
        return None;
    }

    let headers = resolve_key(OTEL_HEADERS_VAR, bundle_env).map(|(value, _)| value);
    let timeout = resolve_key(OTEL_TIMEOUT_VAR, bundle_env).map(|(value, _)| value);
    let service_name = resolve_key(OTEL_SERVICE_NAME_VAR, bundle_env).map(|(value, _)| value);
    let resource_attributes =
        resolve_key(OTEL_RESOURCE_ATTRIBUTES_VAR, bundle_env).map(|(value, _)| value);
    let extra_redact_args = resolve_redact_args(bundle_env);

    if let Some((endpoint, source)) = resolve_key(TELEMETRY_ENDPOINT_VAR, bundle_env) {
        let transport = detect_transport(&endpoint);
        return Some(TelemetryConfig {
            endpoint,
            transport,
            source,
            headers,
            timeout,
            service_name,
            resource_attributes,
            extra_redact_args,
        });
    }

    // Fall back to standard OTel env vars, for teams already standardized on
    // them. These are always URLs (never file-drop paths), matching how
    // `OTEL_EXPORTER_OTLP_*` is defined upstream.
    let otel_endpoint = resolve_key(OTEL_TRACES_ENDPOINT_VAR, bundle_env)
        .or_else(|| resolve_key(OTEL_ENDPOINT_VAR, bundle_env));

    otel_endpoint.map(|(endpoint, _source)| TelemetryConfig {
        endpoint,
        transport: TelemetryTransport::Http,
        source: TelemetryConfigSource::OtelFallback,
        headers,
        timeout,
        service_name,
        resource_attributes,
        extra_redact_args,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    struct EnvVarGuard {
        previous: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvVarGuard {
        fn set_many(updates: &[(&'static str, Option<&str>)]) -> Self {
            let mut previous = Vec::new();
            for (key, value) in updates {
                previous.push((*key, env::var_os(key)));
                match value {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
            Self { previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (key, previous) in &self.previous {
                match previous {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }

    fn all_telemetry_vars_cleared() -> Vec<(&'static str, Option<&'static str>)> {
        vec![
            (TELEMETRY_ENABLED_VAR, None),
            (TELEMETRY_ENDPOINT_VAR, None),
            (TELEMETRY_REDACT_ARGS_VAR, None),
            (OTEL_TRACES_ENDPOINT_VAR, None),
            (OTEL_ENDPOINT_VAR, None),
            (OTEL_HEADERS_VAR, None),
            (OTEL_TIMEOUT_VAR, None),
            (OTEL_SERVICE_NAME_VAR, None),
            (OTEL_RESOURCE_ATTRIBUTES_VAR, None),
        ]
    }

    #[test]
    fn returns_none_when_nothing_is_configured() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _guard = EnvVarGuard::set_many(&all_telemetry_vars_cleared());

        assert!(resolve_telemetry_config(None).is_none());
    }

    #[test]
    fn enabled_false_always_disables_even_with_endpoint_set() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((TELEMETRY_ENABLED_VAR, Some("false")));
        updates.push((
            TELEMETRY_ENDPOINT_VAR,
            Some("http://localhost:4318/v1/traces"),
        ));
        let _guard = EnvVarGuard::set_many(&updates);

        assert!(resolve_telemetry_config(None).is_none());
    }

    #[test]
    fn enabled_false_is_case_insensitive() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((TELEMETRY_ENABLED_VAR, Some("FALSE")));
        updates.push((
            TELEMETRY_ENDPOINT_VAR,
            Some("http://localhost:4318/v1/traces"),
        ));
        let _guard = EnvVarGuard::set_many(&updates);

        assert!(resolve_telemetry_config(None).is_none());
    }

    #[test]
    fn process_env_takes_precedence_over_bundle_env() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((
            TELEMETRY_ENDPOINT_VAR,
            Some("http://from-process-env/v1/traces"),
        ));
        let _guard = EnvVarGuard::set_many(&updates);

        let mut bundle_env = HashMap::new();
        bundle_env.insert(
            TELEMETRY_ENDPOINT_VAR.to_string(),
            "http://from-bundle-env/v1/traces".to_string(),
        );

        let config = resolve_telemetry_config(Some(&bundle_env)).expect("should resolve");
        assert_eq!(config.endpoint, "http://from-process-env/v1/traces");
        assert_eq!(config.source, TelemetryConfigSource::ProcessEnv);
    }

    #[test]
    fn falls_back_to_bundle_env_when_process_env_unset() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let _guard = EnvVarGuard::set_many(&all_telemetry_vars_cleared());

        let mut bundle_env = HashMap::new();
        bundle_env.insert(
            TELEMETRY_ENDPOINT_VAR.to_string(),
            "\\\\studio-share\\telemetry".to_string(),
        );

        let config = resolve_telemetry_config(Some(&bundle_env)).expect("should resolve");
        assert_eq!(config.endpoint, "\\\\studio-share\\telemetry");
        assert_eq!(config.source, TelemetryConfigSource::BundleEnv);
        assert_eq!(config.transport, TelemetryTransport::FileDrop);
    }

    #[test]
    fn http_endpoint_selects_http_transport() {
        assert_eq!(
            detect_transport("http://localhost:4318/v1/traces"),
            TelemetryTransport::Http
        );
        assert_eq!(
            detect_transport("HTTPS://collector.studio.example/v1/traces"),
            TelemetryTransport::Http
        );
    }

    #[test]
    fn non_http_endpoint_selects_file_drop_transport() {
        assert_eq!(
            detect_transport("\\\\studio-share\\telemetry"),
            TelemetryTransport::FileDrop
        );
        assert_eq!(
            detect_transport("Z:\\telemetry"),
            TelemetryTransport::FileDrop
        );
        assert_eq!(
            detect_transport("/mnt/telemetry"),
            TelemetryTransport::FileDrop
        );
    }

    #[test]
    fn falls_back_to_otel_traces_endpoint_when_envoy_endpoint_unset() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((
            OTEL_TRACES_ENDPOINT_VAR,
            Some("http://otel-traces.example/v1/traces"),
        ));
        updates.push((OTEL_ENDPOINT_VAR, Some("http://otel-generic.example")));
        let _guard = EnvVarGuard::set_many(&updates);

        let config = resolve_telemetry_config(None).expect("should resolve");
        assert_eq!(config.endpoint, "http://otel-traces.example/v1/traces");
        assert_eq!(config.source, TelemetryConfigSource::OtelFallback);
        assert_eq!(config.transport, TelemetryTransport::Http);
    }

    #[test]
    fn falls_back_to_otel_generic_endpoint_when_traces_specific_unset() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((OTEL_ENDPOINT_VAR, Some("http://otel-generic.example")));
        let _guard = EnvVarGuard::set_many(&updates);

        let config = resolve_telemetry_config(None).expect("should resolve");
        assert_eq!(config.endpoint, "http://otel-generic.example");
        assert_eq!(config.source, TelemetryConfigSource::OtelFallback);
    }

    #[test]
    fn envoy_endpoint_takes_precedence_over_otel_fallback() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((
            TELEMETRY_ENDPOINT_VAR,
            Some("http://envoy-endpoint.example"),
        ));
        updates.push((
            OTEL_TRACES_ENDPOINT_VAR,
            Some("http://otel-endpoint.example"),
        ));
        let _guard = EnvVarGuard::set_many(&updates);

        let config = resolve_telemetry_config(None).expect("should resolve");
        assert_eq!(config.endpoint, "http://envoy-endpoint.example");
        assert_eq!(config.source, TelemetryConfigSource::ProcessEnv);
    }

    #[test]
    fn sanitized_endpoint_strips_embedded_credentials() {
        let config = TelemetryConfig {
            endpoint: "http://user:secret-pass@collector.example/v1/traces".to_string(),
            transport: TelemetryTransport::Http,
            source: TelemetryConfigSource::ProcessEnv,
            headers: Some("Authorization=Bearer supersecrettoken".to_string()),
            timeout: None,
            service_name: None,
            resource_attributes: None,
            extra_redact_args: Vec::new(),
        };

        let sanitized = config.sanitized_endpoint();
        assert_eq!(sanitized, "http://collector.example/v1/traces");
        assert!(!sanitized.contains("secret-pass"));
        assert!(!sanitized.contains("supersecrettoken"));
    }

    #[test]
    fn sanitized_endpoint_ignores_an_at_sign_outside_the_authority() {
        // An `@` in the query string (or path/fragment) is not userinfo
        // and must survive untouched -- only an `@` within the authority
        // component (before the first `/`, `?`, or `#`) is credentials.
        let config = TelemetryConfig {
            endpoint: "http://collector.example/v1/traces?owner=team@example.com".to_string(),
            transport: TelemetryTransport::Http,
            source: TelemetryConfigSource::ProcessEnv,
            headers: None,
            timeout: None,
            service_name: None,
            resource_attributes: None,
            extra_redact_args: Vec::new(),
        };

        assert_eq!(
            config.sanitized_endpoint(),
            "http://collector.example/v1/traces?owner=team@example.com"
        );
    }

    #[test]
    fn sanitized_endpoint_is_unchanged_for_file_drop() {
        let config = TelemetryConfig {
            endpoint: "\\\\studio-share\\telemetry".to_string(),
            transport: TelemetryTransport::FileDrop,
            source: TelemetryConfigSource::BundleEnv,
            headers: None,
            timeout: None,
            service_name: None,
            resource_attributes: None,
            extra_redact_args: Vec::new(),
        };

        assert_eq!(config.sanitized_endpoint(), "\\\\studio-share\\telemetry");
    }

    #[test]
    fn extra_redact_args_are_parsed_from_comma_separated_list() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut updates = all_telemetry_vars_cleared();
        updates.push((
            TELEMETRY_ENDPOINT_VAR,
            Some("http://localhost:4318/v1/traces"),
        ));
        updates.push((
            TELEMETRY_REDACT_ARGS_VAR,
            Some(" custom-flag , other_flag ,,"),
        ));
        let _guard = EnvVarGuard::set_many(&updates);

        let config = resolve_telemetry_config(None).expect("should resolve");
        assert_eq!(
            config.extra_redact_args,
            vec!["custom-flag".to_string(), "other_flag".to_string()]
        );
    }
}
