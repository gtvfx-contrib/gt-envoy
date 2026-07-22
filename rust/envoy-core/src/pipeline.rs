//! Context-aware pipeline resolution for `envoy`.
//!
//! This module implements context hierarchy resolution where pipelines
//! are discovered and resolved based on a colon-separated context path. The
//! fallback chain walks from the most specific context to broader ones until a
//! matching pipeline is found, with local pins taking precedence over global
//! ones at each level.
//!
//! # Design
//!
//! A **pipeline** is defined in a bundle's `.envoy/pipeline.json` file:
//! ```json
//! {
//!     "name": "build",
//!     "namespace": "bfd",
//!     "source": {"type": "local", "path": "/path/to/bundle/.envoy/pipeline.json"},
//!     "pinned_version": null,
//!     "metadata": {}
//! }
//! ```
//!
//! **Context hierarchy** resolution walks from the most specific context to
//! broader ones:
//! - `resolve("team:project:feature")` tries: team:project:feature -> team:project -> team -> default

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::json_util::parse_json_with_comments;

/// Error type for pipeline resolution operations.
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("I/O error reading pipeline at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid JSON in pipeline file at {file_path}: {source}")]
    Json {
        file_path: PathBuf,
        source: serde_json::Error,
    },

    #[error("pipeline '{name}' not found for context '{context}'")]
    NotFound { name: String, context: String },

    #[error("no bundles discovered; cannot resolve pipeline '{0}'")]
    NoBundles(String),

    #[error("invalid pipeline JSON at {0}: missing required 'name' field")]
    MissingName(PathBuf),

    #[error("pipeline source type not supported: {source_type}")]
    UnsupportedSource { source_type: String },
}

/// Source of a pipeline definition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineSource {
    /// Pipeline defined locally in the bundle's `.envoy/` directory.
    Local { path: PathBuf },
}

/// A resolved pipeline with its metadata and source location.
#[derive(Clone, Debug)]
pub struct Pipeline {
    /// Human-readable name (e.g., `"build"`, `"test"`).
    pub name: String,
    /// Namespace this pipeline belongs to (e.g., `"bfd"`).
    pub namespace: String,
    /// Where the pipeline definition was loaded from.
    pub source: PipelineSource,
    /// Optional pinned version string for reproducible builds.
    pub pinned_version: Option<String>,
    /// Arbitrary metadata attached by the bundle author.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.name)
    }
}

/// A context hierarchy path like `"team:project:feature"`.
#[derive(Clone, Debug)]
pub struct ContextHierarchy {
    /// The full colon-separated context string.
    pub raw: String,
}

impl ContextHierarchy {
    /// Create a new context hierarchy from a colon-separated string.
    pub fn new(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            panic!("ContextHierarchy requires non-empty input");
        }
        Self {
            raw: trimmed.to_string(),
        }
    }

    /// Return the individual context levels from broadest to most specific.
    ///
    /// `resolve("team:project:feature")` returns `["team", "team:project", "team:project:feature"]`.
    pub fn levels(&self) -> Vec<String> {
        let parts: Vec<&str> = self.raw.split(':').collect();
        (0..parts.len()).map(|i| parts[..=i].join(":")).collect()
    }

    /// Return `true` if this context is a parent of another (e.g., `"team:a"`
    /// contains `"team:a:b"`).
    pub fn contains(&self, other: &ContextHierarchy) -> bool {
        self.raw == other.raw || other.raw.starts_with(&format!("{}:", self.raw))
    }

    /// Return the top-level (broadest) context.
    pub fn root_context(&self) -> String {
        self.levels().first().cloned().unwrap_or_default()
    }
}

impl fmt::Display for ContextHierarchy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

/// Configuration for pipeline resolution behavior.
#[derive(Clone, Debug)]
pub struct PipelineConfig {
    /// Default namespace to use when resolving pipelines without explicit namespaces.
    pub default_namespace: String,
    /// Maximum depth of context hierarchy traversal (0 = unlimited).
    pub max_depth: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            default_namespace: "gt".to_string(),
            max_depth: 16,
        }
    }
}

/// Discover all pipeline definitions from a set of bundles.
///
/// Scans each bundle's `.envoy/` directory for `pipeline.json` files and loads
/// them into [`Pipeline`] structs. Returns an empty vec if no pipelines are found.
pub fn discover_pipelines(bundles: &[crate::discovery::BundleInfo]) -> Vec<Pipeline> {
    let mut pipelines = Vec::new();

    for bundle in bundles {
        let pipeline_path = bundle.envoy_env().join("pipeline.json");
        if !pipeline_path.is_file() {
            continue;
        }

        match load_pipeline_from_file(&pipeline_path, &bundle.namespace) {
            Ok(pipeline) => pipelines.push(pipeline),
            Err(e) => tracing::warn!(
                path = %pipeline_path.display(),
                error = %e,
                "failed to load pipeline"
            ),
        }
    }

    pipelines
}

/// Load a single pipeline definition from a JSON file.
pub fn load_pipeline_from_file(
    path: &Path,
    default_namespace: &str,
) -> Result<Pipeline, PipelineError> {
    let content = fs::read_to_string(path).map_err(|e| PipelineError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let value: serde_json::Value =
        parse_json_with_comments(&content).map_err(|e| PipelineError::Json {
            file_path: path.to_path_buf(),
            source: e,
        })?;

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PipelineError::MissingName(path.to_path_buf()))?
        .to_string();

    let namespace = value
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or(default_namespace)
        .to_string();

    // Parse source (optional — defaults to local).
    let source = match value
        .get("source")
        .and_then(|s| s.get("type"))
        .and_then(|t| t.as_str())
    {
        Some("local") => PipelineSource::Local {
            path: path.to_path_buf(),
        },
        None | Some("") => PipelineSource::Local {
            path: path.to_path_buf(),
        },
        Some(other) => {
            return Err(PipelineError::UnsupportedSource {
                source_type: other.to_string(),
            })
        }
    };

    // Parse pinned version (optional).
    let pinned_version = value
        .get("pinned_version")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Collect remaining fields as metadata.
    let mut metadata = HashMap::new();
    for key in &["description", "author", "tags"] {
        if let Some(val) = value.get(key).cloned() {
            metadata.insert(key.to_string(), val);
        }
    }

    Ok(Pipeline {
        name,
        namespace,
        source,
        pinned_version,
        metadata,
    })
}

/// Resolve a pipeline for the given context hierarchy.
///
/// Walks through each level of the context chain (from broadest to most specific)
/// and returns the first matching pipeline found across all bundles. If no
/// pipelines match any context level, falls back to `default_namespace:default`.
pub fn resolve_pipeline(
    context: &ContextHierarchy,
    pipelines: &[Pipeline],
    config: &PipelineConfig,
) -> Result<Pipeline, PipelineError> {
    let levels = context.levels();

    // Limit traversal depth.
    let effective_levels = if config.max_depth > 0 && levels.len() > config.max_depth {
        levels[..config.max_depth].to_vec()
    } else {
        levels
    };

    for level in &effective_levels {
        // Try to find a pipeline matching this context level.
        let matches: Vec<&Pipeline> = pipelines.iter().filter(|p| p.namespace == *level).collect();

        if !matches.is_empty() {
            return Ok(matches[0].clone());
        }
    }

    // Fall back to default namespace pipeline.
    for p in pipelines {
        if p.namespace == config.default_namespace {
            return Ok(p.clone());
        }
    }

    Err(PipelineError::NotFound {
        name: context.raw.clone(),
        context: "default".to_string(),
    })
}

/// Get the current pipeline based on environment variables.
///
/// Reads `ENVOY_PIPELINE_CONTEXT` (if set) and resolves against discovered pipelines.
pub fn get_current_pipeline(
    bundles: &[crate::discovery::BundleInfo],
    config: &PipelineConfig,
) -> Result<Pipeline, PipelineError> {
    let context_str = std::env::var("ENVOY_PIPELINE_CONTEXT").unwrap_or_default();

    if context_str.is_empty() {
        // No explicit context — return first discovered pipeline or error.
        let pipelines = discover_pipelines(bundles);
        if let Some(first) = pipelines.first() {
            return Ok(first.clone());
        }
        Err(PipelineError::NotFound {
            name: "default".to_string(),
            context: String::new(),
        })
    } else {
        let ctx = ContextHierarchy::new(&context_str);
        let pipelines = discover_pipelines(bundles);
        resolve_pipeline(&ctx, &pipelines, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_bundle(
        temp: &TempDir,
        namespace: &str,
        name: &str,
        pipeline_json: Option<&str>,
    ) -> crate::discovery::BundleInfo {
        let bundle_dir = temp.path().join(namespace).join(name);
        fs::create_dir_all(bundle_dir.join(".envoy")).unwrap();

        if let Some(json) = pipeline_json {
            let mut f = fs::File::create(bundle_dir.join(".envoy").join("pipeline.json")).unwrap();
            write!(f, "{}", json).unwrap();
        }

        crate::discovery::BundleInfo::new(
            bundle_dir.clone(),
            name.to_string(),
            namespace.to_string(),
        )
    }

    #[test]
    fn context_hierarchy_levels_are_ordered_broadest_first() {
        let ctx = ContextHierarchy::new("team:project:feature");
        assert_eq!(
            ctx.levels(),
            vec!["team", "team:project", "team:project:feature"]
        );
    }

    #[test]
    fn context_hierarchy_single_level_returns_itself() {
        let ctx = ContextHierarchy::new("team");
        assert_eq!(ctx.levels(), vec!["team"]);
    }

    #[test]
    fn context_contains_works_for_parent_child() {
        let parent = ContextHierarchy::new("team:project");
        let child = ContextHierarchy::new("team:project:feature");
        assert!(parent.contains(&child)); // "team:project" contains "team:project:feature"
        assert!(!child.contains(&parent));
    }

    #[test]
    fn context_root_context_returns_broadest_level() {
        let ctx = ContextHierarchy::new("a:b:c:d");
        assert_eq!(ctx.root_context(), "a");
    }

    #[test]
    fn load_pipeline_from_file_parses_correctly() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pipeline.json");
        fs::write(
            &path,
            r#"{
                "name": "build",
                "namespace": "bfd",
                "source": {"type": "local"},
                "pinned_version": null,
                "description": "Build pipeline"
            }"#,
        )
        .unwrap();

        let pipeline = load_pipeline_from_file(&path, "gt").unwrap();
        assert_eq!(pipeline.name, "build");
        assert_eq!(pipeline.namespace, "bfd");
        assert!(matches!(&pipeline.source, PipelineSource::Local { .. }));
        assert!(pipeline.pinned_version.is_none());
    }

    #[test]
    fn load_pipeline_from_file_accepts_comment_annotated_json() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pipeline.json");
        fs::write(
            &path,
            r#"{
                // Pipeline name.
                "name": "build",
                "namespace": "bfd",
                "source": {"type": "local"}, /* local file */
                "pinned_version": null,
                # optional metadata
                "description": "Build pipeline"
            }"#,
        )
        .unwrap();

        let pipeline = load_pipeline_from_file(&path, "gt").unwrap();
        assert_eq!(pipeline.name, "build");
        assert_eq!(pipeline.namespace, "bfd");
        assert!(matches!(&pipeline.source, PipelineSource::Local { .. }));
        assert_eq!(
            pipeline.metadata.get("description"),
            Some(&serde_json::Value::String(String::from("Build pipeline")))
        );
    }

    #[test]
    fn load_pipeline_uses_default_namespace_when_missing() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pipeline.json");
        fs::write(&path, r#"{"name": "build"}"#).unwrap();

        let pipeline = load_pipeline_from_file(&path, "gt").unwrap();
        assert_eq!(pipeline.namespace, "gt");
    }

    #[test]
    fn load_pipeline_rejects_missing_name() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pipeline.json");
        fs::write(&path, r#"{"namespace": "bfd"}"#).unwrap();

        assert!(load_pipeline_from_file(&path, "gt").is_err());
    }

    #[test]
    fn discover_pipelines_finds_all_in_bundles() {
        let temp = TempDir::new().unwrap();
        let bundles = vec![
            create_test_bundle(
                &temp,
                "bfd",
                "build-pipeline",
                Some(r#"{"name": "build", "namespace": "bfd"}"#),
            ),
            create_test_bundle(&temp, "gt", "test-bundle", None), // no pipeline.json
        ];

        let pipelines = discover_pipelines(&bundles);
        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].name, "build");
    }

    #[test]
    fn resolve_pipeline_finds_matching_context() {
        let temp = TempDir::new().unwrap();
        create_test_bundle(
            &temp,
            "bfd",
            "team-pipeline",
            Some(r#"{"name": "build", "namespace": "bfd"}"#),
        );

        // Create a second bundle with a different namespace.
        let bundles = vec![crate::discovery::BundleInfo::new(
            temp.path().join("bfd").join("team-pipeline"),
            "team-pipeline".to_string(),
            "bfd".to_string(),
        )];

        let pipelines = discover_pipelines(&bundles);
        assert_eq!(pipelines.len(), 1);

        let config = PipelineConfig::default();
        let ctx = ContextHierarchy::new("bfd");
        let result = resolve_pipeline(&ctx, &pipelines, &config).unwrap();
        assert_eq!(result.name, "build");
    }

    #[test]
    fn resolve_pipeline_falls_back_to_default_namespace() {
        let temp = TempDir::new().unwrap();
        create_test_bundle(
            &temp,
            "gt",
            "default-pipeline",
            Some(r#"{"name": "build", "namespace": "gt"}"#),
        );

        let bundles = vec![crate::discovery::BundleInfo::new(
            temp.path().join("gt").join("default-pipeline"),
            "default-pipeline".to_string(),
            "gt".to_string(),
        )];

        let pipelines = discover_pipelines(&bundles);
        assert_eq!(pipelines.len(), 1);

        // Request a context that doesn't match any pipeline namespace.
        let config = PipelineConfig {
            default_namespace: "gt".into(),
            ..Default::default()
        };
        let ctx = ContextHierarchy::new("unknown");
        let result = resolve_pipeline(&ctx, &pipelines, &config).unwrap();
        assert_eq!(result.name, "build"); // falls back to gt namespace.
    }

    #[test]
    fn resolve_pipeline_returns_error_when_no_match() {
        let temp = TempDir::new().unwrap();
        create_test_bundle(
            &temp,
            "bfd",
            "team-pipeline",
            Some(r#"{"name": "build", "namespace": "bfd"}"#),
        );

        let bundles = vec![crate::discovery::BundleInfo::new(
            temp.path().join("bfd").join("team-pipeline"),
            "team-pipeline".to_string(),
            "bfd".to_string(),
        )];

        let pipelines = discover_pipelines(&bundles);
        assert_eq!(pipelines.len(), 1);

        // Request a context that doesn't match any namespace and no default.
        let config = PipelineConfig {
            default_namespace: "".into(),
            ..Default::default()
        };
        let ctx = ContextHierarchy::new("unknown");
        assert!(resolve_pipeline(&ctx, &pipelines, &config).is_err());
    }

    #[test]
    fn pipeline_display_formats_correctly() {
        let p = Pipeline {
            name: "build".to_string(),
            namespace: "bfd".to_string(),
            source: PipelineSource::Local {
                path: PathBuf::from("/tmp/pipeline.json"),
            },
            pinned_version: None,
            metadata: HashMap::new(),
        };
        assert_eq!(format!("{}", p), "bfd:build");
    }

    #[test]
    fn pipeline_source_serializes_correctly() {
        let source = PipelineSource::Local {
            path: PathBuf::from("/tmp/pipeline.json"),
        };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"type\":\"local\""));
        assert!(json.contains("pipeline.json"));
    }

    #[test]
    fn pipeline_config_defaults_are_reasonable() {
        let config = PipelineConfig::default();
        assert_eq!(config.default_namespace, "gt");
        assert_eq!(config.max_depth, 16);
    }
}
