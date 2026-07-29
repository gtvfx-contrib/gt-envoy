use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use envoy_core::bundle_cache::{open_default_bundle_cache, resolve_bundle_cache_dir};
use envoy_core::commands::{find_commands_file, CommandDefinition, CommandRegistry};
use envoy_core::discovery::{get_bundles, BundleInfo};
use envoy_core::environment::{EnvironmentManager, TraceEvent};
use envoy_core::error::EnvoyError;
use envoy_core::executor::ProcessExecutor;
use envoy_core::models::WrapperConfig;
use envoy_core::runtime::{
    collect_env_files, is_raw_path, prepare_env, resolve_cached_bundles,
    resolve_team_config_for_bundles,
};
use envoy_core::stack::{Stack, DEFAULT_STACK_MAX_DEPTH, DEFAULT_STACK_NAMESPACE};
use envoy_core::stack_registry::{
    is_stack_name, list_named_stacks, resolve_named_stack, STACK_ROOTS_VAR,
};
use envoy_core::user_config::{known_settings, UserConfig};
use envoy_core::wrapper::ApplicationWrapper;

use crate::args::{self, Cli};

const DOCS_URL: &str = "https://gtvfx-contrib.github.io/gt-envoy/";

struct ExecutionOptions<'a> {
    bundles: Option<&'a [BundleInfo]>,
    verbose: bool,
    inherit_env: bool,
    env_allowlist: Option<&'a [String]>,
    env_override: Option<&'a str>,
}

struct LoadedRegistry {
    registry: CommandRegistry,
    bundles: Option<Vec<BundleInfo>>,
    stack: Option<Stack>,
}

pub(crate) fn run(argv: &[String]) -> i32 {
    init_tracing();

    let cli = match args::parse(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return error.exit_code();
        }
    };

    run_cli(cli)
}

/// Install a default `tracing` subscriber writing to stderr, honoring
/// `RUST_LOG` (falling back to `warn`) for the internal diagnostic logging
/// envoy-core emits via the `tracing` crate (for example malformed team.json warnings).
///
/// Uses `try_init` rather than `init` since `run()` can be invoked more than
/// once per process -- e.g. `envoy.cli_main(...)` from a long-lived Python
/// process -- and installing a global subscriber twice panics. A failed
/// second attempt is expected and silently ignored.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn run_cli(cli: Cli) -> i32 {
    if cli.list_configs {
        return handle_list_configs();
    }

    if let Some(raw) = cli.set_config.as_deref() {
        return handle_set_config(raw);
    }

    if cli.get_config.is_some() {
        return handle_get_config(cli.get_config.as_deref());
    }

    if cli.docs {
        return open_docs();
    }

    let user_cfg = UserConfig::load(None);
    let verbose =
        cli.verbose || (!cli.ignore_config && matches!(user_cfg.get("verbosity"), Some("verbose")));

    let LoadedRegistry {
        registry,
        bundles,
        stack,
    } = match load_registry_for_cli(&cli, verbose) {
        Ok(result) => result,
        Err(code) => return code,
    };

    if registry.is_empty()
        && !raw_path_without_env_override(cli.command.as_deref(), cli.env.as_deref())
    {
        eprintln!("Error: No commands loaded");
        return 1;
    }

    if cli.list {
        return list_commands(&registry);
    }

    if let Some(command_name) = cli.info.as_deref() {
        return show_command_info(&registry, command_name);
    }

    let env_allowlist = parse_allowlist_env();
    if let Some(values) = env_allowlist.as_ref().filter(|_| verbose) {
        let mut sorted = values.clone();
        sorted.sort();
        debug(verbose, &format!("Allowlist: {sorted:?}"));
    }

    if let Some(command_name) = cli.diagnose.as_deref() {
        return run_diagnose(
            &registry,
            bundles.as_deref(),
            stack.as_ref(),
            Some(command_name).filter(|name| !name.is_empty()),
            cli.inherit_env,
            env_allowlist.as_deref(),
        );
    }

    if let Some(command_name) = cli.which.as_deref() {
        return show_which(
            &registry,
            command_name,
            bundles.as_deref(),
            cli.inherit_env,
            env_allowlist.as_deref(),
        );
    }

    if let Some(trace_var) = cli.trace.as_deref() {
        let Some(command_name) = cli.command.as_deref() else {
            eprintln!("Error: --trace requires a COMMAND argument");
            eprintln!("Example: envoy --trace UE_PYTHONPATH unreal");
            return 1;
        };

        return trace_command(
            &registry,
            command_name,
            trace_var,
            bundles.as_deref(),
            cli.inherit_env,
            env_allowlist.as_deref(),
            cli.env.as_deref(),
        );
    }

    let Some(command_name) = cli.command.as_deref() else {
        if let Err(error) = args::print_help() {
            eprintln!("Error: {error}");
            return 1;
        }
        return 0;
    };

    let start = std::time::Instant::now();
    let exit_code = run_command(
        &registry,
        command_name,
        &cli.args,
        ExecutionOptions {
            bundles: bundles.as_deref(),
            verbose,
            inherit_env: cli.inherit_env,
            env_allowlist: env_allowlist.as_deref(),
            env_override: cli.env.as_deref(),
        },
    );

    // Best-effort usage tracking: a no-op unless a caller has opted in via
    // `envoy.enable_telemetry(...)`. Recording this unconditionally (rather
    // than only when telemetry happens to already be enabled) means any
    // future in-process caller that enables telemetry before invoking the
    // CLI dispatch (e.g. via `envoy.cli_main`) gets `command_run` events
    // without envoy-cli needing to know that decision was made.
    let mut attributes = std::collections::HashMap::new();
    attributes.insert(
        "command".to_string(),
        envoy_core::telemetry::TelemetryValue::Str(command_name.to_string()),
    );
    attributes.insert(
        "duration_ms".to_string(),
        envoy_core::telemetry::TelemetryValue::Int(start.elapsed().as_millis() as i64),
    );
    attributes.insert(
        "success".to_string(),
        envoy_core::telemetry::TelemetryValue::Bool(exit_code == 0),
    );
    envoy_core::telemetry::track("command_run", attributes);

    exit_code
}

fn load_registry_for_cli(cli: &Cli, verbose: bool) -> Result<LoadedRegistry, i32> {
    let mut registry = CommandRegistry::empty();
    let mut bundles = None;
    let selected_stack = if let Some(raw_value) = cli.stack.as_deref() {
        Some(resolve_stack_value(raw_value, verbose)?)
    } else {
        match Stack::current(
            cli.ignore_config,
            None,
            DEFAULT_STACK_NAMESPACE,
            DEFAULT_STACK_MAX_DEPTH,
        ) {
            Ok(stack) => stack,
            Err(error) => {
                eprintln!("Error resolving stack: {}", display_envoy_error(&error));
                return Err(1);
            }
        }
    };

    // Resolve the local bundle cache once up front so both stack-backed and
    // root-based discovery apply it consistently. `--ignore-config` skips the
    // user-config-sourced cache directory while still allowing
    // the `ENVOY_BUNDLE_CACHE` / `ENVOY_DISABLE_BUNDLE_CACHE` env vars.
    let mut bundle_cache = open_default_bundle_cache(!cli.ignore_config);

    if let Some(stack) = selected_stack.as_ref() {
        match stack.bundle_infos() {
            Ok(discovered_bundles) => {
                if discovered_bundles.is_empty() {
                    debug(verbose, "No bundles found in stack");
                } else {
                    let team_config = resolve_team_config_for_bundles(Some(&discovered_bundles));
                    let discovered_bundles = resolve_cached_bundles(
                        discovered_bundles,
                        bundle_cache.as_mut(),
                        team_config.as_ref(),
                    );
                    debug(
                        verbose,
                        &format!(
                            "Discovered {} bundle(s) from stack {}",
                            discovered_bundles.len(),
                            stack.name()
                        ),
                    );
                    registry.load_from_bundles(&discovered_bundles);
                    bundles = Some(discovered_bundles);
                }
            }
            Err(error) => {
                eprintln!(
                    "Error loading stack bundles: {}",
                    display_envoy_error(&error)
                );
                return Err(1);
            }
        }
    } else if let Some(commands_file) = cli.commands_file.as_deref() {
        let commands_path = PathBuf::from(commands_file);
        if !commands_path.exists() {
            eprintln!(
                "Error: Commands file not found: {}",
                commands_path.display()
            );
            return Err(1);
        }

        if let Err(error) = registry.load_from_file(&commands_path, None) {
            eprintln!("Error loading commands: {}", display_envoy_error(&error));
            return Err(1);
        }
    } else {
        match get_bundles(None) {
            Ok(discovered_bundles) if !discovered_bundles.is_empty() => {
                let team_config = resolve_team_config_for_bundles(Some(&discovered_bundles));
                let discovered_bundles = resolve_cached_bundles(
                    discovered_bundles,
                    bundle_cache.as_mut(),
                    team_config.as_ref(),
                );
                debug(
                    verbose,
                    &format!("Auto-discovered {} bundle(s)", discovered_bundles.len()),
                );
                registry.load_from_bundles(&discovered_bundles);
                bundles = Some(discovered_bundles);
            }
            Ok(_) => {}
            Err(error) => {
                debug(
                    verbose,
                    &format!(
                        "Bundle auto-discovery failed: {}",
                        display_envoy_error(&error)
                    ),
                );
            }
        }

        if registry.is_empty() {
            match find_commands_file(None) {
                Ok(Some(commands_file)) => {
                    if let Err(error) = registry.load_from_file(&commands_file, None) {
                        eprintln!("Error loading commands: {}", display_envoy_error(&error));
                        return Err(1);
                    }
                }
                Ok(None) => {
                    if !raw_path_without_env_override(cli.command.as_deref(), cli.env.as_deref()) {
                        eprintln!("Error: Could not find commands.json");
                        eprintln!(
                            "Searched for .envoy/commands.json in current directory and parents"
                        );
                        eprintln!(
                            "Or set ENVOY_BNDL_ROOTS environment variable for auto-discovery"
                        );
                        return Err(1);
                    }
                }
                Err(error) => {
                    eprintln!("Error loading commands: {}", display_envoy_error(&error));
                    return Err(1);
                }
            }
        }
    }

    if verbose {
        if let Some(team) = resolve_team_config_for_bundles(bundles.as_deref()) {
            debug(verbose, &format!("Resolved team config: {}", team.name));
        }
        if let Some(stack) = selected_stack.as_ref() {
            debug(verbose, &format!("Resolved stack: {}", stack.name()));
        }
    }

    Ok(LoadedRegistry {
        registry,
        bundles,
        stack: selected_stack,
    })
}

fn resolve_stack_value(raw: &str, verbose: bool) -> Result<Stack, i32> {
    if is_stack_name(raw) {
        let Some(resolved) = resolve_named_stack(raw) else {
            eprintln!("Error: Named stack {raw:?} not found in {STACK_ROOTS_VAR}.");
            return Err(1);
        };
        debug(
            verbose,
            &format!("Resolved named stack {raw:?} to: {}", resolved.display()),
        );
        return Stack::from_name(raw).map_err(|error| {
            eprintln!("Error loading named stack: {}", display_envoy_error(&error));
            1
        });
    }

    Stack::new(raw).map_err(|error| {
        eprintln!("Error loading stack: {}", display_envoy_error(&error));
        1
    })
}

fn raw_path_without_env_override(command: Option<&str>, env_override: Option<&str>) -> bool {
    command.is_some_and(is_raw_path) && env_override.is_none()
}

/// Implements `envoy --diagnose [COMMAND]`.
///
/// Surfaces the current stack, discovered bundles/commands, resolved team,
/// bundle cache status, VCS detection, telemetry status, and bundle-root
/// reachability -- everything the stretch-goals plan's Phase 5.3 "Diagnostic
/// Tools" design called for in one place, wrapping the underlying
/// `envoy_core` pieces (`resolve_team_config_for_bundles`,
/// `open_default_bundle_cache`, `EnvironmentManager::diagnose_environment`)
/// that are each individually reachable via other flags/the Python API but
/// were not previously surfaced together in one CLI report.
///
/// When `command_name` is given, also resolves and prints that command's
/// full environment (all variables, not just one), complementing the
/// existing single-variable `--trace VAR COMMAND` flag.
fn run_diagnose(
    registry: &CommandRegistry,
    bundles: Option<&[BundleInfo]>,
    stack: Option<&Stack>,
    command_name: Option<&str>,
    inherit_env: bool,
    env_allowlist: Option<&[String]>,
) -> i32 {
    let separator = "=".repeat(64);
    println!("{separator}");
    println!("envoy diagnose");
    println!("{separator}");
    println!();
    println!("Target: {}/{}", env::consts::OS, env::consts::ARCH);
    println!();

    match stack {
        Some(stack) => {
            println!(
                "Current stack: {} ({})",
                stack.name(),
                stack.path().display()
            );
            println!("  namespace: {}", stack.namespace());
            if let Some(version) = stack.registry_version() {
                println!("  registry_version: {version}");
            }
        }
        None => println!("Current stack: none (using bundle-root fallback)"),
    }
    println!();

    match bundles {
        Some(bundles) if !bundles.is_empty() => {
            println!("Bundles discovered: {}", bundles.len());
            for bundle in bundles {
                println!("  - {:<24} {}", bundle.bndlid(), bundle.root.display());
            }
        }
        _ => println!("Bundles discovered: 0 (legacy single-.envoy-directory mode, or none found)"),
    }
    println!();

    let mut commands = registry.list_commands();
    commands.sort();
    println!("Commands registered: {}", commands.len());
    if !commands.is_empty() {
        println!("  {}", commands.join(", "));
    }
    println!();

    match resolve_team_config_for_bundles(bundles) {
        Some(team) => {
            println!("Team config: {}", team.name);
            if let Some(root) = team.prod_bundles_root.as_ref() {
                println!("  prod_bundles_root:  {}", root.display());
            }
            if let Some(root) = team.prod_stacks_root.as_ref() {
                println!("  prod_stacks_root: {}", root.display());
            }
        }
        None => println!("Team config: none discovered (.envoy/team.json not found)"),
    }
    println!();

    match resolve_bundle_cache_dir(true) {
        Some(dir) => {
            let reachable = open_default_bundle_cache(true).is_some();
            let note = if reachable {
                "reachable"
            } else {
                "configured but could not be opened"
            };
            println!("Bundle cache: {} ({note})", dir.display());
        }
        None => println!("Bundle cache: disabled (ENVOY_DISABLE_BUNDLE_CACHE is set)"),
    }
    println!();

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match envoy_core::vcs::detect(&cwd) {
        Some(adapter) => {
            println!(
                "VCS detected: {} at {}",
                adapter.kind().as_str(),
                adapter.root().display()
            );
            match adapter.status() {
                Ok(status) => println!("  {} pending change(s)", status.changes.len()),
                Err(error) => println!("  (status unavailable: {error})"),
            }
        }
        None => println!(
            "VCS detected: none (checked Git, Perforce, Lore from {})",
            cwd.display()
        ),
    }
    println!();

    println!(
        "Telemetry: {}",
        if envoy_core::telemetry::is_enabled() {
            "enabled"
        } else {
            "disabled (default; call envoy.enable_telemetry(...) to opt in)"
        }
    );
    println!();

    if let Some(bundles) = bundles.filter(|values| !values.is_empty()) {
        println!("Bundle root reachability:");
        for bundle in bundles {
            let root = &bundle.root;
            let is_network = root.to_string_lossy().starts_with("\\\\");
            let kind = if is_network { "network (UNC)" } else { "local" };
            let status = if root.exists() {
                "reachable"
            } else {
                "UNREACHABLE"
            };
            println!(
                "  - {:<24} [{kind:<13}] {status}: {}",
                bundle.bndlid(),
                root.display()
            );
        }
        println!();
    }

    let Some(command_name) = command_name else {
        println!(
            "(Pass a COMMAND, e.g. --diagnose {}, to also see its full \
resolved environment.)",
            commands.first().map(String::as_str).unwrap_or("mycommand")
        );
        return 0;
    };

    println!("{}", "-".repeat(64));
    println!("Environment resolution for '{command_name}':");
    println!();

    if registry.get(command_name).is_none() {
        eprintln!("Error: Command '{command_name}' not found");
        eprintln!("Run 'envoy --list' to see available commands");
        return 1;
    }

    if let Some(command) = registry.get(command_name) {
        println!("Command configuration: {}", command.platform_resolution());
        println!();
    }

    let env_files = match collect_env_files(command_name, registry, bundles) {
        Ok(env_files) => env_files,
        Err(error) => {
            eprintln!(
                "Error resolving environment for '{command_name}': {}",
                display_envoy_error(&error)
            );
            return 1;
        }
    };

    println!("Env files ({}):", env_files.len());
    for (index, env_file) in env_files.iter().enumerate() {
        println!("  [{}] {}", index + 1, env_file.display());
    }
    println!();

    let allowlist_set = env_allowlist.map(|values| values.iter().cloned().collect::<HashSet<_>>());
    let env_manager = EnvironmentManager::new(inherit_env, allowlist_set);
    match env_manager.diagnose_environment(&env_files, None) {
        Ok((final_env, _trace_events)) => {
            println!("Resolved variables: {}", final_env.len());
            let mut names: Vec<&String> = final_env.keys().collect();
            names.sort();
            for name in names {
                let value = final_env.get(name).map(String::as_str).unwrap_or_default();
                println!("  {name} = {value}");
            }
            println!();
            println!(
                "(Use --trace VAR {command_name} for a step-by-step resolution trace of a \
single variable.)"
            );
        }
        Err(error) => {
            eprintln!("Error: {}", display_envoy_error(&error));
            return 1;
        }
    }

    0
}

fn list_commands(registry: &CommandRegistry) -> i32 {
    let commands = registry.list_commands();

    if commands.is_empty() {
        println!("No commands defined.");
        return 1;
    }

    println!("Available commands:");
    println!();

    for command_name in commands {
        if let Some(command) = registry.get(&command_name) {
            let bundle_suffix = command
                .bundle
                .as_ref()
                .map(|bundle| format!(" [{bundle}]"))
                .unwrap_or_default();

            if let Some(alias) = command.alias.as_ref() {
                println!("  {command_name:<20} → {}{bundle_suffix}", alias.join(" "));
            } else {
                println!("  {command_name:<20} (executable on PATH){bundle_suffix}");
            }
        }
    }

    0
}

fn show_command_info(registry: &CommandRegistry, command_name: &str) -> i32 {
    let Some(command) = registry.get(command_name) else {
        eprintln!("Error: Command '{command_name}' not found");
        eprintln!("Run 'envoy --list' to see available commands");
        return 1;
    };

    println!("Command: {command_name}");
    println!("Target: {}/{}", env::consts::OS, env::consts::ARCH);
    println!("Configuration: {}", command.platform_resolution());

    if let Some(bundle) = command.bundle.as_deref() {
        println!("Bundle: {bundle}");
    }

    println!("Executable: {}", command.executable());

    if !command.base_args().is_empty() {
        println!("Base args: {}", command.base_args().join(" "));
    }

    match registry.resolve_environment(command_name) {
        Ok(resolved_env) => {
            println!("Environment files:");
            for (env_file_name, _) in resolved_env {
                println!("  - {env_file_name}");
            }
        }
        Err(error) => {
            eprintln!(
                "Error resolving environment for '{command_name}': {}",
                display_envoy_error(&error)
            );
            return 1;
        }
    }

    if let Some(env_dir) = command.envoy_env_dir.as_ref() {
        println!("Environment directory: {}", env_dir.display());
    }

    if let Some(alias) = command.alias.as_ref() {
        println!("Alias: {}", alias.join(" "));
    }

    0
}

fn show_which(
    registry: &CommandRegistry,
    command_name: &str,
    bundles: Option<&[BundleInfo]>,
    inherit_env: bool,
    env_allowlist: Option<&[String]>,
) -> i32 {
    let Some(command) = registry.get(command_name) else {
        eprintln!("Error: Command '{command_name}' not found");
        return 1;
    };

    let env_files = match collect_env_files(command_name, registry, bundles) {
        Ok(env_files) => env_files,
        Err(error) => {
            eprintln!(
                "Error resolving environment for '{command_name}': {}",
                display_envoy_error(&error)
            );
            return 1;
        }
    };

    let allowlist = env_allowlist.map(|values| values.iter().cloned().collect::<HashSet<_>>());
    let env_manager = EnvironmentManager::new(inherit_env, allowlist);
    let env = match env_manager.prepare_environment(&env_files, None, None, None) {
        Ok(env) => env,
        Err(error) => {
            eprintln!(
                "Warning: Could not build environment: {}",
                display_envoy_error(&error)
            );
            HashMap::new()
        }
    };

    if let Some(alias) = command.alias.as_ref() {
        println!("command {command_name} aliased to: {}", alias.join(" "));
    }

    let expanded = command.expand_alias(Some(&env));
    let executable = expanded
        .first()
        .cloned()
        .unwrap_or_else(|| command_name.to_string());

    match ProcessExecutor::resolve_executable(
        Path::new(&executable),
        env.get("PATH").map(String::as_str),
    ) {
        Ok(resolved) => {
            println!("command {command_name} resolved to: {}", resolved.display());
        }
        Err(_) => {
            println!("command {command_name} executable: {executable} (not found on PATH)");
        }
    }

    if let Some(source_file) = command.source_file.as_ref() {
        println!("  defined in: {}", source_file.display());
    }

    0
}

fn run_command(
    registry: &CommandRegistry,
    command_name: &str,
    args: &[String],
    options: ExecutionOptions<'_>,
) -> i32 {
    let is_raw = is_raw_path(command_name);

    if !is_raw && registry.get(command_name).is_none() {
        eprintln!("Error: Command '{command_name}' not found");
        eprintln!("Run 'envoy --list' to see available commands");
        return 1;
    }

    if let Some(env_override) = options.env_override {
        if registry.get(env_override).is_none() {
            eprintln!("Error: Environment override command '{env_override}' not found");
            eprintln!("Run 'envoy --list' to see available commands");
            return 1;
        }
    }

    let (env_map, command) = if is_raw && options.env_override.is_none() {
        debug(
            options.verbose,
            &format!("Raw executable '{command_name}' with no env override; inheriting system env"),
        );
        (
            env::vars().collect::<HashMap<String, String>>(),
            CommandDefinition {
                name: command_name.to_string(),
                environment: Vec::new(),
                alias: Some(vec![command_name.to_string()]),
                bundle: None,
                envoy_env_dir: None,
                source_file: None,
                platform_overrides: Vec::new(),
            },
        )
    } else {
        match prepare_env(
            command_name,
            registry,
            options.bundles,
            options.inherit_env,
            options.env_allowlist,
            options.env_override,
        ) {
            Ok(result) => result,
            Err(error) => {
                eprintln!("Error: {}", display_envoy_error(&error));
                return 1;
            }
        }
    };

    let (executable, full_args) = if is_raw {
        (command_name.to_string(), args.to_vec())
    } else {
        let expanded = command.expand_alias(Some(&env_map));
        let executable = expanded
            .first()
            .cloned()
            .unwrap_or_else(|| command_name.to_string());
        let mut full_args = expanded.into_iter().skip(1).collect::<Vec<_>>();
        full_args.extend(args.iter().cloned());
        (executable, full_args)
    };

    if let Err(error) = ProcessExecutor::resolve_executable(
        Path::new(&executable),
        env_map.get("PATH").map(String::as_str),
    ) {
        eprintln!("Error: {}", display_envoy_error(&error));
        return 1;
    }

    let mut config = WrapperConfig::new(executable);
    config.args = full_args;
    config.env = Some(env_map);
    config.inherit_env = false;
    config.capture_output = false;
    config.stream_output = false;
    config.log_execution = options.verbose;
    config.raise_on_error = false;

    let mut wrapper = ApplicationWrapper::new(config);
    match wrapper.run() {
        Ok(result) if result.return_code == -2 => 130,
        Ok(result) => result.return_code.try_into().unwrap_or(1),
        Err(error) => {
            eprintln!("Error: {}", display_envoy_error(&error));
            1
        }
    }
}

fn trace_command(
    registry: &CommandRegistry,
    command_name: &str,
    trace_var: &str,
    bundles: Option<&[BundleInfo]>,
    inherit_env: bool,
    env_allowlist: Option<&[String]>,
    env_override: Option<&str>,
) -> i32 {
    if registry.get(command_name).is_none() {
        eprintln!("Error: Command '{command_name}' not found");
        return 1;
    }

    if let Some(env_override) = env_override {
        if registry.get(env_override).is_none() {
            eprintln!("Error: Environment override command '{env_override}' not found");
            return 1;
        }
    }

    let env_source = env_override.unwrap_or(command_name);
    let env_files = match collect_env_files(env_source, registry, bundles) {
        Ok(env_files) => env_files,
        Err(error) => {
            eprintln!(
                "Error resolving environment for '{env_source}': {}",
                display_envoy_error(&error)
            );
            return 1;
        }
    };

    let allowlist = env_allowlist.map(|values| values.iter().cloned().collect::<HashSet<_>>());
    let env_manager = EnvironmentManager::new(inherit_env, allowlist);
    let mut trace_events = Vec::new();
    let final_env = match env_manager.prepare_environment(
        &env_files,
        None,
        Some(trace_var),
        Some(&mut trace_events),
    ) {
        Ok(env) => env,
        Err(error) => {
            eprintln!("Error: {}", display_envoy_error(&error));
            return 1;
        }
    };

    let final_value = final_env.get(trace_var);
    let separator = "─".repeat(64);
    let env_label = env_override
        .map(|value| format!(" (via --env {value})"))
        .unwrap_or_default();

    println!();
    println!("Tracing '{trace_var}' for command '{command_name}'{env_label}");
    println!();
    println!("Env files ({}):", env_files.len());
    for (index, env_file) in env_files.iter().enumerate() {
        println!("  [{}] {}", index + 1, env_file.display());
    }
    println!();

    println!("{separator}");
    println!("Pre-pass: allowlist seeding");
    let allowlist_events = trace_events
        .iter()
        .filter_map(|event| match event {
            TraceEvent::Allowlist(event) => Some(event),
            TraceEvent::Step(_) => None,
        })
        .collect::<Vec<_>>();
    if allowlist_events.is_empty() {
        println!("  {trace_var} not listed in any environment_allowlist");
    } else {
        for event in allowlist_events {
            let file_label = event
                .file_path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| event.file_path.display().to_string());
            if event.already_set {
                println!(
                    "  {file_label}  {trace_var} in environment_allowlist (already in base env, skipped)"
                );
            } else if event.seeded {
                println!("  {file_label}  {trace_var} in environment_allowlist");
                println!(
                    "    → seeded from os.environ: {}",
                    repr_string(&event.os_value)
                );
            } else {
                println!(
                    "  {file_label}  {trace_var} in environment_allowlist (not present in os.environ)"
                );
            }
        }
    }
    println!();

    println!("{separator}");
    println!("File processing:");
    for (index, env_file) in env_files.iter().enumerate() {
        println!();
        println!("  [{}] {}", index + 1, env_file.display());
        let step_events = trace_events
            .iter()
            .filter_map(|event| match event {
                TraceEvent::Step(event) if event.file_path == *env_file => Some(event),
                _ => None,
            })
            .collect::<Vec<_>>();

        if step_events.is_empty() {
            println!("       {trace_var} not mentioned");
            continue;
        }

        for event in step_events {
            let before = if event.value_before.is_empty() {
                String::from("<not set>")
            } else {
                repr_string(&event.value_before)
            };
            println!("       {}{trace_var}: {}", event.operator, event.raw_value);
            println!("         before:   {before}");
            if event.expanded_value != event.raw_value.trim_matches('"') {
                println!("         expanded: {}", repr_string(&event.expanded_value));
            }
            if !event.was_applied {
                println!(
                    "         → no-op ({} skipped, variable already set)",
                    event.operator
                );
            } else {
                println!("         after:    {}", repr_string(&event.value_after));
            }
        }
    }
    println!();

    println!("{separator}");
    if let Some(final_value) = final_value {
        println!("Result: {trace_var} = {}", repr_string(final_value));
    } else {
        println!("Result: {trace_var} is not set");
    }
    println!();

    0
}

fn handle_set_config(raw: &str) -> i32 {
    let Some((key_raw, value)) = raw.split_once('=') else {
        eprintln!("Error: --set-config requires KEY=VALUE format (use KEY= to clear a setting)");
        return 1;
    };
    let key = key_raw.trim();
    let mut config = UserConfig::load(None);

    if value.is_empty() {
        if config.unset(key) {
            if let Err(error) = config.save() {
                eprintln!("Error: {}", display_envoy_error(&error));
                return 1;
            }
            println!("Cleared: {key}");
        } else {
            println!("Nothing to clear: {key:?} was not set");
        }
        return 0;
    }

    if let Err(error) = config.set(key, value) {
        eprintln!("Error: {}", display_envoy_error(&error));
        return 1;
    }

    if let Err(error) = config.save() {
        eprintln!("Error: {}", display_envoy_error(&error));
        return 1;
    }

    println!("Saved: {key} = {}", repr_string(value));
    println!("Config: {}", config.path.display());
    0
}

fn handle_get_config(key: Option<&str>) -> i32 {
    let config = UserConfig::load(None);

    if let Some(key) = key.filter(|key| !key.is_empty()) {
        match config.get(key) {
            Some(value) => println!("{key} = {}", repr_string(value)),
            None => println!("{key}: <not set>"),
        }
        return 0;
    }

    let settings = config.items();
    if settings.is_empty() {
        println!("No config settings are set.");
        println!("Config: {}", config.path.display());
        return 0;
    }

    println!("Config: {}", config.path.display());
    println!();

    let mut keys = settings.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    for key in keys {
        if let Some(value) = settings.get(&key) {
            println!("  {key} = {}", repr_string(value));
        }
    }

    0
}

fn handle_list_configs() -> i32 {
    let config = UserConfig::load(None);

    println!("Configurable settings:");
    println!();

    let mut settings = known_settings().to_vec();
    settings.sort_by(|left, right| left.0.cmp(right.0));

    for (setting_key, meta) in settings {
        let status = match config.get(setting_key) {
            Some(current) => format!("(current: {})", repr_string(current)),
            None => String::from("(not set)"),
        };
        println!("  {setting_key}  {status}");
        println!("    {}", meta.description);
        if let Some(choices) = meta.choices {
            println!("     Choices: {}", choices.join(", "));
        }
        println!();
    }

    println!("Config file: {}", config.path.display());
    println!();
    println!("Usage:  envoy --set-config KEY=VALUE");
    println!("        envoy --set-config KEY=       (clear a setting)");

    let named_stacks = list_named_stacks();
    if !named_stacks.is_empty() {
        println!();
        println!("Available named stacks ({STACK_ROOTS_VAR}):");
        println!();
        for entry in named_stacks {
            println!("  {:<20}  version: {}", entry.name, entry.version);
            println!("    {}", entry.path.display());
        }
        println!();
        println!("Usage:  envoy --set-config stack=<name>");
    } else if env::var_os(STACK_ROOTS_VAR).is_some() {
        println!();
        println!("No named stacks found in {STACK_ROOTS_VAR}.");
    }

    0
}

fn open_docs() -> i32 {
    let target = find_local_docs()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| DOCS_URL.to_string());

    let result = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/c", "start", "", &target])
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(&target).spawn()
    } else {
        Command::new("xdg-open").arg(&target).spawn()
    };

    match result {
        Ok(_) => 0,
        Err(error) => {
            eprintln!("Error: Failed to open docs: {error}");
            1
        }
    }
}

fn find_local_docs() -> Option<PathBuf> {
    let current_exe = env::current_exe().ok()?;

    for ancestor in current_exe.ancestors().take(6) {
        let candidate = ancestor.join("docs").join("index.html");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

fn parse_allowlist_env() -> Option<Vec<String>> {
    let raw = env::var("ENVOY_ALLOWLIST").unwrap_or_default();
    if raw.is_empty() {
        return None;
    }

    let mut values = raw
        .replace(',', ";")
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    values.sort();
    values.dedup();

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn repr_string(value: &str) -> String {
    format!("{value:?}")
}

fn display_envoy_error(error: &EnvoyError) -> String {
    match error {
        EnvoyError::PreRun(message)
        | EnvoyError::PostRun(message)
        | EnvoyError::Execution(message)
        | EnvoyError::EnvironmentBuild(message)
        | EnvoyError::CommandNotFound(message)
        | EnvoyError::Validation(message) => message.clone(),
        EnvoyError::CalledProcess { .. } | EnvoyError::Io { .. } | EnvoyError::Json { .. } => {
            error.to_string()
        }
    }
}

fn debug(verbose: bool, message: &str) {
    if verbose {
        eprintln!("debug: {message}");
    }
}
