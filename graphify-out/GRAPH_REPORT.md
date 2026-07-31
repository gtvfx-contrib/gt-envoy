# Graph Report - envoy  (2026-07-31)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 2170 nodes · 5693 edges · 85 communities (61 shown, 24 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 143 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- envoy-py/src/wrapper.rs
- proc.rs
- tests.rs
- team_config.rs
- envoy-core/src/environment.rs
- envoy-core/src/wrapper.rs
- bundle_cache.rs
- Bundle
- envoy-core/src/commands.rs
- Stack
- envoy-py/src/commands.rs
- runtime.rs
- vcs.rs
- executor.rs
- String
- envoy-core/src/stack_registry.rs
- semver.rs
- app.rs
- user_config.rs
- envoy-py Crate (PyO3 Python Extension)
- envoy-py/src/stack_registry.rs
- envoy-core/src/telemetry.rs
- exceptions.rs
- Environment
- Option
- Self
- release_automation.py
- tests/cli.rs
- PyResult
- WrapperConfig
- retry.rs
- _pythonCommandsFile
- test_wrapper.py
- Vec
- Environment
- test_proc.py
- cli_main
- package_release.py
- test_consumers.py
- api.rs
- test_release_automation.py
- Bundle
- test_discovery.py
- UserConfig
- Troubleshooting
- main
- TraceAllowlistEvent
- Examples
- TestEnvironmentCheckOutput
- _envoy
- TestEnvironmentBuild
- Installation
- envoy-cli/src/lib.rs
- testing.py
- TestEnvironmentProperties
- __init__.py
- test_user_config.py
- git_describe
- git_describe
- gen_ref_pages.py
- conftest.py
- en
- envoy
- Environment File Chaining (Layered Configuration)
- build_native.sh script
- Bundle Publishing Workflow (engit publish)
- Command Conflicts (Last Bundle Wins)
- GitHub Actions Reusable Bundle Publish Workflow
- envoy
- Command
- Default
- Duration
- VersionSpec
- Regex
- VersionSpec
- envoy
- Box
- PyType
- Child
- ExitStatus
- JoinHandle
- Mutex
- fixture

## God Nodes (most connected - your core abstractions)
1. `_pythonCommandsFile()` - 35 edges
2. `Stack` - 35 edges
3. `WrapperConfig` - 33 edges
4. `PyPopen` - 25 edges
5. `SemVer` - 24 edges
6. `Environment` - 24 edges
7. `&T` - 24 edges
8. `Environment` - 24 edges
9. `ApplicationWrapper` - 23 edges
10. `SemVer` - 23 edges

## Surprising Connections (you probably didn't know these)
- `envoy-core Crate (Framework-Agnostic Logic)` --semantically_similar_to--> `Bundle Discovery (Auto-Discovery Flow)`  [INFERRED] [semantically similar]
  rust/README.md → docs/bundle-discovery.md
- `Envoy CLI (Environment Orchestration)` --references--> `Bundle (Envoy Distribution Unit)`  [EXTRACTED]
  README.md → docs/concepts.md
- `Deploy Docs Workflow (GitHub Pages + rustdoc)` --conceptually_related_to--> `envoy-py Crate (PyO3 Python Extension)`  [EXTRACTED]
  .github/workflows/deploy-docs.yml → rust/README.md
- `build_environment()` --calls--> `open_default_bundle_cache()`  [INFERRED]
  rust/envoy-py/src/environment.rs → rust/envoy-core/src/bundle_cache.rs
- `build_environment()` --calls--> `is_raw_path()`  [INFERRED]
  rust/envoy-py/src/environment.rs → rust/envoy-core/src/runtime.rs

## Import Cycles
- 1-file cycle: `rust/envoy-core/src/executor.rs -> rust/envoy-core/src/executor.rs`
- 2-file cycle: `rust/envoy-core/src/runtime.rs -> rust/envoy-py/src/api.rs -> rust/envoy-core/src/runtime.rs`
- 3-file cycle: `rust/envoy-core/src/runtime.rs -> rust/envoy-py/src/api.rs -> rust/envoy-py/src/proc.rs -> rust/envoy-core/src/runtime.rs`

## Hyperedges (group relationships)
- **Rust Workspace Crate Architecture** — rust_readme_envoy_core, rust_readme_envoy_py, rust_readme_envoy_cli, rust_readme_engit_core, rust_readme_engit_cli [EXTRACTED 1.00]
- **Envoy Testing Strategy (Unit + Contract + Consumer Smoke)** — github_workflows_lint_yml_lint, rust_envoy_py_tests_python_contract_readme_python_contract, rust_envoy_py_tests_consumer_smoke_readme_consumer_smoke [EXTRACTED 1.00]

## Communities (85 total, 24 thin omitted)

### Community 0 - "envoy-py/src/wrapper.rs"
Cohesion: 0.06
Nodes (59): Clone, CoreExecutionResult, CoreWrapperConfig, PyRef, PyTuple, ApplicationWrapper, build_spawn_command(), call_python_noarg() (+51 more)

### Community 1 - "proc.rs"
Cohesion: 0.08
Nodes (76): Child, ExitStatus, JoinHandle, Mutex, MutexGuard, PyBytes, is_raw_path(), apply_default_creationflags() (+68 more)

### Community 2 - "tests.rs"
Cohesion: 0.05
Nodes (94): CachedBundleInfo, CachedDirectoryFingerprint, CachedRootFingerprint, discovery_cache_key(), discovery_cache_lock_path(), discovery_cache_path(), DiscoveryCacheEntry, DiscoveryCacheManifest (+86 more)

### Community 3 - "team_config.rs"
Cohesion: 0.06
Nodes (72): DecodeError, DecryptError, EncryptError, FromUtf8Error, Identity, Recipient, ConfigCryptoError, configured_key_file_path() (+64 more)

### Community 4 - "envoy-core/src/environment.rs"
Cohesion: 0.08
Nodes (65): D, Number, absolute_lexical_path(), as_array(), core_env_vars(), current_process_env(), diagnose_environment_accepts_comment_annotated_json(), env_items_from_value() (+57 more)

### Community 5 - "envoy-core/src/wrapper.rs"
Cohesion: 0.07
Nodes (62): AtomicBool, ExecutionResult, ActiveRun, ActiveRunGuard, ApplicationWrapper, build_spawn_command(), callbacks_fire_for_start_stdout_and_stderr(), command_executable() (+54 more)

### Community 6 - "bundle_cache.rs"
Cohesion: 0.09
Nodes (52): Default, Duration, BundleCache, BundleCacheError, BundleMeta, cache_deduplicates_identical_content(), cache_list_shows_all_bundles(), cache_removes_bundles() (+44 more)

### Community 7 - "Bundle"
Cohesion: 0.05
Nodes (52): Regex, bndlid_regex(), bundle_path_var_regex(), expand_bundle_path(), is_bndlid(), namespace_regex(), parse_bndlid(), resolve_bndlid() (+44 more)

### Community 8 - "envoy-core/src/commands.rs"
Cohesion: 0.07
Nodes (48): I, Map, absolute_lexical_path(), apply_command_override(), apply_platform_overrides(), Bundle, BundleInfo, BundleLike (+40 more)

### Community 9 - "Stack"
Cohesion: 0.08
Nodes (48): Bundle, Deserialize, NamedStackEntry, create_bundle(), current_stack_honors_environment_user_and_context_precedence(), default_namespace(), EnvVarGuard, expand_home_path() (+40 more)

### Community 10 - "envoy-py/src/commands.rs"
Cohesion: 0.08
Nodes (40): CoreBundleLike, CoreCommandDefinition, CoreCommandRegistry, command_definition_expand_alias_uses_special_vars_and_env_values(), command_registry_round_trips_python_visible_objects(), CommandDefinition, CommandRegistry, create_test_dir() (+32 more)

### Community 11 - "runtime.rs"
Cohesion: 0.09
Nodes (59): BundleCache, any_version_spec(), build_bundle_registry(), bundle_with_file(), collect_env_files(), collect_env_files_errors_when_legacy_env_file_is_missing(), collect_env_files_uses_bundle_indexes_in_multi_bundle_mode(), collect_env_files_uses_legacy_env_dir_and_global_env_first() (+51 more)

### Community 12 - "vcs.rs"
Cohesion: 0.10
Nodes (48): detect(), detect_or_error(), detect_vcs_finds_git_root_from_nested_dir(), detect_vcs_honors_override_before_auto_detection(), find_git_root(), find_lore_root(), find_parent_with(), format_command() (+40 more)

### Community 13 - "executor.rs"
Cohesion: 0.09
Nodes (48): drain_stream(), find_in_path(), has_directory_component(), invoke_callback(), is_batch_script(), is_executable_candidate(), long_running_command(), make_absolute() (+40 more)

### Community 14 - "String"
Cohesion: 0.06
Nodes (17): Box, CoreTeamConfig, CoreTraceStepEvent, CoreUserHostConfig, CoreVcsAdapter, CoreVcsChange, CoreVcsKind, CoreVcsStatus (+9 more)

### Community 15 - "envoy-core/src/stack_registry.rs"
Cohesion: 0.08
Nodes (47): civil_from_days(), current_timestamp(), EnvVarGuard, format_system_time(), join_roots(), lexical_normalize(), list_named_stacks(), list_named_stacks_deduplicates_by_first_root_and_sorts_by_name() (+39 more)

### Community 16 - "semver.rs"
Cohesion: 0.09
Nodes (33): Err, FromStr, Ord, Ordering, PartialOrd, compare_prerelease(), Constraint, constraint_caret_matches() (+25 more)

### Community 17 - "app.rs"
Cohesion: 0.10
Nodes (52): debug(), display_envoy_error(), ExecutionOptions, find_local_docs(), handle_get_config(), handle_list_configs(), handle_set_config(), init_tracing() (+44 more)

### Community 18 - "user_config.rs"
Cohesion: 0.08
Nodes (39): config_root(), default_config_path(), default_config_path_ends_with_expected_filename(), default_config_root(), EnvVarGuard, escape_repr_string(), explicit_load_path_takes_precedence_over_config_root(), format_settings() (+31 more)

### Community 19 - "envoy-py Crate (PyO3 Python Extension)"
Cohesion: 0.05
Nodes (50): Bundle Discovery (Auto-Discovery Flow), .bundle Marker File (Version Metadata), Discovery Cache (On-Disk, Short-Lived), ENVOY_BNDL_ROOTS (Bundle Discovery Root), Runtime Stack (.estack YAML), envoy --diagnose (Full Diagnostic Report), envoy --env / -e (Run in Another Command's Environment), Bundle ID (bndlid) (+42 more)

### Community 20 - "envoy-py/src/stack_registry.rs"
Cohesion: 0.11
Nodes (33): CoreNamedStackEntry, build_known_settings_dict(), config_root_function_is_dynamic_but_user_config_path_is_frozen(), EnvVarGuard, get_config_root(), known_settings_matches_python_shape(), list_named_stacks(), list_stack_versions() (+25 more)

### Community 21 - "envoy-core/src/telemetry.rs"
Cohesion: 0.10
Nodes (32): From, disable(), disable_and_clear_flag(), disable_reverts_to_discarding_events(), enable(), null_sink_is_the_default_and_discards_events(), NullSink, opentelemetry::Value (+24 more)

### Community 22 - "exceptions.rs"
Cohesion: 0.10
Nodes (34): CoreEnvoyError, add_exception_types(), assert_issubclass(), assert_maps_to(), called_process_error(), called_process_errors_expose_process_attributes(), envoy_error_to_pyerr(), envoy_error_variants_map_to_expected_exception_types() (+26 more)

### Community 23 - "Environment"
Cohesion: 0.17
Nodes (19): build_environment(), CachedEnv, Environment, path_like_to_pathbuf(), register_environment_module(), Bound, HashMap, Mutex (+11 more)

### Community 24 - "Option"
Cohesion: 0.16
Nodes (22): CoreStack, CoreTraceEvent, PyType, open_default_bundle_cache(), allowlist_to_hashset(), diagnose_environment(), get_current_stack(), get_current_team_config() (+14 more)

### Community 25 - "Self"
Cohesion: 0.08
Nodes (7): CoreConstraint, CoreSemVer, CoreVersionSpec, Constraint, Self, SemVer, VersionSpec

### Community 26 - "release_automation.py"
Cohesion: 0.11
Nodes (31): ArgumentParser, buildParser(), changedFiles(), checkRelease(), classifyImpact(), lockfileHasDependencyChanges(), main(), prepareRelease() (+23 more)

### Community 27 - "tests/cli.rs"
Cohesion: 0.19
Nodes (26): Assert, Command, base_command(), command_info_reports_target_and_platform_resolution(), diagnose_with_command_shows_resolved_environment(), diagnose_with_unknown_command_fails_with_clear_error(), diagnose_without_command_summarizes_stack_bundles_and_team(), help_lists_expected_flags() (+18 more)

### Community 28 - "PyResult"
Cohesion: 0.21
Nodes (9): CoreBundleCache, BundleCache, get_allowlist(), json_map_to_pyobject(), path_to_py_path(), PyObject, PyResult, Python (+1 more)

### Community 29 - "WrapperConfig"
Cohesion: 0.10
Nodes (24): PostRunCallback, PreRunCallback, execution_result_display_matches_failed_repr(), execution_result_display_matches_success_repr(), execution_result_success_is_false_for_non_zero_exit(), execution_result_success_is_false_for_timeout_even_with_zero_exit(), execution_result_success_is_true_for_zero_exit_without_timeout(), ExecutionResult (+16 more)

### Community 30 - "retry.rs"
Cohesion: 0.17
Nodes (14): E, F, is_transient_error(), retry_config_defaults(), retry_config_no_retry(), retry_sync(), retry_sync_gives_up_after_max_attempts(), retry_sync_retries_on_transient_failure() (+6 more)

### Community 31 - "_pythonCommandsFile"
Cohesion: 0.17
Nodes (5): _pythonCommandsFile(), Tests for the module-level call / spawn / checkCall / checkOutput. Free…, Envoy CLI flags embedded in cmd (e.g. -cf path) are forwarded., Return a commands.json that defines a 'py' command using ``python``., TestProcFreeFunctions

### Community 32 - "test_wrapper.py"
Cohesion: 0.11
Nodes (16): Public-API contract tests for ``ApplicationWrapper``, run against the compiled…, Test timeout functionality., Test event callbacks., Test createWrapper convenience function., Test working directory., Test basic command execution., Test environment variable passing., Test pre and post run operations. (+8 more)

### Community 33 - "Vec"
Cohesion: 0.22
Nodes (8): CoreBundleInfo, bundle_infos_to_py(), BundleInfo, discover_bundles_auto(), get_bundles(), load_bundles_from_stack(), Py, Vec

### Community 34 - "Environment"
Cohesion: 0.18
Nodes (8): Environment, Tests for Environment.call()., call() raises ValueError when stdout=PIPE is requested., spawn() returns before the process exits., Variables from the env file are visible inside the spawned process., TestEnvironmentCall, TestEnvironmentCheckCall, TestEnvironmentSpawn

### Community 35 - "test_proc.py"
Cohesion: 0.15
Nodes (12): _makeBundle(), _makeCommandsDir(), Path, Public-API contract tests for ``envoy.proc``, run against the compiled ``envoy-…, Create a minimal bundle directory tree. Produces:: <tmp_dir>/gt/<name>/ .git/…, End-to-end tests exercising bundle discovery + environment building., Environment variables from a bundle env file reach the subprocess., A command that references another command gets both env files applied. (+4 more)

### Community 36 - "cli_main"
Cohesion: 0.23
Nodes (15): cli_main(), cli_main_defaults_to_sys_argv_when_none(), cli_main_returns_success_for_help_flag(), cli_main_returns_success_for_version_flag(), register_cli_bindings(), register_cli_bindings_adds_cli_main(), Bound, FnOnce (+7 more)

### Community 37 - "package_release.py"
Cohesion: 0.19
Nodes (14): copyReleaseFiles(), main(), normalizeTarMetadata(), parseArguments(), Namespace, Path, Create one platform-specific Envoy release archive., Make Unix archive ownership and permissions host-independent. Args: member:… (+6 more)

### Community 38 - "test_consumers.py"
Cohesion: 0.14
Nodes (13): fixture, _clear_envoy_bndl_roots(), Consumer smoke tests for ``envoy-py``, exercising the real API call patterns…, ``gt/devtools/py/cleanup_branches.py`` calls ``envoy.proc.spawn(cmd,…, ``gt/krita/wrapper/py/gt/krita/wrapper/__main__.py``'s real pattern:…, ``gt/unreal/wrapper/py/gt/unreal/wrapper/__main__.py``'s real pattern:…, Exercise the VS Code wrapper's Stack-generation code path. Skipped when…, Verify launch selects the generated Stack only in the child process. (+5 more)

### Community 39 - "api.rs"
Cohesion: 0.21
Nodes (11): allowlist_contains_envoy_roots_and_extra_values(), build_allowlist(), current_operating_system(), map_operating_system_name(), register_api_bindings(), EnvoyError, Path, PyErr (+3 more)

### Community 40 - "test_release_automation.py"
Cohesion: 0.17
Nodes (12): parametrize, Tests for Envoy release automation., Valid SemVer values are returned unchanged., Invalid release values are rejected., The workspace version replacement leaves dependency versions alone., Preparation asks Cargo to refresh the lockfile before validation., Compatibility testing removes the exact remote pin in its temporary copy., testPrepareReleaseRefreshesAndChecksLockfile() (+4 more)

### Community 42 - "test_discovery.py"
Cohesion: 0.22
Nodes (10): _makeBundle(), Path, Public-API contract tests for bundle discovery, run against the compiled…, Legacy domain names are absent from the clean-break Python API., Create a minimal bundle directory tree (git repo + .envoy marker)., Stack and loadBundlesFromStack() resolve bundle paths from YAML., getBundles() auto-discovers bundles under ENVOY_BNDL_ROOTS., test_auto_discovery() (+2 more)

### Community 43 - "UserConfig"
Cohesion: 0.22
Nodes (3): CoreUserConfig, RefCell, UserConfig

### Community 44 - "Troubleshooting"
Cohesion: 0.20
Nodes (9): Commands Not Appearing in `--list`, "Could not find commands.json", Environment Variables Not Applying, Envoy Utils Issues, Executable Not Found, Null/Unresolved Variable Warnings, Path Inconsistency (Mixed Slashes), Start Here — `envoy --diagnose` (+1 more)

### Community 45 - "main"
Cohesion: 0.24
Nodes (9): main(), parseArguments(), Namespace, Path, Build Envoy's native CLIs and optional Python extension wheel., Run one build command and stop if it fails. Args: arguments: Command and…, Parse command-line arguments for the build driver. Returns: Parsed command-line…, Build the requested Envoy artifacts. Returns: Process exit status. (+1 more)

### Community 47 - "Examples"
Cohesion: 0.22
Nodes (8): Example 1 — Python Development Environment, Example 2 — One Command Across Platforms, Example 3 — Application-Specific Environment, Example 4 — Multi-Bundle Setup, Example 5 — Shared Baseline via `global_env.json`, Example 6 — Optional Site Packages, Example 7 — Layered Dev / Prod Environments, Examples

### Community 48 - "TestEnvironmentCheckOutput"
Cohesion: 0.22
Nodes (4): Passing stdout= to checkOutput raises ValueError., Passing both input= and stdin= to checkOutput raises ValueError., bytes passed via input= are forwarded to the process stdin., TestEnvironmentCheckOutput

### Community 49 - "_envoy"
Cohesion: 0.25
Nodes (5): _envoy(), Bound, PyModule, PyResult, Python

### Community 50 - "TestEnvironmentBuild"
Cohesion: 0.25
Nodes (5): Calling build() twice returns the same object (no re-parse)., CommandNotFoundError when the command does not exist., Tests for Environment.build()., build() returns a dict containing variables from the env file., TestEnvironmentBuild

### Community 51 - "Installation"
Cohesion: 0.29
Nodes (6): Developer Build, Full Bundle (Recommended), Installation, Python API, Unsigned Artifact Notice, Verify Checksums

### Community 52 - "envoy-cli/src/lib.rs"
Cohesion: 0.33
Nodes (4): String, Vec, run(), strings()

### Community 53 - "testing.py"
Cohesion: 0.33
Nodes (6): patchBundleRoots(), patchCommandsFile(), Path, envoy.testing -- Test helpers for code that calls the envoy Python API.…, Context manager that temporarily overrides ``ENVOY_BNDL_ROOTS``. All bundle…, Context manager that temporarily points envoy at a specific ``commands.json``…

### Community 54 - "TestEnvironmentProperties"
Cohesion: 0.29
Nodes (3): Tests for Environment properties and repr., whitelist is a deprecated alias that maps to allowlist., TestEnvironmentProperties

### Community 55 - "__init__.py"
Cohesion: 0.33
Nodes (4): async_new_environment(), envoy -- Environment orchestration for managed application execution. This…, Construct an :class:`Environment` without blocking the event loop.…, Main entry point for running envoy as a module. Usage: python -m envoy…

### Community 56 - "test_user_config.py"
Cohesion: 0.33
Nodes (5): Public Python contracts for Envoy's shared config root., Default user-config persistence uses the effective shared root., The root API resolves a non-empty override at call time., testGetConfigRootHonorsEnvironmentOverride(), testLoadUserConfigUsesEffectiveRoot()

### Community 57 - "git_describe"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 58 - "git_describe"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 59 - "gen_ref_pages.py"
Cohesion: 0.50
Nodes (3): Generate API reference pages for mkdocs, consumed by mkdocs-literate-nav. Runs…, Write a single reference page containing a mkdocstrings directive. Args:…, _writeModulePage()

### Community 60 - "conftest.py"
Cohesion: 0.50
Nodes (3): _clear_envoy_bndl_roots(), fixture, Shared fixtures for the ``envoy-py`` wheel Python contract tests. Autouse-…

## Knowledge Gaps
- **44 isolated node(s):** `Example 1 — Python Development Environment`, `Example 2 — One Command Across Platforms`, `Example 3 — Application-Specific Environment`, `Example 4 — Multi-Bundle Setup`, `Example 5 — Shared Baseline via `global_env.json`` (+39 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **24 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `&T` connect `bundle_cache.rs` to `envoy-py/src/wrapper.rs`, `proc.rs`, `tests.rs`, `team_config.rs`, `envoy-core/src/environment.rs`, `cli_main`, `envoy-core/src/commands.rs`, `envoy-py/src/commands.rs`, `runtime.rs`, `envoy-core/src/stack_registry.rs`, `envoy-py/src/stack_registry.rs`, `exceptions.rs`, `retry.rs`?**
  _High betweenness centrality (0.076) - this node is a cross-community bridge._
- **Why does `ProcessExecutor` connect `executor.rs` to `envoy-py/src/wrapper.rs`, `vcs.rs`, `envoy-core/src/wrapper.rs`?**
  _High betweenness centrality (0.056) - this node is a cross-community bridge._
- **What connects `Example 1 — Python Development Environment`, `Example 2 — One Command Across Platforms`, `Example 3 — Application-Specific Environment` to the rest of the system?**
  _44 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `envoy-py/src/wrapper.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06455379482902418 - nodes in this community are weakly interconnected._
- **Should `proc.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08337825696316262 - nodes in this community are weakly interconnected._
- **Should `tests.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.050597460791635546 - nodes in this community are weakly interconnected._
- **Should `team_config.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05640203154236835 - nodes in this community are weakly interconnected._