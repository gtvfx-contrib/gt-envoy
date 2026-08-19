//! Small generic file-locking helpers for on-disk stores that need to
//! coordinate multiple `envoy` processes running concurrently on the same
//! machine (e.g. the telemetry spool in [`crate::telemetry::spool`]).
//!
//! Built on the same `fs4` crate `bundle_cache` already uses for its own
//! index locking, but generalized over the operation's own error type so
//! any caller can use it without adopting a shared error enum.

use std::fs::{self, File};
use std::path::Path;

use fs4::FileExt;

/// Open (creating if necessary) the sidecar lock file at `lock_path`.
pub fn open_lock_file(lock_path: &Path) -> std::io::Result<File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
}

/// Execute `operation` while holding an exclusive lock on `lock_path`,
/// blocking until the lock is available. The lock is always released
/// afterward, even if `operation` returns an error.
pub fn with_exclusive_lock<T>(
    lock_path: &Path,
    operation: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let lock_file = open_lock_file(lock_path)?;
    FileExt::lock(&lock_file)?;

    let result = operation();
    let unlock_result = FileExt::unlock(&lock_file);

    match result {
        Ok(value) => {
            unlock_result?;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn runs_the_operation_and_returns_its_value() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let lock_path = temp_dir.path().join(".lock");

        let value = with_exclusive_lock(&lock_path, || Ok(42)).expect("should succeed");
        assert_eq!(value, 42);
    }

    #[test]
    fn propagates_the_operation_error() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let lock_path = temp_dir.path().join(".lock");

        let result: std::io::Result<()> =
            with_exclusive_lock(&lock_path, || Err(std::io::Error::other("boom")));
        assert!(result.is_err());
    }

    #[test]
    fn serializes_concurrent_operations_from_two_threads() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let lock_path = temp_dir.path().join(".lock");

        // Hold the lock on the main thread while a second thread tries to
        // acquire it; the second thread must block until the first
        // releases, proving the lock is actually exclusive.
        let lock_file = open_lock_file(&lock_path).expect("should open lock file");
        FileExt::lock(&lock_file).expect("should acquire lock");

        let (tx, rx) = mpsc::channel();
        let thread_lock_path = lock_path.clone();
        let handle = thread::spawn(move || {
            with_exclusive_lock(&thread_lock_path, || Ok(())).expect("should eventually succeed");
            tx.send(()).unwrap();
        });

        // The background thread should still be blocked shortly after
        // starting, since the main thread still holds the lock.
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());

        FileExt::unlock(&lock_file).expect("should release lock");
        handle.join().expect("thread should finish");
    }
}
