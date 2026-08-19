//! Pseudonymous installation identity.
//!
//! Persists a random UUID under Envoy's shared config root so telemetry can
//! be attributed to a specific workstation over time (e.g. for the
//! "Installation ID" dashboard variable) without ever recording a username
//! or hostname.

use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use crate::user_config::config_root;

/// Subdirectory of the config root holding all telemetry state (spool,
/// installation id).
pub const TELEMETRY_DIR_NAME: &str = "telemetry";

/// Filename holding the persisted installation UUID, under
/// [`telemetry_dir`].
const INSTALLATION_ID_FILE_NAME: &str = "installation_id";

/// Return the directory holding all telemetry state, honoring
/// `ENVOY_CONFIG_ROOT` the same way [`crate::user_config::config_root`]
/// does.
pub fn telemetry_dir() -> PathBuf {
    config_root().join(TELEMETRY_DIR_NAME)
}

/// Return the path to the persisted installation UUID file.
fn installation_id_path() -> PathBuf {
    telemetry_dir().join(INSTALLATION_ID_FILE_NAME)
}

/// Return this workstation's pseudonymous installation UUID, creating and
/// persisting a new random one on first use.
///
/// Never a username or hostname -- a fresh random UUID untethered from any
/// identifying information about the user or machine. Best-effort: if the
/// config root cannot be created, or the file cannot be read or written
/// (e.g. a read-only filesystem), a fresh in-memory UUID is returned for
/// this call without being persisted, rather than failing the invocation.
pub fn installation_id() -> String {
    let path = installation_id_path();

    if let Ok(existing) = fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if Uuid::parse_str(trimmed).is_ok() {
            return trimmed.to_string();
        }
    }

    let generated = Uuid::new_v4().to_string();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, &generated);
    generated
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use tempfile::tempdir;

    use super::*;

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

    #[test]
    fn generates_and_persists_a_valid_uuid_on_first_use() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let temp_dir = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", temp_dir.path());

        let id = installation_id();
        assert!(Uuid::parse_str(&id).is_ok());

        let persisted_path = temp_dir.path().join("telemetry").join("installation_id");
        assert!(persisted_path.exists());
    }

    #[test]
    fn returns_the_same_id_across_repeated_calls() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let temp_dir = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", temp_dir.path());

        let first = installation_id();
        let second = installation_id();
        assert_eq!(first, second);
    }

    #[test]
    fn regenerates_when_the_persisted_file_is_corrupt() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let temp_dir = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", temp_dir.path());

        let telemetry_dir = temp_dir.path().join("telemetry");
        fs::create_dir_all(&telemetry_dir).expect("dir should be created");
        fs::write(telemetry_dir.join("installation_id"), "not-a-uuid")
            .expect("file should be written");

        let id = installation_id();
        assert!(Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn never_derived_from_username_or_hostname() {
        let _lock = crate::env_test_lock::MUTEX
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let temp_dir = tempdir().expect("tempdir should be created");
        let _guard = EnvVarGuard::set("ENVOY_CONFIG_ROOT", temp_dir.path());

        let id = installation_id();
        let username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_default();
        if !username.is_empty() {
            assert!(!id.to_lowercase().contains(&username.to_lowercase()));
        }
    }
}
