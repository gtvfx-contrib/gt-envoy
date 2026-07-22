//! Transient-failure retry utilities for envoy-core.
//!
//! Provides exponential-backoff retry for operations that may fail
//! intermittently (file locks, network timeouts, etc.).

use std::time::Duration;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first).
    pub max_attempts: u32,
    /// Initial delay between retries. Doubles each attempt.
    pub initial_delay: Duration,
    /// Maximum delay between any two retries.
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        }
    }
}

impl RetryConfig {
    /// Create a config with the given number of attempts and default delays.
    pub fn with_attempts(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            ..Default::default()
        }
    }

    /// Create a config tuned for fast-fail scenarios (1 attempt = no retry).
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    /// Compute the delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let exponential = self.initial_delay * 2u32.pow(attempt.min(31));
        exponential.min(self.max_delay)
    }
}

/// Retry a fallible synchronous operation with exponential backoff.
///
/// Retries when the error is considered transient (see [`is_transient_error`]).
pub fn retry_sync<F, T, E>(config: &RetryConfig, mut op: F) -> std::result::Result<T, E>
where
    F: FnMut() -> std::result::Result<T, E>,
    E: std::fmt::Display,
{
    let mut last_err = None;

    for attempt in 0..config.max_attempts {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) => {
                let err_msg = err.to_string();
                last_err = Some(err);
                if attempt + 1 < config.max_attempts && is_transient_error(&err_msg) {
                    let delay = config.delay_for_attempt(attempt);
                    std::thread::sleep(delay);
                } else {
                    break;
                }
            }
        }
    }

    Err(last_err.expect("loop body always sets last_err on error path"))
}

/// Determine whether an error string indicates a transient failure worth retrying.
pub fn is_transient_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    // Common transient failure indicators across platforms.
    lower.contains("would block")
        || lower.contains("try again")
        || lower.contains("interrupted")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("resource temporarily unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(5));
    }

    #[test]
    fn retry_config_no_retry() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_attempts, 1);
        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
    }

    #[test]
    fn delay_exponential_growth() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(1),
        };
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(800));
        // Capped at max_delay.
        assert_eq!(config.delay_for_attempt(4), Duration::from_secs(1));
    }

    #[test]
    fn is_transient_error_detection() {
        assert!(is_transient_error("resource temporarily unavailable"));
        assert!(is_transient_error("Connection reset by peer"));
        assert!(is_transient_error("operation interrupted"));
        assert!(!is_transient_error("file not found"));
        assert!(!is_transient_error("permission denied"));
    }

    #[test]
    fn retry_sync_succeeds_on_first_try() {
        let config = RetryConfig::no_retry();
        let result: std::result::Result<i32, &str> = retry_sync(&config, || Ok(42));
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn retry_sync_retries_on_transient_failure() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let config = RetryConfig::with_attempts(3);
        let call_count = AtomicU32::new(0);

        let result: std::result::Result<u32, String> = retry_sync(&config, || {
            let count = call_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count < 3 {
                Err("resource temporarily unavailable".to_string())
            } else {
                Ok(count)
            }
        });

        assert_eq!(result.unwrap(), 3);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn retry_sync_gives_up_after_max_attempts() {
        let config = RetryConfig::with_attempts(2);

        let result: std::result::Result<i32, String> = retry_sync(&config, || {
            Err("resource temporarily unavailable".to_string())
        });

        assert!(result.is_err());
    }
}
