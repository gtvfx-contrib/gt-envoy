# Graph Report - envoy  (2026-07-29)

## Corpus Check
- 80 files · ~80,231 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2090 nodes · 5592 edges · 73 communities (62 shown, 11 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 159 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `48a39239`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Bundle
- envoy-py/src/wrapper.rs
- proc.rs
- user_config.rs
- runtime.rs
- team_config.rs
- envoy-py/src/stack_registry.rs
- envoy-core/src/environment.rs
- cache.rs
- envoy-core/src/wrapper.rs
- bundle_cache.rs
- Stack
- envoy-py/src/commands.rs
- envoy-core/src/commands.rs
- vcs.rs
- executor.rs
- String
- envoy-core/src/telemetry.rs
- semver.rs
- EnvoyError
- envoy-core/src/stack_registry.rs
- util.rs
- Environment
- Self
- package_release.py
- PyResult
- WrapperConfig
- tests/cli.rs
- Vcs
- api.rs
- retry.rs
- scan.rs
- app.rs
- discover_bundles_from_roots
- Environment
- _pythonCommandsFile
- Troubleshooting
- test_proc.py
- cli_main
- Bundle Publishing Workflow (engit publish)
- Stack
- test_consumers.py
- UserConfig
- envoy-py Crate (PyO3 Python Extension)
- main
- test_discovery.py
- Examples
- files.rs
- Installation
- TraceAllowlistEvent
- _envoy
- TestEnvironmentBuild
- TestEnvironmentCall
- envoy-cli/src/lib.rs
- testing.py
- __init__.py
- TestEnvironmentSpawn
- EnvVarGuard
- en
- git_describe
- git_describe
- gen_ref_pages.py
- conftest.py
- TestEnvironmentCheckCall
- Environment File Chaining (Layered Configuration)
- Command Conflicts (Last Bundle Wins)
- envoy
- envoy
- build_native.sh script
- tests.rs
- envoy
- GitHub Actions Reusable Bundle Publish Workflow

## God Nodes (most connected - your core abstractions)
1. `Stack` - 35 edges
2. `_pythonCommandsFile()` - 35 edges
3. `WrapperConfig` - 33 edges
4. `PyPopen` - 25 edges
5. `&T` - 24 edges
6. `SemVer` - 24 edges
7. `Environment` - 24 edges
8. `load_registry()` - 23 edges
9. `SemVer` - 23 edges
10. `ApplicationWrapper` - 23 edges

## Surprising Connections (you probably didn't know these)
- `envoy-core Crate (Framework-Agnostic Logic)` --semantically_similar_to--> `Bundle Discovery (Auto-Discovery Flow)`  [INFERRED] [semantically similar]
  rust/README.md → docs/bundle-discovery.md
- `resolve_stack_value()` --calls--> `is_stack_name()`  [INFERRED]
  rust/envoy-core/src/stack.rs → rust/envoy-core/src/stack_registry.rs
- `Envoy CLI (Environment Orchestration)` --references--> `Bundle (Envoy Distribution Unit)`  [EXTRACTED]
  README.md → docs/concepts.md
- `Deploy Docs Workflow (GitHub Pages + rustdoc)` --conceptually_related_to--> `envoy-py Crate (PyO3 Python Extension)`  [EXTRACTED]
  .github/workflows/deploy-docs.yml → rust/README.md
- `load_registry_for_cli()` --calls--> `open_default_bundle_cache()`  [INFERRED]
  rust/envoy-cli/src/app.rs → rust/envoy-core/src/bundle_cache.rs

## Import Cycles
- 1-file cycle: `rust/envoy-core/src/executor.rs -> rust/envoy-core/src/executor.rs`

## Hyperedges (group relationships)
- **Rust Workspace Crate Architecture** — rust_readme_envoy_core, rust_readme_envoy_py, rust_readme_envoy_cli, rust_readme_engit_core, rust_readme_engit_cli [EXTRACTED 1.00]
- **Envoy Testing Strategy (Unit + Contract + Consumer Smoke)** — github_workflows_lint_yml_lint, rust_envoy_py_tests_python_contract_readme_python_contract, rust_envoy_py_tests_consumer_smoke_readme_consumer_smoke [EXTRACTED 1.00]

## Communities (73 total, 11 thin omitted)

### Community 0 - "Bundle"
Cohesion: 0.08
Nodes (29): bndlid_regex(), bundle_path_var_regex(), expand_bundle_path(), is_bndlid(), namespace_regex(), parse_bndlid(), resolve_bndlid(), Option (+21 more)

### Community 1 - "envoy-py/src/wrapper.rs"
Cohesion: 0.06
Nodes (59): Clone, CoreExecutionResult, CoreWrapperConfig, PyRef, PyTuple, ApplicationWrapper, build_spawn_command(), call_python_noarg() (+51 more)

### Community 2 - "proc.rs"
Cohesion: 0.08
Nodes (76): MutexGuard, PyBytes, is_raw_path(), apply_default_creationflags(), build_cached_environment(), build_returns_same_python_dict_object_on_repeated_calls(), cached_communicate_result(), CachedEnvironment (+68 more)

### Community 3 - "user_config.rs"
Cohesion: 0.11
Nodes (30): default_config_path(), default_config_path_ends_with_expected_filename(), escape_repr_string(), format_settings(), known_setting(), known_settings(), KnownSetting, load_comment_annotated_json_returns_expected_settings() (+22 more)

### Community 4 - "runtime.rs"
Cohesion: 0.08
Nodes (60): BundleCache, any_version_spec(), build_bundle_registry(), bundle_with_file(), collect_env_files(), collect_env_files_errors_when_legacy_env_file_is_missing(), collect_env_files_uses_bundle_indexes_in_multi_bundle_mode(), collect_env_files_uses_legacy_env_dir_and_global_env_first() (+52 more)

### Community 5 - "team_config.rs"
Cohesion: 0.05
Nodes (74): DecodeError, DecryptError, EncryptError, FromUtf8Error, Identity, Recipient, ConfigCryptoError, configured_key_file_path() (+66 more)

### Community 6 - "envoy-py/src/stack_registry.rs"
Cohesion: 0.11
Nodes (32): CoreNamedStackEntry, build_known_settings_dict(), EnvVarGuard, known_settings_matches_python_shape(), list_named_stacks(), list_stack_versions(), NamedStackEntry, path_like_to_pathbuf() (+24 more)

### Community 7 - "envoy-core/src/environment.rs"
Cohesion: 0.09
Nodes (64): D, Number, absolute_lexical_path(), as_array(), core_env_vars(), current_process_env(), diagnose_environment_accepts_comment_annotated_json(), env_items_from_value() (+56 more)

### Community 8 - "cache.rs"
Cohesion: 0.22
Nodes (25): CachedBundleInfo, CachedDirectoryFingerprint, CachedRootFingerprint, discovery_cache_key(), discovery_cache_lock_path(), discovery_cache_path(), DiscoveryCacheEntry, DiscoveryCacheManifest (+17 more)

### Community 9 - "envoy-core/src/wrapper.rs"
Cohesion: 0.06
Nodes (73): AtomicBool, ExecutionResult, ActiveRun, ActiveRunGuard, ApplicationWrapper, build_spawn_command(), callbacks_fire_for_start_stdout_and_stderr(), command_executable() (+65 more)

### Community 10 - "bundle_cache.rs"
Cohesion: 0.10
Nodes (50): BundleCache, BundleCacheError, BundleMeta, cache_deduplicates_identical_content(), cache_list_shows_all_bundles(), cache_removes_bundles(), cache_returns_not_found_for_missing(), cache_stores_and_retrieves_bundles() (+42 more)

### Community 11 - "Stack"
Cohesion: 0.09
Nodes (47): Bundle, Deserialize, NamedStackEntry, create_bundle(), current_stack_honors_environment_user_and_context_precedence(), default_namespace(), EnvVarGuard, expand_home_path() (+39 more)

### Community 12 - "envoy-py/src/commands.rs"
Cohesion: 0.08
Nodes (40): CoreBundleLike, CoreCommandDefinition, CoreCommandRegistry, command_definition_expand_alias_uses_special_vars_and_env_values(), command_registry_round_trips_python_visible_objects(), CommandDefinition, CommandRegistry, create_test_dir() (+32 more)

### Community 13 - "envoy-core/src/commands.rs"
Cohesion: 0.07
Nodes (49): I, Map, absolute_lexical_path(), apply_command_override(), apply_platform_overrides(), Bundle, BundleInfo, BundleLike (+41 more)

### Community 14 - "vcs.rs"
Cohesion: 0.10
Nodes (48): detect(), detect_or_error(), detect_vcs_finds_git_root_from_nested_dir(), detect_vcs_honors_override_before_auto_detection(), find_git_root(), find_lore_root(), find_parent_with(), format_command() (+40 more)

### Community 15 - "executor.rs"
Cohesion: 0.09
Nodes (48): drain_stream(), find_in_path(), has_directory_component(), invoke_callback(), is_batch_script(), is_executable_candidate(), long_running_command(), make_absolute() (+40 more)

### Community 16 - "String"
Cohesion: 0.06
Nodes (14): CoreBundle, CoreBundleInfo, CoreTeamConfig, CoreTraceStepEvent, CoreUserHostConfig, CoreVcsChange, Bundle, BundleInfo (+6 more)

### Community 17 - "envoy-core/src/telemetry.rs"
Cohesion: 0.10
Nodes (32): From, disable(), disable_and_clear_flag(), disable_reverts_to_discarding_events(), enable(), null_sink_is_the_default_and_discards_events(), NullSink, opentelemetry::Value (+24 more)

### Community 18 - "semver.rs"
Cohesion: 0.09
Nodes (33): Err, FromStr, Ord, Ordering, PartialOrd, compare_prerelease(), Constraint, constraint_caret_matches() (+25 more)

### Community 19 - "EnvoyError"
Cohesion: 0.07
Nodes (45): CoreEnvoyError, EnvoyError, Error, Into, Option, PathBuf, Self, String (+37 more)

### Community 20 - "envoy-core/src/stack_registry.rs"
Cohesion: 0.08
Nodes (48): civil_from_days(), current_timestamp(), EnvVarGuard, format_system_time(), is_stack_name(), join_roots(), lexical_normalize(), list_named_stacks() (+40 more)

### Community 21 - "util.rs"
Cohesion: 0.22
Nodes (16): current_timestamp(), infer_namespace(), json_value_to_string(), json_value_truthy(), lexical_normalize(), metadata_modified_timestamp(), name_and_namespace(), normalize_windows_path() (+8 more)

### Community 22 - "Environment"
Cohesion: 0.17
Nodes (19): build_environment(), CachedEnv, Environment, path_like_to_pathbuf(), register_environment_module(), Bound, HashMap, Mutex (+11 more)

### Community 23 - "Self"
Cohesion: 0.08
Nodes (7): CoreConstraint, CoreSemVer, CoreVersionSpec, Constraint, Self, SemVer, VersionSpec

### Community 24 - "package_release.py"
Cohesion: 0.19
Nodes (14): copyReleaseFiles(), main(), normalizeTarMetadata(), parseArguments(), Namespace, Path, Create one platform-specific Envoy release archive., Make Unix archive ownership and permissions host-independent. Args: member:… (+6 more)

### Community 25 - "PyResult"
Cohesion: 0.22
Nodes (8): CoreBundleCache, BundleCache, json_map_to_pyobject(), path_to_py_path(), PyObject, PyResult, Python, Value

### Community 26 - "WrapperConfig"
Cohesion: 0.10
Nodes (24): PostRunCallback, PreRunCallback, execution_result_display_matches_failed_repr(), execution_result_display_matches_success_repr(), execution_result_success_is_false_for_non_zero_exit(), execution_result_success_is_false_for_timeout_even_with_zero_exit(), execution_result_success_is_true_for_zero_exit_without_timeout(), ExecutionResult (+16 more)

### Community 27 - "tests/cli.rs"
Cohesion: 0.19
Nodes (25): Assert, base_command(), command_info_reports_target_and_platform_resolution(), diagnose_with_command_shows_resolved_environment(), diagnose_with_unknown_command_fails_with_clear_error(), diagnose_without_command_summarizes_stack_bundles_and_team(), help_lists_expected_flags(), legacy_bundles_config_flag_is_rejected() (+17 more)

### Community 28 - "Vcs"
Cohesion: 0.15
Nodes (7): CoreVcsAdapter, CoreVcsKind, CoreVcsStatus, Box, Vcs, vcs_kind_name(), VcsStatus

### Community 29 - "api.rs"
Cohesion: 0.16
Nodes (32): CoreTraceEvent, allowlist_contains_envoy_roots_and_extra_values(), allowlist_to_hashset(), build_allowlist(), bundle_infos_to_py(), current_operating_system(), diagnose_environment(), discover_bundles_auto() (+24 more)

### Community 30 - "retry.rs"
Cohesion: 0.17
Nodes (14): E, F, is_transient_error(), retry_config_defaults(), retry_config_no_retry(), retry_sync(), retry_sync_gives_up_after_max_attempts(), retry_sync_retries_on_transient_failure() (+6 more)

### Community 31 - "scan.rs"
Cohesion: 0.38
Nodes (13): discover_bundles_for_root(), find_bundle_roots(), find_git_repos(), has_envoy_env(), is_git_repo(), is_published_bundle(), BundleInfo, Path (+5 more)

### Community 32 - "app.rs"
Cohesion: 0.10
Nodes (51): debug(), display_envoy_error(), ExecutionOptions, find_local_docs(), handle_get_config(), handle_list_configs(), handle_set_config(), init_tracing() (+43 more)

### Community 33 - "discover_bundles_from_roots"
Cohesion: 0.44
Nodes (10): discover_bundles_auto(), discover_bundles_from_roots(), get_bundles(), load_bundles_from_stack(), BundleInfo, Option, Path, Result (+2 more)

### Community 34 - "Environment"
Cohesion: 0.16
Nodes (9): Environment, Tests for Environment properties and repr., whitelist is a deprecated alias that maps to allowlist., Tests for Environment.checkOutput()., Passing stdout= to checkOutput raises ValueError., Passing both input= and stdin= to checkOutput raises ValueError., bytes passed via input= are forwarded to the process stdin., TestEnvironmentCheckOutput (+1 more)

### Community 35 - "_pythonCommandsFile"
Cohesion: 0.17
Nodes (5): _pythonCommandsFile(), Tests for the module-level call / spawn / checkCall / checkOutput. Free…, Envoy CLI flags embedded in cmd (e.g. -cf path) are forwarded., Return a commands.json that defines a 'py' command using ``python``., TestProcFreeFunctions

### Community 36 - "Troubleshooting"
Cohesion: 0.20
Nodes (9): Commands Not Appearing in `--list`, "Could not find commands.json", Environment Variables Not Applying, Envoy Utils Issues, Executable Not Found, Null/Unresolved Variable Warnings, Path Inconsistency (Mixed Slashes), Start Here — `envoy --diagnose` (+1 more)

### Community 37 - "test_proc.py"
Cohesion: 0.15
Nodes (12): _makeBundle(), _makeCommandsDir(), Path, Public-API contract tests for ``envoy.proc``, run against the compiled ``envoy-…, Create a minimal bundle directory tree. Produces:: <tmp_dir>/gt/<name>/ .git/…, End-to-end tests exercising bundle discovery + environment building., Environment variables from a bundle env file reach the subprocess., A command that references another command gets both env files applied. (+4 more)

### Community 38 - "cli_main"
Cohesion: 0.23
Nodes (15): cli_main(), cli_main_defaults_to_sys_argv_when_none(), cli_main_returns_success_for_help_flag(), cli_main_returns_success_for_version_flag(), register_cli_bindings(), register_cli_bindings_adds_cli_main(), Bound, FnOnce (+7 more)

### Community 40 - "Stack"
Cohesion: 0.24
Nodes (3): CoreStack, PyType, Stack

### Community 41 - "test_consumers.py"
Cohesion: 0.14
Nodes (13): _clear_envoy_bndl_roots(), fixture, Consumer smoke tests for ``envoy-py``, exercising the real API call patterns…, ``gt/devtools/py/cleanup_branches.py`` calls ``envoy.proc.spawn(cmd,…, ``gt/krita/wrapper/py/gt/krita/wrapper/__main__.py``'s real pattern:…, ``gt/unreal/wrapper/py/gt/unreal/wrapper/__main__.py``'s real pattern:…, Exercise the VS Code wrapper's Stack-generation code path. Skipped when…, Verify launch selects the generated Stack only in the child process. (+5 more)

### Community 42 - "UserConfig"
Cohesion: 0.22
Nodes (3): CoreUserConfig, RefCell, UserConfig

### Community 43 - "envoy-py Crate (PyO3 Python Extension)"
Cohesion: 0.05
Nodes (50): Bundle Discovery (Auto-Discovery Flow), .bundle Marker File (Version Metadata), Discovery Cache (On-Disk, Short-Lived), ENVOY_BNDL_ROOTS (Bundle Discovery Root), Runtime Stack (.estack YAML), envoy --diagnose (Full Diagnostic Report), envoy --env / -e (Run in Another Command's Environment), Bundle ID (bndlid) (+42 more)

### Community 44 - "main"
Cohesion: 0.24
Nodes (9): main(), parseArguments(), Namespace, Path, Build Envoy's native CLIs and optional Python extension wheel., Run one build command and stop if it fails. Args: arguments: Command and…, Parse command-line arguments for the build driver. Returns: Parsed command-line…, Build the requested Envoy artifacts. Returns: Process exit status. (+1 more)

### Community 45 - "test_discovery.py"
Cohesion: 0.22
Nodes (10): _makeBundle(), Path, Public-API contract tests for bundle discovery, run against the compiled…, Legacy domain names are absent from the clean-break Python API., Create a minimal bundle directory tree (git repo + .envoy marker)., Stack and loadBundlesFromStack() resolve bundle paths from YAML., getBundles() auto-discovers bundles under ENVOY_BNDL_ROOTS., test_auto_discovery() (+2 more)

### Community 46 - "Examples"
Cohesion: 0.22
Nodes (8): Example 1 — Python Development Environment, Example 2 — One Command Across Platforms, Example 3 — Application-Specific Environment, Example 4 — Multi-Bundle Setup, Example 5 — Shared Baseline via `global_env.json`, Example 6 — Optional Site Packages, Example 7 — Layered Dev / Prod Environments, Examples

### Community 47 - "files.rs"
Cohesion: 0.44
Nodes (8): get_bundle_commands_files(), get_bundle_env_files(), BundleInfo, HashMap, PathBuf, String, Vec, get_bundle_file_helpers_collect_expected_files()

### Community 48 - "Installation"
Cohesion: 0.29
Nodes (6): Developer Build, Full Bundle (Recommended), Installation, Python API, Unsigned Artifact Notice, Verify Checksums

### Community 50 - "_envoy"
Cohesion: 0.25
Nodes (5): _envoy(), Bound, PyModule, PyResult, Python

### Community 51 - "TestEnvironmentBuild"
Cohesion: 0.25
Nodes (5): Calling build() twice returns the same object (no re-parse)., CommandNotFoundError when the command does not exist., Tests for Environment.build()., build() returns a dict containing variables from the env file., TestEnvironmentBuild

### Community 52 - "TestEnvironmentCall"
Cohesion: 0.25
Nodes (4): Tests for Environment.call()., call() raises ValueError when stdout=PIPE is requested., call() raises ValueError when stderr=PIPE is requested., TestEnvironmentCall

### Community 53 - "envoy-cli/src/lib.rs"
Cohesion: 0.33
Nodes (4): String, Vec, run(), strings()

### Community 54 - "testing.py"
Cohesion: 0.33
Nodes (6): patchBundleRoots(), patchCommandsFile(), Path, envoy.testing -- Test helpers for code that calls the envoy Python API.…, Context manager that temporarily overrides ``ENVOY_BNDL_ROOTS``. All bundle…, Context manager that temporarily points envoy at a specific ``commands.json``…

### Community 55 - "__init__.py"
Cohesion: 0.17
Nodes (10): async_new_environment(), envoy -- Environment orchestration for managed application execution. This…, Construct an :class:`Environment` without blocking the event loop.…, Main entry point for running envoy as a module. Usage: python -m envoy…, Path, Public-API contract test for the ``?=`` (default) operator, run against the…, Write a JSON env file to a temp directory and return its path., End-to-end: ?= sets variable via ApplicationWrapper when not in env. (+2 more)

### Community 56 - "TestEnvironmentSpawn"
Cohesion: 0.29
Nodes (4): Tests for Environment.spawn()., spawn() returns before the process exits., Variables from the env file are visible inside the spawned process., TestEnvironmentSpawn

### Community 57 - "EnvVarGuard"
Cohesion: 0.29
Nodes (5): EnvVarGuard, Drop, OsStr, OsString, Self

### Community 59 - "git_describe"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 60 - "git_describe"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 61 - "gen_ref_pages.py"
Cohesion: 0.50
Nodes (3): Generate API reference pages for mkdocs, consumed by mkdocs-literate-nav. Runs…, Write a single reference page containing a mkdocstrings directive. Args:…, _writeModulePage()

### Community 62 - "conftest.py"
Cohesion: 0.50
Nodes (3): _clear_envoy_bndl_roots(), fixture, Shared fixtures for the ``envoy-py`` wheel Python contract tests. Autouse-…

### Community 70 - "tests.rs"
Cohesion: 0.14
Nodes (31): bundle_info_display_and_debug_match_python_style(), bundle_reads_marker_version_and_production_state(), bundle_supports_path_specs_bndlid_specs_and_namespace_overrides(), cache_entry_created_at(), create_checkout_bundle(), create_published_bundle(), discover_bundles_from_roots_creates_cache_lock_file(), discover_bundles_from_roots_invalidates_cache_when_bundle_state_changes() (+23 more)

## Knowledge Gaps
- **44 isolated node(s):** `envoy`, `envoy`, `build_native.sh script`, `Example 1 — Python Development Environment`, `Example 2 — One Command Across Platforms` (+39 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **11 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ProcessExecutor` connect `executor.rs` to `app.rs`, `envoy-py/src/wrapper.rs`, `proc.rs`, `envoy-core/src/wrapper.rs`, `vcs.rs`?**
  _High betweenness centrality (0.062) - this node is a cross-community bridge._
- **Why does `&T` connect `envoy-core/src/commands.rs` to `envoy-py/src/wrapper.rs`, `proc.rs`, `runtime.rs`, `team_config.rs`, `tests.rs`, `envoy-core/src/environment.rs`, `cli_main`, `envoy-py/src/stack_registry.rs`, `bundle_cache.rs`, `envoy-py/src/commands.rs`, `EnvoyError`, `envoy-core/src/stack_registry.rs`, `retry.rs`?**
  _High betweenness centrality (0.055) - this node is a cross-community bridge._
- **Why does `async_new_environment()` connect `__init__.py` to `Environment`?**
  _High betweenness centrality (0.048) - this node is a cross-community bridge._
- **What connects `envoy`, `envoy`, `build_native.sh script` to the rest of the system?**
  _44 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Bundle` be split into smaller, more focused modules?**
  _Cohesion score 0.08408163265306122 - nodes in this community are weakly interconnected._
- **Should `envoy-py/src/wrapper.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06455379482902418 - nodes in this community are weakly interconnected._
- **Should `proc.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08337825696316262 - nodes in this community are weakly interconnected._