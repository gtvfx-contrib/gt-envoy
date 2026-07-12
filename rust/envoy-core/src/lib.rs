//! `envoy-core` -- pure Rust port of envoy's framework-agnostic logic.
//!
//! This crate contains no Python or CLI-specific code. It is consumed by:
//! - `envoy-py` (PyO3 bindings, preserving the `import envoy` Python API)
//! - `envoy-cli` (native CLI binary, no Python runtime dependency)
//! - `engit-core` (bundle discovery / named-config resolution for `engit`)
//!
//! Modules are ported from the original Python implementation in
//! `py/envoy/`, module-for-module, to keep behavior traceable during the
//! migration:
//!
//! | Rust module         | Python source           |
//! |----------------------|--------------------------|
//! | [`error`]            | `_exceptions.py`         |
//! | [`models`]           | `_models.py`             |
//! | [`user_config`]      | `_user_config.py`        |
//! | [`config_registry`]  | `_config_registry.py`    |
//! | [`discovery`]        | `_discovery.py`          |
//! | [`environment`]      | `_environment.py`        |
//! | [`commands`]         | `_commands.py`           |
//! | [`executor`]         | `_executor.py`           |
//! | [`wrapper`]          | `_wrapper.py`            |

pub mod error;

pub use error::{EnvoyError, Result};
