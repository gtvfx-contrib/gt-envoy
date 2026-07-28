# Graph Report - V:/repo/gtvfx-contrib/gt/envoy  (2026-07-28)

## Corpus Check
- 91 files · ~90,297 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2271 nodes · 6225 edges · 72 communities (65 shown, 7 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 169 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Bundle Discovery
- Python Wrapper Bindings
- Process Execution
- Engit Git Cleanup
- CLI Application
- Config & Crypto
- User Configuration
- Environment Management
- Engit CLI
- Core Wrapper Model
- Bundle Cache
- Stack Management
- Python Commands API
- Core Commands
- VCS Detection
- Command Executor
- Python API Bundle
- Telemetry
- Semver Library (Core)
- Engit Errors
- Stack Registry
- Changelog Generation
- Python Environment API
- Python Semver API
- Engit Semver
- Python BundleCache API
- Core Models
- CLI Integration Tests
- Python Core API
- Cross-Layer Diagnostics
- Retry Logic
- Editor Integration
- CLI Args Parsing
- Py Contract: Wrapper Tests
- Py Contract: Environment Tests
- Py Contract: Process Tests
- Python BundleInfo API
- Py Contract: Test Helpers
- Python CLI Entry
- Bundle Publishing Docs
- Python Stack API
- Consumer Smoke Tests
- Python UserConfig API
- Envoy Concepts Docs
- Project Infrastructure
- Py Contract: Discovery Tests
- Documentation & Standards
- Environment Files Docs
- Engit CLI Tests
- Trace Allowlist Events
- Python Library Module
- Py Contract: EnvironmentBuild
- Py Contract: EnvironmentCall
- CLI Library Entry
- Python Testing Utilities
- Py Contract: Default Operator
- Py Contract: EnvironmentSpawn
- Python Envoy Package
- Engit Build Config
- Envoy CLI Build Config
- Envoy Py Build Config
- Docs Reference Generation
- Py Contract: Conftest
- Py Contract: CheckCall Tests
- Advanced Docs: Chaining + Trace
- Advanced Docs: Conflicts
- Engit Release Docs
- Package Config: envoy
- Envoy Py Package Config

## God Nodes (most connected - your core abstractions)
1. `Stack` - 35 edges
2. `_pythonCommandsFile()` - 35 edges
3. `WrapperConfig` - 33 edges
4. `run_git()` - 28 edges
5. `BundleInfo` - 25 edges
6. `PyPopen` - 25 edges
7. `&T` - 24 edges
8. `SemVer` - 24 edges
9. `Environment` - 24 edges
10. `load_registry()` - 23 edges

## Surprising Connections (you probably didn't know these)
- `envoy-core Crate (Framework-Agnostic Logic)` --semantically_similar_to--> `Bundle Discovery (Auto-Discovery Flow)`  [INFERRED] [semantically similar]
  rust/README.md → docs/bundle-discovery.md
- `Deploy Docs Workflow (GitHub Pages + rustdoc)` --conceptually_related_to--> `envoy-py Crate (PyO3 Python Extension)`  [EXTRACTED]
  .github/workflows/deploy-docs.yml → rust/README.md
- `resolve_stack_value()` --calls--> `is_stack_name()`  [INFERRED]
  rust/envoy-core/src/stack.rs → rust/envoy-core/src/stack_registry.rs
- `Envoy CLI (Environment Orchestration)` --references--> `Bundle (Envoy Distribution Unit)`  [EXTRACTED]
  README.md → docs/concepts.md
- `Git-Derived Versioning (build.rs, git describe)` --conceptually_related_to--> `engit tag (Semantic Version Tagging)`  [INFERRED]
  rust/README.md → docs/cli-reference/engit.md

## Import Cycles
- 1-file cycle: `rust/envoy-core/src/executor.rs -> rust/envoy-core/src/executor.rs`

## Hyperedges (group relationships)
- **Rust Workspace Crate Architecture** — rust_readme_envoy_core, rust_readme_envoy_py, rust_readme_envoy_cli, rust_readme_engit_core, rust_readme_engit_cli [EXTRACTED 1.00]
- **CI/CD Workflow Pipeline (Build, Lint, Deploy, Publish)** — github_workflows_build_release_yml_build_release, github_workflows_lint_yml_lint, github_workflows_deploy_docs_yml_deploy_docs, github_workflows_bundle_publish_yml_bundle_publish [EXTRACTED 1.00]
- **Envoy Testing Strategy (Unit + Contract + Consumer Smoke)** — github_workflows_lint_yml_lint, rust_envoy_py_tests_python_contract_readme_python_contract, rust_envoy_py_tests_consumer_smoke_readme_consumer_smoke [EXTRACTED 1.00]

## Communities (72 total, 7 thin omitted)

### Community 0 - "Bundle Discovery"
Cohesion: 0.06
Nodes (97): bndlid_regex(), Bundle, bundle_info_display_and_debug_match_python_style(), bundle_path_var_regex(), bundle_reads_marker_version_and_production_state(), bundle_supports_path_specs_bndlid_specs_and_namespace_overrides(), BundleInfo, cache_entry_created_at() (+89 more)

### Community 1 - "Python Wrapper Bindings"
Cohesion: 0.06
Nodes (59): Clone, CoreExecutionResult, CoreWrapperConfig, PyRef, PyTuple, ApplicationWrapper, build_spawn_command(), call_python_noarg() (+51 more)

### Community 2 - "Process Execution"
Cohesion: 0.08
Nodes (78): MutexGuard, PyBytes, is_raw_path(), called_process_error(), String, apply_default_creationflags(), build_cached_environment(), build_returns_same_python_dict_object_on_repeated_calls() (+70 more)

### Community 3 - "Engit Git Cleanup"
Cohesion: 0.05
Nodes (94): Option, Path, Result, run_cleanup(), args_to_strings(), create_tag(), delete_local_branch(), detects_git_repositories_and_root() (+86 more)

### Community 4 - "CLI Application"
Cohesion: 0.06
Nodes (92): BundleCache, Cli, debug(), display_envoy_error(), ExecutionOptions, find_local_docs(), handle_get_config(), handle_list_configs() (+84 more)

### Community 5 - "Config & Crypto"
Cohesion: 0.05
Nodes (73): DecodeError, DecryptError, EncryptError, FromUtf8Error, Identity, Recipient, ConfigCryptoError, configured_key_file_path() (+65 more)

### Community 6 - "User Configuration"
Cohesion: 0.05
Nodes (62): CoreNamedStackEntry, default_config_path(), default_config_path_ends_with_expected_filename(), escape_repr_string(), format_settings(), known_setting(), known_settings(), KnownSetting (+54 more)

### Community 7 - "Environment Management"
Cohesion: 0.09
Nodes (64): D, Number, absolute_lexical_path(), as_array(), core_env_vars(), current_process_env(), diagnose_environment_accepts_comment_annotated_json(), env_items_from_value() (+56 more)

### Community 8 - "Engit CLI"
Cohesion: 0.07
Nodes (68): ExitCode, ChangelogArgs, CleanupArgs, Cli, Commands, current_dir_path(), default_stack_root_from_env(), main() (+60 more)

### Community 9 - "Core Wrapper Model"
Cohesion: 0.08
Nodes (55): AtomicBool, ExecutionResult, ActiveRun, ActiveRunGuard, ApplicationWrapper, build_spawn_command(), callbacks_fire_for_start_stdout_and_stderr(), command_executable() (+47 more)

### Community 10 - "Bundle Cache"
Cohesion: 0.10
Nodes (49): BundleCache, BundleCacheError, BundleMeta, cache_deduplicates_identical_content(), cache_list_shows_all_bundles(), cache_removes_bundles(), cache_returns_not_found_for_missing(), cache_stores_and_retrieves_bundles() (+41 more)

### Community 11 - "Stack Management"
Cohesion: 0.09
Nodes (46): Bundle, NamedStackEntry, create_bundle(), current_stack_honors_environment_user_and_context_precedence(), default_namespace(), EnvVarGuard, expand_home_path(), load_registry_stacks() (+38 more)

### Community 12 - "Python Commands API"
Cohesion: 0.08
Nodes (40): CoreBundleLike, CoreCommandDefinition, CoreCommandRegistry, command_definition_expand_alias_uses_special_vars_and_env_values(), command_registry_round_trips_python_visible_objects(), CommandDefinition, CommandRegistry, create_test_dir() (+32 more)

### Community 13 - "Core Commands"
Cohesion: 0.07
Nodes (42): absolute_lexical_path(), Bundle, BundleInfo, BundleLike, CommandDefinition, CommandRegistry, EnvVarGuard, expand_alias_supports_bundle_special_vars_and_env_values() (+34 more)

### Community 14 - "VCS Detection"
Cohesion: 0.10
Nodes (48): detect(), detect_or_error(), detect_vcs_finds_git_root_from_nested_dir(), detect_vcs_honors_override_before_auto_detection(), find_git_root(), find_lore_root(), find_parent_with(), format_command() (+40 more)

### Community 15 - "Command Executor"
Cohesion: 0.09
Nodes (48): drain_stream(), find_in_path(), has_directory_component(), invoke_callback(), is_batch_script(), is_executable_candidate(), long_running_command(), make_absolute() (+40 more)

### Community 16 - "Python API Bundle"
Cohesion: 0.06
Nodes (11): CoreBundle, CoreTeamConfig, CoreTraceStepEvent, CoreUserHostConfig, CoreVcsChange, Bundle, String, TeamConfig (+3 more)

### Community 17 - "Telemetry"
Cohesion: 0.07
Nodes (43): From, disable(), disable_and_clear_flag(), disable_reverts_to_discarding_events(), enable(), null_sink_is_the_default_and_discards_events(), NullSink, opentelemetry::Value (+35 more)

### Community 18 - "Semver Library (Core)"
Cohesion: 0.09
Nodes (33): Err, compare_prerelease(), Constraint, constraint_caret_matches(), constraint_eq_exact(), constraint_gte_matches(), constraint_tilde_matches(), display_round_trips() (+25 more)

### Community 19 - "Engit Errors"
Cohesion: 0.08
Nodes (42): CoreEnvoyError, command_failure_falls_back_to_stdout(), command_failure_prefers_stderr(), command_failure_uses_generic_message_when_empty(), EngitError, format_command_failure(), Error, Into (+34 more)

### Community 20 - "Stack Registry"
Cohesion: 0.11
Nodes (41): civil_from_days(), current_timestamp(), EnvVarGuard, format_system_time(), is_stack_name(), join_roots(), lexical_normalize(), list_named_stacks() (+33 more)

### Community 21 - "Changelog Generation"
Cohesion: 0.11
Nodes (36): Deserialize, fetch_release_detail(), ReleaseDetail, Option, Path, Result, String, Vec (+28 more)

### Community 22 - "Python Environment API"
Cohesion: 0.17
Nodes (19): build_environment(), CachedEnv, Environment, path_like_to_pathbuf(), register_environment_module(), Bound, HashMap, Mutex (+11 more)

### Community 23 - "Python Semver API"
Cohesion: 0.08
Nodes (7): CoreConstraint, CoreSemVer, CoreVersionSpec, Constraint, Self, SemVer, VersionSpec

### Community 24 - "Engit Semver"
Cohesion: 0.12
Nodes (20): bump_helpers_reset_lower_parts(), compare_prerelease(), display_and_to_tag_match_python_behavior(), ordering_places_stable_after_matching_prerelease(), parse_accepts_stable_and_prerelease_tags(), parse_rejects_invalid_versions(), Display, Formatter (+12 more)

### Community 25 - "Python BundleCache API"
Cohesion: 0.22
Nodes (8): CoreBundleCache, BundleCache, json_map_to_pyobject(), path_to_py_path(), PyObject, PyResult, Python, Value

### Community 26 - "Core Models"
Cohesion: 0.10
Nodes (24): PostRunCallback, PreRunCallback, execution_result_display_matches_failed_repr(), execution_result_display_matches_success_repr(), execution_result_success_is_false_for_non_zero_exit(), execution_result_success_is_false_for_timeout_even_with_zero_exit(), execution_result_success_is_true_for_zero_exit_without_timeout(), ExecutionResult (+16 more)

### Community 27 - "CLI Integration Tests"
Cohesion: 0.19
Nodes (24): base_command(), diagnose_with_command_shows_resolved_environment(), diagnose_with_unknown_command_fails_with_clear_error(), diagnose_without_command_summarizes_stack_bundles_and_team(), help_lists_expected_flags(), legacy_bundles_config_flag_is_rejected(), list_configs_runs_without_error(), raw_absolute_path_executable_runs_successfully() (+16 more)

### Community 28 - "Python Core API"
Cohesion: 0.11
Nodes (15): CoreVcsAdapter, CoreVcsKind, CoreVcsStatus, allowlist_contains_envoy_roots_and_extra_values(), build_allowlist(), current_operating_system(), get_allowlist(), map_operating_system_name() (+7 more)

### Community 29 - "Cross-Layer Diagnostics"
Cohesion: 0.27
Nodes (19): CoreTraceEvent, open_default_bundle_cache(), allowlist_to_hashset(), diagnose_environment(), get_current_team_config(), get_environment(), load_user_config(), path_like_to_pathbuf() (+11 more)

### Community 30 - "Retry Logic"
Cohesion: 0.17
Nodes (14): E, F, is_transient_error(), retry_config_defaults(), retry_config_no_retry(), retry_sync(), retry_sync_gives_up_after_max_attempts(), retry_sync_retries_on_transient_failure() (+6 more)

### Community 31 - "Editor Integration"
Cohesion: 0.18
Nodes (16): EnvVarGuard, find_editor(), open_in_editor(), remove_if_empty(), Drop, Option, Path, PathBuf (+8 more)

### Community 32 - "CLI Args Parsing"
Cohesion: 0.21
Nodes (20): canonicalize_legacy_aliases(), canonicalize_legacy_aliases_translates_multi_char_aliases(), canonicalize_legacy_aliases_understands_option_values_before_command(), Cli, legacy_alias(), normalize_argv(), normalize_argv_does_not_expand_child_process_args(), normalize_argv_expands_short_and_long_equals_forms() (+12 more)

### Community 33 - "Py Contract: Wrapper Tests"
Cohesion: 0.14
Nodes (18): Public-API contract tests for ``ApplicationWrapper``, run against the compiled…, Test timeout functionality., Test event callbacks., Test createWrapper convenience function., Test working directory., Test basic command execution., Test environment variable passing., Test pre and post run operations. (+10 more)

### Community 34 - "Py Contract: Environment Tests"
Cohesion: 0.16
Nodes (9): Environment, Tests for Environment properties and repr., whitelist is a deprecated alias that maps to allowlist., Tests for Environment.checkOutput()., Passing stdout= to checkOutput raises ValueError., Passing both input= and stdin= to checkOutput raises ValueError., bytes passed via input= are forwarded to the process stdin., TestEnvironmentCheckOutput (+1 more)

### Community 35 - "Py Contract: Process Tests"
Cohesion: 0.17
Nodes (5): _pythonCommandsFile(), Tests for the module-level call / spawn / checkCall / checkOutput. Free…, Envoy CLI flags embedded in cmd (e.g. -cf path) are forwarded., Return a commands.json that defines a 'py' command using ``python``., TestProcFreeFunctions

### Community 36 - "Python BundleInfo API"
Cohesion: 0.22
Nodes (8): CoreBundleInfo, bundle_infos_to_py(), BundleInfo, discover_bundles_auto(), get_bundles(), load_bundles_from_stack(), Py, Vec

### Community 37 - "Py Contract: Test Helpers"
Cohesion: 0.15
Nodes (12): _makeBundle(), _makeCommandsDir(), Path, Public-API contract tests for ``envoy.proc``, run against the compiled ``envoy-…, Create a minimal bundle directory tree. Produces:: <tmp_dir>/gt/<name>/ .git/…, End-to-end tests exercising bundle discovery + environment building., Environment variables from a bundle env file reach the subprocess., A command that references another command gets both env files applied. (+4 more)

### Community 38 - "Python CLI Entry"
Cohesion: 0.23
Nodes (15): cli_main(), cli_main_defaults_to_sys_argv_when_none(), cli_main_returns_success_for_help_flag(), cli_main_returns_success_for_version_flag(), register_cli_bindings(), register_cli_bindings_adds_cli_main(), Bound, FnOnce (+7 more)

### Community 39 - "Bundle Publishing Docs"
Cohesion: 0.18
Nodes (13): Bundle Publishing Workflow (engit publish), GitHub Actions Reusable Bundle Publish Workflow, Bundle Discovery (Auto-Discovery Flow), .bundle Marker File (Version Metadata), Discovery Cache (On-Disk, Short-Lived), ENVOY_BNDL_ROOTS (Bundle Discovery Root), Runtime Stack (.estack YAML), engit publish (Clean Bundle Publishing) (+5 more)

### Community 40 - "Python Stack API"
Cohesion: 0.24
Nodes (4): CoreStack, get_current_stack(), PyType, Stack

### Community 41 - "Consumer Smoke Tests"
Cohesion: 0.17
Nodes (11): _clear_envoy_bndl_roots(), fixture, Consumer smoke tests for ``envoy-py``, exercising the real API call patterns…, ``gt/unreal/wrapper/py/gt/unreal/wrapper/__main__.py``'s real pattern:…, ``gt/globals/py/gt/vscode/wrapper/_wrapper.py``'s real code path:…, ``gt/devtools/py/cleanup_branches.py`` calls ``envoy.proc.spawn(cmd,…, ``gt/krita/wrapper/py/gt/krita/wrapper/__main__.py``'s real pattern:…, test_devtools_cleanup_branches_spawn_kwargs_are_a_pre_existing_bug() (+3 more)

### Community 42 - "Python UserConfig API"
Cohesion: 0.20
Nodes (3): CoreUserConfig, RefCell, UserConfig

### Community 43 - "Envoy Concepts Docs"
Cohesion: 0.25
Nodes (11): envoy --diagnose (Full Diagnostic Report), Bundle ID (bndlid), Bundle (Envoy Distribution Unit), Bundle Cache (Content-Addressed Production Cache), Command Definitions (commands.json), Environment Files (JSON with Operators), global_env.json (Bundle-Wide Baseline), team.json (Team-Scoped Configuration) (+3 more)

### Community 44 - "Project Infrastructure"
Cohesion: 0.22
Nodes (11): Envoy Documentation Home (index.md), Cross-Platform Build Verification (Linux/macOS), Build & Release Workflow (build-release.yml), Deploy Docs Workflow (GitHub Pages + rustdoc), ProperDocs (MkDocs Material Documentation Builder), engit-cli Crate (Native Binary), engit-core Crate (Git/GitHub Logic), envoy-cli Crate (Native Binary) (+3 more)

### Community 45 - "Py Contract: Discovery Tests"
Cohesion: 0.22
Nodes (10): _makeBundle(), Path, Public-API contract tests for bundle discovery, run against the compiled…, Legacy domain names are absent from the clean-break Python API., Create a minimal bundle directory tree (git repo + .envoy marker)., Stack and loadBundlesFromStack() resolve bundle paths from YAML., getBundles() auto-discovers bundles under ENVOY_BNDL_ROOTS., test_auto_discovery() (+2 more)

### Community 46 - "Documentation & Standards"
Cohesion: 0.29
Nodes (10): engit tag (Semantic Version Tagging), Copilot Coding Standards (PEP 8 + CamelCase Functions), Google Style Python Docstrings Standard, Lint Workflow (Ruff + Rust fmt/clippy/test), Consumer Smoke Tests (Real Consumer API Verification), PyPopen (Duck-Typed Popen Replacement), Python Contract Tests (Public API Parity), envoy-py Crate (PyO3 Python Extension) (+2 more)

### Community 47 - "Environment Files Docs"
Cohesion: 0.20
Nodes (10): envoy --env / -e (Run in Another Command's Environment), Env File Operators (=, +=, ^=, ?=), environment_allowlist (Per-Command Allowlist), Environment File Operators Reference, Optional Variable References (${?VAR}), Special Variables (__BUNDLE__, __FILE__, etc.), Closed Environment Mode (Default), ENVOY_ALLOWLIST (System Variable Passthrough) (+2 more)

### Community 48 - "Engit CLI Tests"
Cohesion: 0.39
Nodes (8): help_lists_all_subcommands(), missing_required_argument_returns_usage_error(), publish_stack_without_stack_root_or_env_var_fails_with_expected_message(), Assert, String, stderr_text(), stdout_text(), tag_requires_exactly_one_bump_or_version_input()

### Community 50 - "Python Library Module"
Cohesion: 0.25
Nodes (5): _envoy(), Bound, PyModule, PyResult, Python

### Community 51 - "Py Contract: EnvironmentBuild"
Cohesion: 0.25
Nodes (5): Calling build() twice returns the same object (no re-parse)., CommandNotFoundError when the command does not exist., Tests for Environment.build()., build() returns a dict containing variables from the env file., TestEnvironmentBuild

### Community 52 - "Py Contract: EnvironmentCall"
Cohesion: 0.25
Nodes (4): Tests for Environment.call()., call() raises ValueError when stdout=PIPE is requested., call() raises ValueError when stderr=PIPE is requested., TestEnvironmentCall

### Community 53 - "CLI Library Entry"
Cohesion: 0.33
Nodes (4): String, Vec, run(), strings()

### Community 54 - "Python Testing Utilities"
Cohesion: 0.33
Nodes (6): patchBundleRoots(), patchCommandsFile(), Path, envoy.testing -- Test helpers for code that calls the envoy Python API.…, Context manager that temporarily overrides ``ENVOY_BNDL_ROOTS``. All bundle…, Context manager that temporarily points envoy at a specific ``commands.json``…

### Community 55 - "Py Contract: Default Operator"
Cohesion: 0.33
Nodes (6): Path, Public-API contract test for the ``?=`` (default) operator, run against the…, Write a JSON env file to a temp directory and return its path., End-to-end: ?= sets variable via ApplicationWrapper when not in env., test_default_operator_via_wrapper(), _writeEnvFile()

### Community 56 - "Py Contract: EnvironmentSpawn"
Cohesion: 0.29
Nodes (4): Tests for Environment.spawn()., spawn() returns before the process exits., Variables from the env file are visible inside the spawned process., TestEnvironmentSpawn

### Community 57 - "Python Envoy Package"
Cohesion: 0.33
Nodes (4): async_new_environment(), envoy -- Environment orchestration for managed application execution. This…, Construct an :class:`Environment` without blocking the event loop.…, Main entry point for running envoy as a module. Usage: python -m envoy…

### Community 58 - "Engit Build Config"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 59 - "Envoy CLI Build Config"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 60 - "Envoy Py Build Config"
Cohesion: 0.50
Nodes (4): git_describe(), main(), Option, String

### Community 61 - "Docs Reference Generation"
Cohesion: 0.50
Nodes (3): Generate API reference pages for mkdocs, consumed by mkdocs-literate-nav. Runs…, Write a single reference page containing a mkdocstrings directive. Args:…, _writeModulePage()

### Community 62 - "Py Contract: Conftest"
Cohesion: 0.50
Nodes (3): _clear_envoy_bndl_roots(), fixture, Shared fixtures for the ``envoy-py`` wheel Python contract tests. Autouse-…

## Knowledge Gaps
- **22 isolated node(s):** `envoy`, `envoy`, `engit CLI (Developer Toolchain)`, `Bundle ID (bndlid)`, `global_env.json (Bundle-Wide Baseline)` (+17 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **7 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Commands` connect `Engit CLI` to `CLI Application`, `Python Commands API`?**
  _High betweenness centrality (0.106) - this node is a cross-community bridge._
- **Why does `&T` connect `Core Commands` to `Bundle Discovery`, `Python Wrapper Bindings`, `Process Execution`, `CLI Application`, `Config & Crypto`, `Python CLI Entry`, `Environment Management`, `User Configuration`, `Bundle Cache`, `Python Commands API`, `Engit Errors`, `Stack Registry`, `Retry Logic`?**
  _High betweenness centrality (0.104) - this node is a cross-community bridge._
- **Why does `ProcessExecutor` connect `Command Executor` to `Python Wrapper Bindings`, `Process Execution`, `CLI Application`, `Core Wrapper Model`, `VCS Detection`?**
  _High betweenness centrality (0.092) - this node is a cross-community bridge._
- **What connects `envoy`, `envoy`, `engit CLI (Developer Toolchain)` to the rest of the system?**
  _22 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Bundle Discovery` be split into smaller, more focused modules?**
  _Cohesion score 0.06101928374655648 - nodes in this community are weakly interconnected._
- **Should `Python Wrapper Bindings` be split into smaller, more focused modules?**
  _Cohesion score 0.06455379482902418 - nodes in this community are weakly interconnected._
- **Should `Process Execution` be split into smaller, more focused modules?**
  _Cohesion score 0.0811699550017307 - nodes in this community are weakly interconnected._