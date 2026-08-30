//! Argument redaction for telemetry.
//!
//! Two independent, always-on layers protect argv values before they are
//! ever attached to a telemetry record. Neither layer has a bundle-level
//! off-switch -- only additional sensitive flag names can be layered on top
//! of the built-in list, via `ENVOY_TELEMETRY_REDACT_ARGS`
//! ([`crate::telemetry::config::TELEMETRY_REDACT_ARGS_VAR`]).
//!
//! 1. **Flag-name-based** ([`redact_by_flag_name`]): recognizes sensitive
//!    flag names by convention (`--token foo`, `--api-key=bar`, ...).
//! 2. **Pattern-based, defense-in-depth** ([`redact_by_pattern`]): scans
//!    every value -- positional or flag-associated -- for secret-shaped
//!    substrings regardless of which flag (if any) it followed. Tuned to
//!    avoid false positives on ordinary long identifiers such as bundle IDs,
//!    UUIDs, and content hashes.
//!
//! [`redact_argv`] applies both layers in sequence and is the entry point
//! callers should use.

use std::sync::OnceLock;

use regex::Regex;

/// Placeholder substituted for any redacted value.
pub const REDACTED_PLACEHOLDER: &str = "***REDACTED***";

/// Built-in, always-checked sensitive flag-name substrings
/// (case-insensitive, checked against the flag with leading dashes
/// stripped).
const BUILTIN_SENSITIVE_NAMES: &[&str] = &[
    "token",
    "password",
    "secret",
    "apikey",
    "api-key",
    "api_key",
    "accesskey",
    "access-key",
    "access_key",
    "authorization",
    "credential",
    "privatekey",
    "private-key",
    "private_key",
];

/// Return `true` if `flag_name` (e.g. `"--api-key"`, `"-t"`, or a bare
/// option name like `"api-key"`) matches a built-in sensitive name or one
/// of `extra_names`, by case-insensitive substring match.
pub fn is_sensitive_flag_name(flag_name: &str, extra_names: &[String]) -> bool {
    let normalized = flag_name.trim_start_matches('-').to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    if BUILTIN_SENSITIVE_NAMES
        .iter()
        .any(|name| normalized.contains(name))
    {
        return true;
    }

    extra_names.iter().any(|name| {
        let normalized_extra = name.trim_start_matches('-').to_ascii_lowercase();
        !normalized_extra.is_empty() && normalized.contains(&normalized_extra)
    })
}

/// Redact sensitive values from `argv` by flag name, handling both
/// `--flag value` and `--flag=value` forms.
///
/// Returns a new vector; `argv` itself is never mutated -- telemetry
/// records must always be built from a redacted copy, never the original.
pub fn redact_by_flag_name(argv: &[String], extra_names: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(argv.len());
    let mut redact_next_value = false;

    for arg in argv {
        if redact_next_value {
            result.push(REDACTED_PLACEHOLDER.to_string());
            redact_next_value = false;
            continue;
        }

        if let Some(eq_index) = arg.find('=') {
            let flag = &arg[..eq_index];
            if flag.starts_with('-') && is_sensitive_flag_name(flag, extra_names) {
                result.push(format!("{flag}={REDACTED_PLACEHOLDER}"));
                continue;
            }
            result.push(arg.clone());
            continue;
        }

        if arg.starts_with('-') && is_sensitive_flag_name(arg, extra_names) {
            result.push(arg.clone());
            redact_next_value = true;
            continue;
        }

        result.push(arg.clone());
    }

    result
}

fn looks_like_jwt(value: &str) -> bool {
    static JWT_RE: OnceLock<Regex> = OnceLock::new();
    let re = JWT_RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}$")
            .expect("JWT regex should compile")
    });
    re.is_match(value)
}

fn looks_like_bearer_header(value: &str) -> bool {
    static BEARER_RE: OnceLock<Regex> = OnceLock::new();
    let re = BEARER_RE
        .get_or_init(|| Regex::new(r"(?i)^bearer\s+\S{8,}$").expect("bearer regex should compile"));
    re.is_match(value)
}

fn looks_like_connection_string_credential(value: &str) -> bool {
    static URL_CRED_RE: OnceLock<Regex> = OnceLock::new();
    let url_cred_re = URL_CRED_RE.get_or_init(|| {
        Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.\-]*://[^/\s:@]+:[^/\s@]+@")
            .expect("connection-string regex should compile")
    });
    if url_cred_re.is_match(value) {
        return true;
    }

    static KV_CRED_RE: OnceLock<Regex> = OnceLock::new();
    let kv_cred_re = KV_CRED_RE.get_or_init(|| {
        Regex::new(r"(?i)(^|;)\s*(password|pwd|accountkey|secret)\s*=\s*[^;]+")
            .expect("connection-string kv regex should compile")
    });
    // Only treat this as a connection string (rather than e.g. a single
    // `--flag=value` pair, already covered by the flag-based layer) when it
    // has the semicolon-delimited shape a real connection string has.
    value.contains(';') && kv_cred_re.is_match(value)
}

fn looks_like_uuid(value: &str) -> bool {
    static UUID_RE: OnceLock<Regex> = OnceLock::new();
    let re = UUID_RE.get_or_init(|| {
        Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
            .expect("UUID regex should compile")
    });
    re.is_match(value)
}

/// Minimum length before a value is even considered for the high-entropy
/// heuristic. Chosen to comfortably exceed ordinary bundle IDs, semantic
/// versions, and short flags while still catching realistic API keys/tokens.
const HIGH_ENTROPY_MIN_LENGTH: usize = 24;

/// Return `true` if `value` looks like a long, high-entropy token (e.g. a
/// generated API key) rather than an ordinary identifier.
///
/// Deliberately excludes shapes that are extremely common as legitimate,
/// non-secret CLI arguments in this codebase and would otherwise be
/// false positives: plain hex strings (git SHAs, content hashes) and
/// UUIDs (bundle/installation identifiers) both have long runs of
/// same-class characters and would otherwise look "random" to a naive
/// length-only check.
fn looks_like_high_entropy_token(value: &str) -> bool {
    if value.len() < HIGH_ENTROPY_MIN_LENGTH {
        return false;
    }

    static TOKEN_CHARSET_RE: OnceLock<Regex> = OnceLock::new();
    let charset_re = TOKEN_CHARSET_RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9+/_=.\-]+$").expect("charset regex should compile")
    });
    if !charset_re.is_match(value) {
        return false;
    }

    let is_hex_only = value.chars().all(|c| c.is_ascii_hexdigit());
    if is_hex_only || looks_like_uuid(value) {
        return false;
    }

    let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let has_symbol = value.chars().any(|c| "+/_=.-".contains(c));
    let class_count = [has_upper, has_lower, has_digit, has_symbol]
        .into_iter()
        .filter(|present| *present)
        .count();

    // Requiring at least 3 of 4 character classes (rather than just
    // "long enough") is what keeps this from flagging long-but-uniform
    // identifiers such as all-lowercase slugs.
    class_count >= 3
}

/// Return `true` if `value` matches any recognized secret-shaped pattern,
/// independent of whichever flag (if any) it followed.
pub fn looks_like_secret(value: &str) -> bool {
    looks_like_jwt(value)
        || looks_like_bearer_header(value)
        || looks_like_connection_string_credential(value)
        || looks_like_high_entropy_token(value)
}

/// Redact every value in `argv` that matches a recognized secret-shaped
/// pattern (see [`looks_like_secret`]), regardless of which flag (if any)
/// it followed. This is the defense-in-depth layer: it runs in addition to,
/// not instead of, [`redact_by_flag_name`].
pub fn redact_by_pattern(argv: &[String]) -> Vec<String> {
    argv.iter()
        .map(|value| {
            if looks_like_secret(value) {
                REDACTED_PLACEHOLDER.to_string()
            } else {
                value.clone()
            }
        })
        .collect()
}

/// Redact `argv` using both layers: flag-name-based first, then
/// pattern-based defense-in-depth on the result. This is the entry point
/// callers should use; there is no way to disable either layer.
pub fn redact_argv(argv: &[String], extra_sensitive_names: &[String]) -> Vec<String> {
    let flag_redacted = redact_by_flag_name(argv, extra_sensitive_names);
    redact_by_pattern(&flag_redacted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn redacts_space_separated_sensitive_flag_value() {
        let argv = strings(&["--token", "abc123secretvalue", "run"]);
        let result = redact_by_flag_name(&argv, &[]);
        assert_eq!(result, strings(&["--token", REDACTED_PLACEHOLDER, "run"]));
    }

    #[test]
    fn redacts_equals_separated_sensitive_flag_value() {
        let argv = strings(&["--api-key=abc123secretvalue", "run"]);
        let result = redact_by_flag_name(&argv, &[]);
        assert_eq!(
            result,
            strings(&[&format!("--api-key={REDACTED_PLACEHOLDER}"), "run"])
        );
    }

    #[test]
    fn sensitive_flag_matching_is_case_insensitive() {
        assert!(is_sensitive_flag_name("--TOKEN", &[]));
        assert!(is_sensitive_flag_name("--Access-Key", &[]));
    }

    #[test]
    fn extra_redact_args_extend_the_built_in_list() {
        let argv = strings(&["--custom-secret-flag", "hidden-value"]);
        let extra = vec!["custom-secret-flag".to_string()];
        let result = redact_by_flag_name(&argv, &extra);
        assert_eq!(
            result,
            strings(&["--custom-secret-flag", REDACTED_PLACEHOLDER])
        );
    }

    #[test]
    fn leaves_ordinary_flags_and_values_untouched() {
        let argv = strings(&["--stack", "gt:pythoncore", "--verbose"]);
        let result = redact_by_flag_name(&argv, &[]);
        assert_eq!(result, argv);
    }

    #[test]
    fn pattern_layer_redacts_jwt_shaped_value_regardless_of_flag() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PYb4tSau1XSU";
        let argv = strings(&["--positional-arg-not-a-flag", jwt]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result[1], REDACTED_PLACEHOLDER);
    }

    #[test]
    fn pattern_layer_redacts_bearer_header_value() {
        let argv = strings(&["Bearer abcdefghijklmnopqrstuvwxyz0123456789"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result[0], REDACTED_PLACEHOLDER);
    }

    #[test]
    fn pattern_layer_redacts_connection_string_credential() {
        let argv = strings(&["Server=tcp:db.example.com;Password=SuperSecret123!;Database=prod"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result[0], REDACTED_PLACEHOLDER);
    }

    #[test]
    fn pattern_layer_redacts_url_embedded_credentials() {
        let argv = strings(&["postgres://admin:hunter2pass@db.example.com:5432/prod"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result[0], REDACTED_PLACEHOLDER);
    }

    #[test]
    fn pattern_layer_redacts_long_high_entropy_token() {
        let argv = strings(&["aB3xQ9zK_mP7vR2wL8yN5tU1sC6dF4gH0j="]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result[0], REDACTED_PLACEHOLDER);
    }

    // False-positive avoidance: explicitly required by the test plan.
    // Ordinary long identifiers must survive the pattern-based layer
    // untouched.

    #[test]
    fn does_not_redact_bundle_ids() {
        let argv = strings(&["gt:pythoncore", "gt:some-longer-bundle-name-example"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result, argv);
    }

    #[test]
    fn does_not_redact_git_sha_hashes() {
        let argv = strings(&[
            "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678",
            "0123456789abcdef0123456789abcdef01234567",
        ]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result, argv);
    }

    #[test]
    fn does_not_redact_uuids() {
        let argv = strings(&["550e8400-e29b-41d4-a716-446655440000"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result, argv);
    }

    #[test]
    fn does_not_redact_semantic_versions_or_short_flags() {
        let argv = strings(&["--stack", "1.2.3", "-v", "--verbose"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result, argv);
    }

    #[test]
    fn does_not_redact_ordinary_file_paths() {
        let argv = strings(&["C:\\Users\\artist\\projects\\shot010\\scene.max"]);
        let result = redact_by_pattern(&argv);
        assert_eq!(result, argv);
    }

    #[test]
    fn redact_argv_applies_both_layers_together() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PYb4tSau1XSU";
        let argv = strings(&["--password", "hunter2", "--stack", "gt:pythoncore", jwt]);
        let result = redact_argv(&argv, &[]);
        assert_eq!(
            result,
            strings(&[
                "--password",
                REDACTED_PLACEHOLDER,
                "--stack",
                "gt:pythoncore",
                REDACTED_PLACEHOLDER,
            ])
        );
    }

    #[test]
    fn redact_argv_has_no_way_to_disable_either_layer() {
        // There is deliberately no parameter anywhere in this module that
        // turns off redaction -- `redact_argv` always applies both layers.
        // This test pins that contract: even with an empty extra-names
        // list, both built-in layers still fire.
        let argv = strings(&["--token", "abc", "postgres://u:p@host/db"]);
        let result = redact_argv(&argv, &[]);
        assert_eq!(result[1], REDACTED_PLACEHOLDER);
        assert_eq!(result[2], REDACTED_PLACEHOLDER);
    }
}
