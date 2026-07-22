use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use envoy_core::commands::{find_commands_file, CommandDefinition, CommandRegistry};
use envoy_core::config_registry::{
    is_config_name, list_named_configs, resolve_named_config, CFG_ROOTS_VAR,
};
use envoy_core::discovery::{get_bundles, BundleInfo};
use envoy_core::environment::{EnvironmentManager, TraceEvent};
use envoy_core::error::EnvoyError;
use envoy_core::executor::ProcessExecutor;
use envoy_core::models::WrapperConfig;
use envoy_core::package_cache::open_default_package_cache;
use envoy_core::runtime::{collect_env_files, is_raw_path, prepare_env, resolve_cached_bundles};
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

pub(crate) fn run(argv: &[String]) -> i32 {
    let cli = match args::parse(argv) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return error.exit_code();
        }
    };

    run_cli(cli)
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

    let (registry, bundles) = match load_registry_for_cli(&cli, &user_cfg, verbose) {
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

    run_command(
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
    )
}

fn load_registry_for_cli(
    cli: &Cli,
    user_cfg: &UserConfig,
    verbose: bool,
) -> Result<(CommandRegistry, Option<Vec<BundleInfo>>), i32> {
    let mut registry = CommandRegistry::empty();
    let mut bundles = None;
    let mut effective_bundles_config = None;

    // Resolve the local package cache once up front so both the explicit
    // bundles-config path and the auto-discovery path below apply it
    // consistently. Honors `--ignore-config` the same way `bundles_config`
    // does: skip the user-config-sourced cache directory, but still allow
    // the `ENVOY_PACKAGE_CACHE` / `ENVOY_DISABLE_PACKAGE_CACHE` env vars.
    let package_cache = open_default_package_cache(!cli.ignore_config);

    if let Some(raw_value) = cli.bundles_config.as_deref() {
        let resolved = resolve_config_value(raw_value, verbose)?;
        effective_bundles_config = Some(resolved);
    }

    if effective_bundles_config.is_none() && !cli.ignore_config {
        if let Some(stored) = user_cfg.get("bundles_config") {
            let resolved = resolve_config_value(stored, verbose)?;
            debug(
                verbose,
                &format!(
                    "Using bundles_config from user config: {}",
                    resolved.display()
                ),
            );
            effective_bundles_config = Some(resolved);
        }
    }

    if let Some(config_path) = effective_bundles_config {
        match get_bundles(Some(&config_path)) {
            Ok(discovered_bundles) => {
                if discovered_bundles.is_empty() {
                    debug(verbose, "No bundles found in config file");
                } else {
                    let discovered_bundles =
                        resolve_cached_bundles(discovered_bundles, package_cache.as_ref());
                    debug(
                        verbose,
                        &format!(
                            "Discovered {} bundle(s) from config file",
                            discovered_bundles.len()
                        ),
                    );
                    registry.load_from_bundles(&discovered_bundles);
                    bundles = Some(discovered_bundles);
                }
            }
            Err(error) => {
                eprintln!(
                    "Error loading bundles config: {}",
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
                let discovered_bundles =
                    resolve_cached_bundles(discovered_bundles, package_cache.as_ref());
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

    Ok((registry, bundles))
}

fn resolve_config_value(raw: &str, verbose: bool) -> Result<PathBuf, i32> {
    if is_config_name(raw) {
        let Some(resolved) = resolve_named_config(raw) else {
            eprintln!(
                "Error: Named config {raw:?} not found in {CFG_ROOTS_VAR}. Run 'envoy --list-configs' to see available configs."
            );
            return Err(1);
        };
        debug(
            verbose,
            &format!("Resolved named config {raw:?} to: {}", resolved.display()),
        );
        return Ok(resolved);
    }

    Ok(PathBuf::from(raw))
}

fn raw_path_without_env_override(command: Option<&str>, env_override: Option<&str>) -> bool {
    command.is_some_and(is_raw_path) && env_override.is_none()
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

    let named_configs = list_named_configs();
    if !named_configs.is_empty() {
        println!();
        println!("Available named configs ({CFG_ROOTS_VAR}):");
        println!();
        for entry in named_configs {
            println!("  {:<20}  version: {}", entry.name, entry.version);
            println!("    {}", entry.path.display());
        }
        println!();
        println!("Usage:  envoy --set-config bundles_config=<name>");
    } else if env::var_os(CFG_ROOTS_VAR).is_some() {
        println!();
        println!("No named configs found in {CFG_ROOTS_VAR}.");
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
