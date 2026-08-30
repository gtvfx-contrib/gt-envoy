//! Bounded local spool for telemetry delivery retries.
//!
//! When a delivery attempt fails (share unreachable, collector unreachable),
//! the already-redacted record is written to this on-disk spool instead of
//! being lost. Every subsequent invocation makes one bounded, time-limited
//! attempt to flush previously spooled records (oldest first) so a
//! workstation that reconnects gradually catches up without ever blocking a
//! command on network or file I/O.
//!
//! The spool is capped by both record count and total size, with
//! oldest-first eviction, so an extended offline period cannot grow local
//! disk usage without limit. Spooled files contain only already-redacted
//! data -- redaction happens once, before a record is ever constructed, so
//! nothing sensitive is retried or persisted longer than the original event.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::fs_lock::with_exclusive_lock;
use crate::telemetry::install_id::telemetry_dir;
use crate::telemetry::TelemetryValue;

/// Default maximum number of spooled records retained.
pub const DEFAULT_MAX_SPOOL_COUNT: usize = 1000;

/// Default maximum total size, in bytes, of the spool directory.
pub const DEFAULT_MAX_SPOOL_BYTES: u64 = 10 * 1024 * 1024;

/// Default time budget for one flush attempt, so a large backlog never
/// meaningfully delays a command.
pub const DEFAULT_FLUSH_BUDGET: Duration = Duration::from_millis(500);

const SPOOL_FILE_EXTENSION: &str = "json";
const LOCK_FILE_NAME: &str = ".lock";

/// Failure modes for spool operations. Best-effort by design: callers
/// should generally not let these fail a command (see
/// [`crate::telemetry`]'s dispatch helpers), but the error is still
/// surfaced for callers (e.g. `--diagnose`) that want to report it.
#[derive(Debug, Error)]
pub enum SpoolError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize spooled record: {0}")]
    Serialize(#[source] serde_json::Error),
}

/// One already-redacted record waiting for delivery.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SpooledRecord {
    pub name: String,
    pub attributes: HashMap<String, TelemetryValue>,
    pub timestamp_unix_millis: u128,
}

/// A bounded, oldest-first-eviction on-disk queue of [`SpooledRecord`]s.
pub struct TelemetrySpool {
    dir: PathBuf,
    max_count: usize,
    max_bytes: u64,
}

impl TelemetrySpool {
    /// The default spool, rooted under Envoy's shared telemetry directory
    /// (honoring `ENVOY_CONFIG_ROOT`).
    pub fn new() -> Self {
        Self::with_dir(telemetry_dir().join("spool"))
    }

    /// A spool rooted at an explicit directory, using the default bounds.
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            dir,
            max_count: DEFAULT_MAX_SPOOL_COUNT,
            max_bytes: DEFAULT_MAX_SPOOL_BYTES,
        }
    }

    /// A spool with explicit bounds, primarily for tests exercising
    /// eviction without needing thousands of records.
    pub fn with_bounds(dir: PathBuf, max_count: usize, max_bytes: u64) -> Self {
        Self {
            dir,
            max_count,
            max_bytes,
        }
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK_FILE_NAME)
    }

    /// All currently-spooled record file paths, oldest first. Filenames are
    /// zero-padded-timestamp-prefixed so lexical sort order is also
    /// chronological order.
    fn spooled_file_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = fs::read_dir(&self.dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension().and_then(|ext| ext.to_str()) == Some(SPOOL_FILE_EXTENSION)
                    })
                    .collect()
            })
            .unwrap_or_default();
        paths.sort();
        paths
    }

    /// Number of records currently spooled, surfaced via `--diagnose`.
    pub fn depth(&self) -> usize {
        self.spooled_file_paths().len()
    }

    /// Append `record` to the spool, then enforce the count/size bounds
    /// (oldest-first eviction), while holding an exclusive lock so
    /// concurrent envoy invocations on the same machine never corrupt the
    /// spool.
    pub fn enqueue(&self, record: &SpooledRecord) -> Result<(), SpoolError> {
        let json = serde_json::to_vec(record).map_err(SpoolError::Serialize)?;

        fs::create_dir_all(&self.dir).map_err(|source| SpoolError::Io {
            path: self.dir.clone(),
            source,
        })?;

        let lock_path = self.lock_path();
        let file_name = spool_file_name(record.timestamp_unix_millis);
        let record_path = self.dir.join(&file_name);
        // `with_exclusive_lock` can fail either because it couldn't
        // acquire/release the lock itself (a `lock_path` problem) or
        // because this closure's own write failed (a `record_path`
        // problem, since `enforce_bounds_locked` never itself returns
        // `Err` -- its own I/O failures are already swallowed). Track
        // which one actually happened so the reported path is accurate.
        let write_failed = Cell::new(false);
        with_exclusive_lock(&lock_path, || {
            let result = fs::write(&record_path, &json);
            if result.is_err() {
                write_failed.set(true);
            }
            result?;
            self.enforce_bounds_locked()
        })
        .map_err(|source| SpoolError::Io {
            path: if write_failed.get() {
                record_path.clone()
            } else {
                lock_path.clone()
            },
            source,
        })
    }

    /// Evict oldest-first until both bounds are satisfied. Must only be
    /// called while holding the exclusive lock.
    fn enforce_bounds_locked(&self) -> std::io::Result<()> {
        // A `VecDeque` (rather than repeated `Vec::remove(0)`, which is
        // O(n) per call and O(n^2) overall across a large eviction) lets
        // oldest-first removal stay O(1) per entry.
        let mut paths: VecDeque<PathBuf> = self.spooled_file_paths().into();
        let mut total_size: u64 = paths
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .sum();

        while !paths.is_empty() && (paths.len() > self.max_count || total_size > self.max_bytes) {
            let oldest = paths.pop_front().expect("checked non-empty above");
            if let Ok(metadata) = fs::metadata(&oldest) {
                total_size = total_size.saturating_sub(metadata.len());
            }
            let _ = fs::remove_file(&oldest);
        }

        Ok(())
    }

    /// Attempt to flush the spool, oldest first, within `budget`.
    ///
    /// `deliver` is called once per record; if it returns `true` the record
    /// is considered delivered and removed, if `false` the flush stops
    /// immediately -- preserving oldest-first delivery order rather than
    /// skipping ahead to a later record whose destination might coincidentally
    /// still be reachable. Returns the number of records successfully
    /// flushed.
    ///
    /// Never blocks longer than `budget` on I/O beyond `deliver`'s own
    /// calls; a large backlog simply gets picked up further across more
    /// invocations rather than delaying any single command.
    ///
    /// Holds the same exclusive lock `enqueue` uses for the whole flush
    /// loop (including `deliver`'s own calls), so enqueue/flush/eviction
    /// are serialized consistently across concurrent `envoy` processes --
    /// without it, one process's flush could read a file mid-write by
    /// another's `enqueue` (spurious corrupt-file deletion) or two
    /// processes could both read and deliver the same record.
    pub fn flush_with_budget(
        &self,
        budget: Duration,
        mut deliver: impl FnMut(&SpooledRecord) -> bool,
    ) -> usize {
        // Mirrors `enqueue`: the lock file can't be created inside a
        // not-yet-existing spool directory, which `spooled_file_paths`
        // otherwise tolerates today (via `unwrap_or_default`).
        if fs::create_dir_all(&self.dir).is_err() {
            return 0;
        }

        let start = Instant::now();
        let mut flushed = 0usize;
        let lock_path = self.lock_path();
        // Best-effort: if the lock itself can't be acquired, simply flush
        // nothing this invocation rather than failing the caller, matching
        // how callers already treat `enqueue`'s own `Result` as best-effort.
        let _ = with_exclusive_lock(&lock_path, || {
            for path in self.spooled_file_paths() {
                if start.elapsed() >= budget {
                    break;
                }

                let record = match fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<SpooledRecord>(&bytes).ok())
                {
                    Some(record) => record,
                    None => {
                        // Unreadable/corrupt spooled file -- drop it rather
                        // than block the flush on it forever.
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                };

                if deliver(&record) {
                    let _ = fs::remove_file(&path);
                    flushed += 1;
                } else {
                    break;
                }
            }
            Ok(())
        });

        flushed
    }
}

impl Default for TelemetrySpool {
    fn default() -> Self {
        Self::new()
    }
}

fn spool_file_name(timestamp_unix_millis: u128) -> String {
    format!(
        "{timestamp_unix_millis:020}-{}.{SPOOL_FILE_EXTENSION}",
        Uuid::new_v4()
    )
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn record(name: &str, timestamp_unix_millis: u128) -> SpooledRecord {
        let mut attributes = HashMap::new();
        attributes.insert(
            "command".to_string(),
            TelemetryValue::Str("unreal".to_string()),
        );
        SpooledRecord {
            name: name.to_string(),
            attributes,
            timestamp_unix_millis,
        }
    }

    #[test]
    fn depth_is_zero_for_an_empty_or_missing_spool() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_dir(temp_dir.path().join("spool"));
        assert_eq!(spool.depth(), 0);
    }

    #[test]
    fn enqueue_increases_depth() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_dir(temp_dir.path().join("spool"));

        spool
            .enqueue(&record("command_run", 1000))
            .expect("should enqueue");
        assert_eq!(spool.depth(), 1);

        spool
            .enqueue(&record("command_run", 2000))
            .expect("should enqueue");
        assert_eq!(spool.depth(), 2);
    }

    #[test]
    fn flush_delivers_oldest_first_and_removes_delivered_records() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_dir(temp_dir.path().join("spool"));

        spool
            .enqueue(&record("first", 1000))
            .expect("should enqueue");
        spool
            .enqueue(&record("second", 2000))
            .expect("should enqueue");
        spool
            .enqueue(&record("third", 3000))
            .expect("should enqueue");

        let mut delivered_order = Vec::new();
        let flushed = spool.flush_with_budget(Duration::from_secs(5), |rec| {
            delivered_order.push(rec.name.clone());
            true
        });

        assert_eq!(flushed, 3);
        assert_eq!(delivered_order, vec!["first", "second", "third"]);
        assert_eq!(spool.depth(), 0);
    }

    #[test]
    fn flush_stops_on_first_failure_preserving_order() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_dir(temp_dir.path().join("spool"));

        spool
            .enqueue(&record("first", 1000))
            .expect("should enqueue");
        spool
            .enqueue(&record("second", 2000))
            .expect("should enqueue");
        spool
            .enqueue(&record("third", 3000))
            .expect("should enqueue");

        let mut attempts = 0;
        let flushed = spool.flush_with_budget(Duration::from_secs(5), |_rec| {
            attempts += 1;
            // Only the first record "delivers" successfully.
            attempts == 1
        });

        assert_eq!(flushed, 1);
        // The two records after the failure must still be spooled, in
        // their original order -- the flush must not skip ahead.
        assert_eq!(spool.depth(), 2);
    }

    #[test]
    fn flush_respects_the_time_budget() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_dir(temp_dir.path().join("spool"));
        spool
            .enqueue(&record("first", 1000))
            .expect("should enqueue");
        spool
            .enqueue(&record("second", 2000))
            .expect("should enqueue");

        let flushed = spool.flush_with_budget(Duration::from_nanos(0), |_rec| true);
        assert_eq!(flushed, 0);
        assert_eq!(spool.depth(), 2);
    }

    #[test]
    fn eviction_is_bounded_by_count_oldest_first() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_bounds(temp_dir.path().join("spool"), 2, u64::MAX);

        spool
            .enqueue(&record("first", 1000))
            .expect("should enqueue");
        spool
            .enqueue(&record("second", 2000))
            .expect("should enqueue");
        spool
            .enqueue(&record("third", 3000))
            .expect("should enqueue");

        assert_eq!(spool.depth(), 2);

        let mut remaining = Vec::new();
        spool.flush_with_budget(Duration::from_secs(5), |rec| {
            remaining.push(rec.name.clone());
            true
        });
        // "first" should have been evicted; only "second" and "third" remain.
        assert_eq!(remaining, vec!["second", "third"]);
    }

    #[test]
    fn eviction_is_bounded_by_total_size() {
        let temp_dir = tempdir().expect("tempdir should be created");
        // A tiny byte budget forces eviction down to whatever fits.
        let spool = TelemetrySpool::with_bounds(temp_dir.path().join("spool"), usize::MAX, 1);

        spool
            .enqueue(&record("first", 1000))
            .expect("should enqueue");
        spool
            .enqueue(&record("second", 2000))
            .expect("should enqueue");

        // With a 1-byte budget neither record can really "fit", but
        // eviction must still leave at most the newest record behind
        // rather than growing without bound.
        assert!(spool.depth() <= 1);
    }

    #[test]
    fn spooled_files_never_contain_unredacted_markers() {
        // This is a structural guarantee, not a redaction test in itself:
        // `enqueue` only ever serializes whatever `SpooledRecord` it is
        // given, so callers (see `crate::telemetry`) are responsible for
        // redacting before constructing one. Pin that a record built from
        // already-redacted attributes round-trips without alteration.
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool = TelemetrySpool::with_dir(temp_dir.path().join("spool"));

        let mut attributes = HashMap::new();
        attributes.insert(
            "envoy.cli.arg.0".to_string(),
            TelemetryValue::Str("***REDACTED***".to_string()),
        );
        let redacted_record = SpooledRecord {
            name: "envoy.command.run".to_string(),
            attributes,
            timestamp_unix_millis: 1000,
        };
        spool.enqueue(&redacted_record).expect("should enqueue");

        let mut flushed_record = None;
        spool.flush_with_budget(Duration::from_secs(5), |rec| {
            flushed_record = Some(rec.clone());
            true
        });

        assert_eq!(flushed_record, Some(redacted_record));
    }

    #[test]
    fn corrupt_spool_file_is_dropped_rather_than_blocking_flush() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let spool_dir = temp_dir.path().join("spool");
        let spool = TelemetrySpool::with_dir(spool_dir.clone());

        spool
            .enqueue(&record("good", 2000))
            .expect("should enqueue");
        fs::create_dir_all(&spool_dir).expect("dir should exist");
        fs::write(
            spool_dir.join("00000000000000001000.json"),
            b"not valid json",
        )
        .expect("should write corrupt file");

        let mut delivered = Vec::new();
        let flushed = spool.flush_with_budget(Duration::from_secs(5), |rec| {
            delivered.push(rec.name.clone());
            true
        });

        assert_eq!(flushed, 1);
        assert_eq!(delivered, vec!["good"]);
        assert_eq!(spool.depth(), 0);
    }
}
