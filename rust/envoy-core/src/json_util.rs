//! Shared JSON parsing helpers.
//!
//! Wraps `serde_json` parsing with comment-stripping support (`//`, `/* */`,
//! and `#` line comments) for envoy's user-authored `.envoy/*.json`
//! configuration files, without changing `serde_json`'s error type or
//! requiring call-site changes to existing error enums.
//!
//! Support for encrypted values remains a separate follow-up because envoy
//! still needs a key-management approach for distribution and rotation before
//! that work can be implemented safely.

use json_comments::StripComments;
use serde::de::DeserializeOwned;

/// Parse `content` as JSON while tolerating supported comment styles.
///
/// This is a drop-in replacement for `serde_json::from_str(content)` at
/// user-authored config call sites. Failures still return the original
/// `serde_json::Error` type, so existing error handling can remain unchanged.
pub fn parse_json_with_comments<T: DeserializeOwned>(content: &str) -> serde_json::Result<T> {
    serde_json::from_reader(StripComments::new(content.as_bytes()))
}
