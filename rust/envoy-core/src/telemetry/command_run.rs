//! Top-level orchestration for the `envoy.command.run` record.
//!
//! This is the single integration point envoy-cli calls from each of its
//! early-return branches (see the Command-Run Contract plan): it resolves
//! telemetry configuration, builds the appropriate sink, applies the shared
//! spool/retry layer, and records the event -- all without ever raising or
//! changing the caller's exit code. A misconfigured or unreachable
//! destination is best-effort by design.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::config::{resolve_telemetry_config, TelemetryConfig, TelemetryTransport};
use super::file_drop::FileDropSink;
use super::install_id::installation_id;
use super::schema::{self, CommandRunContext};
use super::spool::{self, SpooledRecord, TelemetrySpool};
use super::{NullSink, OtlpSink, TelemetryEvent, TelemetrySink, TelemetryValue};

/// Record one `envoy.command.run` event for the current invocation.
///
/// `bundle_env` is the selected stack's merged bundle environment, if any
/// was resolved yet at this point in `run_cli` (several early-return
/// branches run before stack/bundle resolution and pass `None`, which is
/// fine -- process-environment-only resolution still applies).
///
/// No-ops entirely (aside from resolving configuration) when telemetry is
/// disabled or unconfigured.
pub fn record_command_run(
    mut context: CommandRunContext,
    start_time: SystemTime,
    end_time: SystemTime,
    bundle_env: Option<&HashMap<String, String>>,
) {
    let Some(config) = resolve_telemetry_config(bundle_env) else {
        return;
    };

    if context.installation_id.is_none() {
        context.installation_id = Some(installation_id());
    }
    if context.extra_redact_args.is_empty() {
        context.extra_redact_args = config.extra_redact_args.clone();
    }

    let attributes = context.build_attributes();
    deliver_with_spool(
        &config,
        schema::COMMAND_RUN_EVENT_NAME,
        attributes,
        start_time,
        end_time,
    );
}

fn build_sink(config: &TelemetryConfig) -> Box<dyn TelemetrySink> {
    match config.transport {
        TelemetryTransport::Http => {
            match OtlpSink::with_headers(&config.endpoint, config.headers.as_deref()) {
                Ok(sink) => Box::new(sink),
                // An endpoint that fails to even build (e.g. an invalid URL) is
                // a configuration error, not a transient outage -- retrying it
                // via the spool would never succeed, so this discards rather
                // than spooling indefinitely.
                Err(_) => Box::new(NullSink),
            }
        }
        TelemetryTransport::FileDrop => Box::new(FileDropSink::new(config.endpoint.clone())),
    }
}

fn deliver_with_spool(
    config: &TelemetryConfig,
    event_name: &str,
    mut attributes: HashMap<String, TelemetryValue>,
    start_time: SystemTime,
    end_time: SystemTime,
) {
    let sink = build_sink(config);
    let spool = TelemetrySpool::new();
    let transport_label = config.transport.as_str();

    // First, make one bounded attempt to flush anything already spooled
    // from a previous invocation, oldest first, through this invocation's
    // sink -- so a workstation that has reconnected gradually catches up.
    // This never blocks longer than the budget, regardless of backlog size.
    spool.flush_with_budget(spool::DEFAULT_FLUSH_BUDGET, |spooled| {
        let mut retry_attributes = spooled.attributes.clone();
        schema::stamp_delivery_metadata(&mut retry_attributes, transport_label, true);
        let retry_time = unix_millis_to_system_time(spooled.timestamp_unix_millis);
        let event = TelemetryEvent {
            name: spooled.name.clone(),
            attributes: retry_attributes,
            start_time: retry_time,
            timestamp: retry_time,
        };
        deliver_one(sink.as_ref(), &event, spool::DEFAULT_FLUSH_BUDGET)
    });

    // Then record and deliver this invocation's own event.
    schema::stamp_delivery_metadata(&mut attributes, transport_label, false);
    let event = TelemetryEvent {
        name: event_name.to_string(),
        attributes,
        start_time,
        timestamp: end_time,
    };

    if !deliver_one(sink.as_ref(), &event, spool::DEFAULT_FLUSH_BUDGET) {
        let spooled = SpooledRecord {
            name: event.name.clone(),
            attributes: event.attributes.clone(),
            timestamp_unix_millis: end_time
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        };
        let _ = spool.enqueue(&spooled);
    }
}

/// Record `event` and, only if accepted, attempt a bounded flush for a real
/// delivery confirmation. Returns `false` (should be spooled) if either
/// step fails.
fn deliver_one(sink: &dyn TelemetrySink, event: &TelemetryEvent, budget: Duration) -> bool {
    if !sink.record(event) {
        return false;
    }
    sink.flush(budget)
}

fn unix_millis_to_system_time(unix_millis: u128) -> SystemTime {
    let clamped = u64::try_from(unix_millis).unwrap_or(u64::MAX);
    UNIX_EPOCH + Duration::from_millis(clamped)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::telemetry::config::{TELEMETRY_ENABLED_VAR, TELEMETRY_ENDPOINT_VAR};
    use crate::telemetry::schema::CommandKind;

    struct EnvVarGuard {
        previous: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvVarGuard {
        fn set_many(updates: &[(&'static str, Option<&str>)]) -> Self {
            let mut previous = Vec::new();
            for (key, value) in updates {
                previous.push((*key, std::env::var_os(key)));
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
            Self { previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (key, previous) in &self.previous {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn sample_context() -> CommandRunContext {
        CommandRunContext {
            kind: CommandKind::ManagedCommand,
            command_name: Some("unreal".to_string()),
            ..Default::default()
        }
    }

    fn json_files_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn does_nothing_when_telemetry_is_disabled() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set_many(&[
            (
                "ENVOY_CONFIG_ROOT",
                Some(config_root.path().to_str().unwrap()),
            ),
            (TELEMETRY_ENABLED_VAR, Some("false")),
            (TELEMETRY_ENDPOINT_VAR, None),
        ]);

        let now = SystemTime::now();
        record_command_run(sample_context(), now, now, None);

        // Nothing should have been created under the config root at all.
        assert!(!config_root.path().join("telemetry").exists());
    }

    #[test]
    fn file_drop_transport_delivers_without_spooling() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let drop_dir = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set_many(&[
            (
                "ENVOY_CONFIG_ROOT",
                Some(config_root.path().to_str().unwrap()),
            ),
            (TELEMETRY_ENABLED_VAR, None),
            (
                TELEMETRY_ENDPOINT_VAR,
                Some(drop_dir.path().to_str().unwrap()),
            ),
        ]);

        let start = SystemTime::now();
        let end = start + Duration::from_millis(250);
        record_command_run(sample_context(), start, end, None);

        assert_eq!(json_files_in(drop_dir.path()).len(), 1);
        let spool = TelemetrySpool::new();
        assert_eq!(spool.depth(), 0);
    }

    #[test]
    fn unreachable_file_drop_destination_spools_the_event() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let blocking_file = config_root.path().join("blocked-by-a-file");
        fs::write(&blocking_file, b"not a directory").expect("should write blocking file");
        let unreachable_drop_dir = blocking_file.join("telemetry");
        let _guard = EnvVarGuard::set_many(&[
            (
                "ENVOY_CONFIG_ROOT",
                Some(config_root.path().to_str().unwrap()),
            ),
            (TELEMETRY_ENABLED_VAR, None),
            (
                TELEMETRY_ENDPOINT_VAR,
                Some(unreachable_drop_dir.to_str().unwrap()),
            ),
        ]);

        let now = SystemTime::now();
        record_command_run(sample_context(), now, now, None);

        let spool = TelemetrySpool::new();
        assert_eq!(spool.depth(), 1);
    }

    #[test]
    fn a_later_successful_invocation_flushes_the_previously_spooled_record() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        // Kept alive for the whole test: `TelemetrySpool::new()` re-resolves
        // `ENVOY_CONFIG_ROOT` every time it is constructed, so this must
        // still be set when `spool.depth()` is checked below, not just
        // while each `record_command_run` call is in flight.
        let _config_root_guard = EnvVarGuard::set_many(&[(
            "ENVOY_CONFIG_ROOT",
            Some(config_root.path().to_str().unwrap()),
        )]);

        // First invocation: destination unreachable, so its event spools.
        {
            let blocking_file = config_root.path().join("blocked-by-a-file");
            fs::write(&blocking_file, b"not a directory").expect("should write blocking file");
            let unreachable_drop_dir = blocking_file.join("telemetry");
            let _guard = EnvVarGuard::set_many(&[
                (TELEMETRY_ENABLED_VAR, None),
                (
                    TELEMETRY_ENDPOINT_VAR,
                    Some(unreachable_drop_dir.to_str().unwrap()),
                ),
            ]);
            let now = SystemTime::now();
            record_command_run(sample_context(), now, now, None);
        }

        let spool = TelemetrySpool::new();
        assert_eq!(spool.depth(), 1);

        // Second invocation: destination now reachable. Both the flushed
        // retry and this invocation's own event should be delivered.
        let drop_dir = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set_many(&[
            (TELEMETRY_ENABLED_VAR, None),
            (
                TELEMETRY_ENDPOINT_VAR,
                Some(drop_dir.path().to_str().unwrap()),
            ),
        ]);
        let now = SystemTime::now();
        record_command_run(sample_context(), now, now, None);

        assert_eq!(json_files_in(drop_dir.path()).len(), 2);
        assert_eq!(spool.depth(), 0);

        // Exactly one of the two delivered files should be flagged as
        // delivered via a spool retry.
        let mut retry_flags = Vec::new();
        for path in json_files_in(drop_dir.path()) {
            let contents = fs::read_to_string(&path).expect("file should be readable");
            let value: serde_json::Value =
                serde_json::from_str(&contents).expect("should be valid JSON");
            retry_flags.push(
                value["attributes"]["envoy.telemetry.delivered_via_retry"]["Bool"]
                    .as_bool()
                    .unwrap_or(false),
            );
        }
        retry_flags.sort();
        assert_eq!(retry_flags, vec![false, true]);
    }
}
