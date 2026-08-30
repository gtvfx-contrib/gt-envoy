//! File-drop telemetry export.
//!
//! Serializes each recorded event as a small OTLP-JSON-shaped payload and
//! writes it atomically (temp file + rename) under a configured
//! filesystem/network path, for later ingestion by the telemetry bundle's
//! sweep -- no listening service required at that path.
//!
//! Collision-safety is load-bearing here: many workstations can write to
//! the same shared UNC path concurrently, so every write uses a
//! per-attempt, globally-unique filename (a high-resolution timestamp, this
//! workstation's installation UUID, and a random suffix) written first
//! under a temporary name and then atomically `rename()`d to its final
//! name. The sweep only ever observes fully-written files with
//! globally-unique names, so two workstations can never clobber or
//! interleave each other's payloads.
//!
//! Redaction is the caller's responsibility (see
//! [`crate::telemetry::command_run`]) -- this sink writes whatever
//! attributes it is given as-is, so a [`super::TelemetryEvent`] must
//! already be redacted before it reaches [`FileDropSink::record`].

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

use super::install_id::installation_id;
use super::{TelemetryEvent, TelemetrySink, TelemetryValue};

/// OTLP-JSON-shaped serialized form of one recorded event, written to the
/// drop path.
///
/// Deliberately small/flat rather than a full OTLP
/// `ExportTraceServiceRequest` envelope -- the sweep (owned by the
/// telemetry bundle, not envoy itself) is responsible for expanding this
/// into a real OTLP payload when forwarding it to the Collector, matching
/// how backend-specific concerns live in the bundle rather than in envoy.
#[derive(Serialize)]
struct FileDropPayload<'a> {
    name: &'a str,
    attributes: &'a HashMap<String, TelemetryValue>,
    timestamp_unix_millis: u128,
}

/// Writes recorded events as OTLP-JSON files under a filesystem/network
/// drop path.
pub struct FileDropSink {
    drop_dir: PathBuf,
}

impl FileDropSink {
    /// Build a sink that drops files under `drop_dir` (a UNC path, mapped
    /// drive, or mount point). The directory is created on first write if
    /// it does not already exist.
    pub fn new(drop_dir: impl Into<PathBuf>) -> Self {
        Self {
            drop_dir: drop_dir.into(),
        }
    }

    /// A globally-unique file stem: zero-padded nanosecond timestamp (so
    /// lexical sort order matches chronological order for the sweep),
    /// this workstation's installation UUID, and a random suffix.
    fn unique_file_stem() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        format!(
            "{:020}-{}-{}",
            timestamp.as_nanos(),
            installation_id(),
            Uuid::new_v4()
        )
    }

    fn write_atomically(&self, payload: &FileDropPayload<'_>) -> std::io::Result<()> {
        fs::create_dir_all(&self.drop_dir)?;

        let stem = Self::unique_file_stem();
        let final_path = self.drop_dir.join(format!("{stem}.json"));
        let temp_path = self.drop_dir.join(format!("{stem}.json.tmp"));

        let json = serde_json::to_vec(payload)
            .map_err(|source| std::io::Error::new(std::io::ErrorKind::InvalidData, source))?;
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, &final_path)?;
        Ok(())
    }
}

impl TelemetrySink for FileDropSink {
    fn record(&self, event: &TelemetryEvent) -> bool {
        let timestamp_unix_millis = event
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let payload = FileDropPayload {
            name: &event.name,
            attributes: &event.attributes,
            timestamp_unix_millis,
        };

        // Each write is synchronous, so its own result is already the
        // definitive per-record answer -- no separate `flush` override is
        // needed here; the trait's default (`true`) is correct once we get
        // this far.
        self.write_atomically(&payload).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::*;

    // `record()` calls `installation_id()`, which reads the real
    // `ENVOY_CONFIG_ROOT` process environment variable. Every test here
    // must isolate that (and hold the crate-wide env-var test lock) even
    // though these tests don't otherwise care about the config root --
    // without it, a test elsewhere in the crate that legitimately mutates
    // `ENVOY_CONFIG_ROOT` under the same lock could race with an
    // unguarded read here across threads (`cargo test` runs tests in
    // parallel within one process, and environment variables are
    // process-global).
    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn sample_event() -> TelemetryEvent {
        let mut attributes = HashMap::new();
        attributes.insert(
            "command".to_string(),
            TelemetryValue::Str("unreal".to_string()),
        );
        let now = SystemTime::now();
        TelemetryEvent {
            name: "envoy.command.run".to_string(),
            attributes,
            start_time: now,
            timestamp: now,
        }
    }

    #[test]
    fn record_writes_exactly_one_final_file_and_no_temp_file() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", config_root.path());
        let temp_dir = tempdir().expect("tempdir should be created");
        let sink = FileDropSink::new(temp_dir.path());

        assert!(sink.record(&sample_event()));

        let entries: Vec<_> = fs::read_dir(temp_dir.path())
            .expect("dir should be readable")
            .filter_map(|entry| entry.ok())
            .collect();
        assert_eq!(entries.len(), 1, "exactly one file should be visible");
        let name = entries[0].file_name().to_string_lossy().into_owned();
        assert!(name.ends_with(".json"));
        assert!(!name.ends_with(".tmp"));
    }

    #[test]
    fn record_produces_valid_json_with_the_events_attributes() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", config_root.path());
        let temp_dir = tempdir().expect("tempdir should be created");
        let sink = FileDropSink::new(temp_dir.path());
        sink.record(&sample_event());

        let entry = fs::read_dir(temp_dir.path())
            .expect("dir should be readable")
            .next()
            .expect("one file should exist")
            .expect("entry should be readable");
        let contents = fs::read_to_string(entry.path()).expect("file should be readable");
        let value: serde_json::Value =
            serde_json::from_str(&contents).expect("should be valid JSON");
        assert_eq!(value["name"], "envoy.command.run");
    }

    #[test]
    fn concurrent_writes_never_collide_on_filename() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", config_root.path());
        let temp_dir = tempdir().expect("tempdir should be created");
        let sink = FileDropSink::new(temp_dir.path());

        for _ in 0..50 {
            sink.record(&sample_event());
        }

        let names: HashSet<String> = fs::read_dir(temp_dir.path())
            .expect("dir should be readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names.len(),
            50,
            "every write must produce a uniquely named file"
        );
    }

    #[test]
    fn record_returns_true_on_success_and_default_flush_agrees() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", config_root.path());
        let temp_dir = tempdir().expect("tempdir should be created");
        let sink = FileDropSink::new(temp_dir.path());

        assert!(sink.record(&sample_event()));
        assert!(sink.flush(Duration::from_millis(100)));
    }

    #[test]
    fn record_returns_false_on_write_failure() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config_root = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", config_root.path());

        // Create a *file* where the sink expects to create a directory, so
        // `create_dir_all` fails for every write attempt.
        let temp_dir = tempdir().expect("tempdir should be created");
        let blocked_path = temp_dir.path().join("blocked-by-a-file");
        fs::write(&blocked_path, b"not a directory").expect("should write blocking file");

        let sink = FileDropSink::new(&blocked_path);
        assert!(!sink.record(&sample_event()));
    }
}
