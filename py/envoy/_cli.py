"""Command-line interface for envoy."""

import os
import sys
import argparse
import logging
from importlib.metadata import version as _metadata_version, PackageNotFoundError as _PackageNotFoundError
from pathlib import Path

try:
    _VERSION = _metadata_version('envoy')
except _PackageNotFoundError:
    try:
        from ._version import __version__ as _VERSION
    except ImportError:
        _VERSION = '0.0.0+uninstalled'

from ._commands import CommandRegistry, findCommandsFile
from ._discovery import getBundles, BundleInfo
from ._wrapper import ApplicationWrapper
from ._environment import EnvironmentManager, TraceAllowlistEvent, TraceStepEvent
from ._executor import ProcessExecutor
from ._models import WrapperConfig
from ._exceptions import WrapperError
from ._user_config import UserConfig, KNOWN_SETTINGS
from ._config_registry import (
    isConfigName, resolveNamedConfig, listNamedConfigs, CFG_ROOTS_VAR,
)


log = logging.getLogger(__name__)


def setupLogging(verbose: bool = False) -> None:
    """Setup logging configuration.
    
    Args:
        verbose: Enable verbose logging
        
    """
    level = logging.DEBUG if verbose else logging.WARNING
    logging.basicConfig(
        level=level,
        format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
    )


def listCommands(registry: CommandRegistry) -> int:
    """List all available commands.
    
    Args:
        registry: Command registry
        
    Returns:
        Exit code (0 for success)
        
    """
    commands = registry.listCommands()
    
    if not commands:
        print("No commands defined.")
        return 1
    
    print("Available commands:")
    print()
    
    for cmd_name in commands:
        cmd = registry.get(cmd_name)
        if cmd:
            # Build command display
            bundle_str = f" [{cmd.bundle}]" if cmd.bundle else ""
            
            if cmd.alias:
                alias_str = " ".join(cmd.alias)
                print(f"  {cmd_name:<20} → {alias_str}{bundle_str}")
            else:
                print(f"  {cmd_name:<20} (executable on PATH){bundle_str}")
    
    return 0


def showCommandInfo(registry: CommandRegistry, command_name: str) -> int:
    """Show detailed information about a command.
    
    Args:
        registry: Command registry
        command_name: Name of command to show
        
    Returns:
        Exit code (0 for success)
        
    """
    cmd = registry.get(command_name)
    
    if not cmd:
        print(f"Error: Command '{command_name}' not found")
        return 1
    
    print(f"Command: {command_name}")
    
    if cmd.bundle:
        print(f"Bundle: {cmd.bundle}")
    
    print(f"Executable: {cmd.executable}")
    
    if cmd.base_args:
        print(f"Base args: {' '.join(cmd.base_args)}")
    
    try:
        resolved_env = registry.resolveEnvironment(command_name)
    except WrapperError as e:
        print(f"Error resolving environment for '{command_name}': {e}", file=sys.stderr)
        return 1

    print(f"Environment files:")
    for env_file_name, _env_dir in resolved_env:
        print(f"  - {env_file_name}")
    
    if cmd.envoy_env_dir:
        print(f"Environment directory: {cmd.envoy_env_dir}")
    
    if cmd.alias:
        print(f"Alias: {' '.join(cmd.alias)}")
    
    return 0


def showWhich(
    registry: CommandRegistry,
    command_name: str,
    bundles: list[BundleInfo] | None = None,
    inherit_env: bool = False,
    env_allowlist: set[str] | None = None,
) -> int:
    """Show the resolved executable path for a command.
    
    Builds the subprocess environment from the command's env files so that
    PATH resolution matches what the child process would actually see.
    
    Args:
        registry: Command registry
        command_name: Name of command to find
        bundles: Discovered bundles (for multi-bundle env file search)
        inherit_env: Whether to inherit the full system environment
        env_allowlist: System variable names to seed in closed mode
        
    Returns:
        Exit code (0 for success)
        
    """
    cmd = registry.get(command_name)
    
    if not cmd:
        print(f"Error: Command '{command_name}' not found", file=sys.stderr)
        return 1
    
    executable = cmd.expandAlias()[0]
    
    if cmd.alias:
        alias_str = " ".join(cmd.alias)
        print(f"command {command_name} aliased to: {alias_str}")
        if cmd.source_file:
            print(f"  defined in: {cmd.source_file}")
        return 0
    
    # Build env files the same way runCommand does so PATH is correct.
    env_files = []
    try:
        resolved_env = registry.resolveEnvironment(command_name)
    except WrapperError as e:
        print(f"Error resolving environment for '{command_name}': {e}", file=sys.stderr)
        return 1

    if bundles:
        for bundle in bundles:
            if 'global_env.json' in bundle.env_files:
                env_files.append(str(bundle.env_files['global_env.json']))
        for env_file_name, _env_dir in resolved_env:
            for bundle in bundles:
                if env_file_name in bundle.env_files:
                    env_files.append(str(bundle.env_files[env_file_name]))
    elif cmd.envoy_env_dir:
        global_env = cmd.envoy_env_dir / 'global_env.json'
        if global_env.exists():
            env_files.append(str(global_env))
        for env_file_name, env_dir in resolved_env:
            dir_to_use = env_dir or cmd.envoy_env_dir
            env_files.append(str(dir_to_use / env_file_name))
    
    env_mgr = EnvironmentManager(inherit_env=inherit_env, allowlist=env_allowlist)
    try:
        env = env_mgr.prepareEnvironment(env_files=[Path(f) for f in env_files])
    except WrapperError as e:
        print(f"Warning: Could not build environment: {e}", file=sys.stderr)
        env = {}
    
    # Resolve using the subprocess PATH.
    try:
        resolved = ProcessExecutor.resolveExecutable(executable, search_path=env.get('PATH'))
        print(f"command {command_name} resolved to: {resolved}")
        if cmd.source_file:
            print(f"  defined in: {cmd.source_file}")
    except WrapperError:
        print(f"command {command_name} executable: {executable} (not found on PATH)")
        if cmd.source_file:
            print(f"  defined in: {cmd.source_file}")
    
    return 0


def runCommand(
    registry: CommandRegistry,
    command_name: str,
    args: list[str],
    bundles: list[BundleInfo] | None = None,
    verbose: bool = False,
    inherit_env: bool = False,
    env_allowlist: set[str] | None = None,
    env_override: str | None = None,
) -> int:
    """Run a command from the registry.

    Args:
        registry: Command registry
        command_name: Name of command to run
        args: Arguments to pass to the command
        bundles: List of discovered bundles (for multi-bundle env file search)
        verbose: Enable verbose output
        inherit_env: If True, child process inherits the full system environment
        env_allowlist: System variable names to inherit in closed mode
        env_override: Optional name of another envoy command whose environment
            files should be used in place of *command_name*'s own environment.
            The target command's executable/alias is still used; only the
            environment resolution is replaced.

    Returns:
        Exit code from the executed command

    """
    cmd = registry.get(command_name)

    if not cmd:
        print(f"Error: Command '{command_name}' not found", file=sys.stderr)
        print(f"Run 'envoy --list' to see available commands", file=sys.stderr)
        return 1

    # When an env override is requested, validate it exists and use its
    # environment resolution instead of the target command's own env files.
    env_source_name = command_name
    if env_override is not None:
        if registry.get(env_override) is None:
            print(
                f"Error: Environment override command '{env_override}' not found",
                file=sys.stderr,
            )
            print(f"Run 'envoy --list' to see available commands", file=sys.stderr)
            return 1
        env_source_name = env_override
        log.debug(f"Using environment from '{env_override}' for command '{command_name}'")

    # Collect environment files
    env_files = []

    if bundles:
        # Multi-bundle mode: use pre-indexed env_files dict — no filesystem calls at run time
        for bundle in bundles:
            if 'global_env.json' in bundle.env_files:
                env_files.append(str(bundle.env_files['global_env.json']))
                log.debug(f"Found global environment file: {bundle.env_files['global_env.json']}")

        try:
            resolved_env = registry.resolveEnvironment(env_source_name)
        except WrapperError as e:
            print(f"Error resolving environment for '{env_source_name}': {e}", file=sys.stderr)
            return 1

        for env_file_name, _env_dir in resolved_env:
            for bundle in bundles:
                if env_file_name in bundle.env_files:
                    env_files.append(str(bundle.env_files[env_file_name]))
                    log.debug(f"Found environment file: {bundle.env_files[env_file_name]}")
    else:
        # Legacy mode: use the env-source command's envoy_env_dir for env resolution
        # but the target command's dir as fallback.
        env_source_cmd = registry.get(env_source_name)
        env_dir_cmd = env_source_cmd if env_source_cmd is not None else cmd

        if env_dir_cmd.envoy_env_dir:
            wrapper_env_dir = env_dir_cmd.envoy_env_dir
        else:
            # Fall back to finding commands.json
            commands_file = findCommandsFile()
            if commands_file:
                wrapper_env_dir = commands_file.parent
            else:
                print(f"Error: Cannot determine envoy_env directory", file=sys.stderr)
                return 1

        # Collect global_env.json first if it exists
        global_env = wrapper_env_dir / 'global_env.json'
        if global_env.exists():
            env_files.append(str(global_env))
            log.debug(f"Found global environment file: {global_env}")

        # Resolve env file names (expanding any command-name references) and
        # build full paths using each entry's owning directory.
        try:
            resolved_env = registry.resolveEnvironment(env_source_name)
        except WrapperError as e:
            print(f"Error resolving environment for '{env_source_name}': {e}", file=sys.stderr)
            return 1

        cmd_env_files = [
            str((env_dir or wrapper_env_dir) / env_file_name)
            for env_file_name, env_dir in resolved_env
        ]

        # Verify all environment files exist (only in legacy mode)
        for env_file in cmd_env_files:
            if not Path(env_file).exists():
                print(f"Error: Environment file not found: {env_file}", file=sys.stderr)
                return 1

        env_files.extend(cmd_env_files)
    
    # Expand ${__BUNDLE__} and other special vars in alias parts.
    expanded = cmd.expandAlias()
    # Combine base args with user args
    full_args = expanded[1:] + args
    
    # Create wrapper config
    config = WrapperConfig(
        executable=expanded[0],
        args=full_args,
        env_files=[Path(f) for f in env_files],
        inherit_env=inherit_env,
        env_allowlist=env_allowlist,
        capture_output=False,
        stream_output=False,
        log_execution=verbose
    )
    
    try:
        wrapper = ApplicationWrapper(config)
        result = wrapper.run()
        return result.return_code
        
    except WrapperError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("\nInterrupted", file=sys.stderr)
        return 130


def traceCommand(
    registry: CommandRegistry,
    command_name: str,
    trace_var: str,
    bundles: list[BundleInfo] | None = None,
    inherit_env: bool = False,
    env_allowlist: set[str] | None = None,
    env_override: str | None = None,
) -> int:
    """Show how *trace_var* is mutated through env file processing for *command_name*.

    Prints each mutation step (allowlist seeding, per-file entries) and exits
    without running the command.

    Args:
        registry: Command registry
        command_name: Name of the command whose env files to trace
        trace_var: Environment variable name to trace
        bundles: Discovered bundles (for multi-bundle env file search)
        inherit_env: Whether to inherit the full system environment
        env_allowlist: System variable names to seed in closed mode
        env_override: Alternate command whose env files to use

    Returns:
        Exit code (0 for success)

    """
    cmd = registry.get(command_name)
    if not cmd:
        print(f"Error: Command '{command_name}' not found", file=sys.stderr)
        return 1

    env_source_name = command_name
    if env_override is not None:
        if registry.get(env_override) is None:
            print(f"Error: Environment override command '{env_override}' not found", file=sys.stderr)
            return 1
        env_source_name = env_override

    # Collect environment files (mirrors runCommand logic)
    env_files: list[str] = []
    if bundles:
        for bundle in bundles:
            if 'global_env.json' in bundle.env_files:
                env_files.append(str(bundle.env_files['global_env.json']))
        try:
            resolved_env = registry.resolveEnvironment(env_source_name)
        except WrapperError as e:
            print(f"Error resolving environment for '{env_source_name}': {e}", file=sys.stderr)
            return 1
        for env_file_name, _env_dir in resolved_env:
            for bundle in bundles:
                if env_file_name in bundle.env_files:
                    env_files.append(str(bundle.env_files[env_file_name]))
    else:
        env_source_cmd = registry.get(env_source_name)
        env_dir_cmd = env_source_cmd if env_source_cmd is not None else cmd
        if env_dir_cmd.envoy_env_dir:
            wrapper_env_dir = env_dir_cmd.envoy_env_dir
        else:
            commands_file = findCommandsFile()
            if commands_file:
                wrapper_env_dir = commands_file.parent
            else:
                print("Error: Cannot determine envoy_env directory", file=sys.stderr)
                return 1
        global_env = wrapper_env_dir / 'global_env.json'
        if global_env.exists():
            env_files.append(str(global_env))
        try:
            resolved_env = registry.resolveEnvironment(env_source_name)
        except WrapperError as e:
            print(f"Error resolving environment for '{env_source_name}': {e}", file=sys.stderr)
            return 1
        cmd_env_files = [
            str((env_dir or wrapper_env_dir) / env_file_name)
            for env_file_name, env_dir in resolved_env
        ]
        for env_file in cmd_env_files:
            if not Path(env_file).exists():
                print(f"Error: Environment file not found: {env_file}", file=sys.stderr)
                return 1
        env_files.extend(cmd_env_files)

    # Build environment with tracing enabled
    trace_events: list = []
    env_mgr = EnvironmentManager(inherit_env=inherit_env, allowlist=env_allowlist)
    final_env = env_mgr.prepareEnvironment(
        env_files=[Path(f) for f in env_files],
        trace_var=trace_var,
        trace_out=trace_events,
    )
    final_value = final_env.get(trace_var)

    # ── Format output ────────────────────────────────────────────────────────
    sep = '─' * 64
    env_label = f" (via --env {env_override})" if env_override else ""
    print(f"\nTracing '{trace_var}' for command '{command_name}'{env_label}\n")

    print(f"Env files ({len(env_files)}):")
    for i, f in enumerate(env_files, 1):
        print(f"  [{i}] {f}")
    print()

    # Pre-pass: allowlist seeding
    allowlist_events = [e for e in trace_events if isinstance(e, TraceAllowlistEvent)]
    print(sep)
    print("Pre-pass: allowlist seeding")
    if allowlist_events:
        for ev in allowlist_events:
            file_label = Path(ev.file_path).name
            if ev.already_set:
                print(f"  {file_label}  {trace_var} in environment_allowlist (already in base env, skipped)")
            elif ev.seeded:
                print(f"  {file_label}  {trace_var} in environment_allowlist")
                print(f"    → seeded from os.environ: {ev.os_value!r}")
            else:
                print(f"  {file_label}  {trace_var} in environment_allowlist (not present in os.environ)")
    else:
        print(f"  {trace_var} not listed in any environment_allowlist")
    print()

    # Per-file processing
    print(sep)
    print("File processing:")
    step_events = [e for e in trace_events if isinstance(e, TraceStepEvent)]
    for i, f in enumerate(env_files, 1):
        file_path = Path(f)
        steps = [e for e in step_events if Path(e.file_path) == file_path]
        print(f"\n  [{i}] {file_path}")
        if not steps:
            print(f"       {trace_var} not mentioned")
        else:
            for ev in steps:
                before_str = repr(ev.value_before) if ev.value_before else "<not set>"
                print(f"       {ev.operator}{trace_var}: {ev.raw_value}")
                print(f"         before:   {before_str}")
                if ev.expanded_value != ev.raw_value.strip('"'):
                    print(f"         expanded: {ev.expanded_value!r}")
                if not ev.was_applied:
                    print(f"         → no-op ({ev.operator} skipped, variable already set)")
                else:
                    print(f"         after:    {ev.value_after!r}")
    print()

    # Final result
    print(sep)
    if final_value is not None:
        print(f"Result: {trace_var} = {final_value!r}")
    else:
        print(f"Result: {trace_var} is not set")
    print()

    return 0


# ---------------------------------------------------------------------------
# User config handlers
# ---------------------------------------------------------------------------

def handleSetConfig(raw: str) -> int:
    """Handle ``--set-config KEY=VALUE`` or ``--set-config KEY=`` (clear).

    Args:
        raw: The raw ``KEY=VALUE`` string from the command line.

    Returns:
        Exit code (0 on success, 1 on error).

    """
    if '=' not in raw:
        print(
            f"Error: --set-config requires KEY=VALUE format "
            f"(use KEY= to clear a setting)",
            file=sys.stderr,
        )
        return 1

    key, _, value = raw.partition('=')
    key = key.strip()

    cfg = UserConfig.load()

    if value == '':
        if cfg.unset(key):
            cfg.save()
            print(f"Cleared: {key}")
        else:
            print(f"Nothing to clear: {key!r} was not set")
        return 0

    try:
        cfg.set(key, value)
    except ValueError as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    cfg.save()
    print(f"Saved: {key} = {value!r}")
    print(f"Config: {cfg.path}")
    return 0


def handleGetConfig(key: str | None) -> int:
    """Handle ``--get-config [KEY]``.

    Args:
        key: Specific setting to show, or ``None`` / empty string to show all.

    Returns:
        Exit code.

    """
    cfg = UserConfig.load()

    if key:
        value = cfg.get(key)
        if value is None:
            print(f"{key}: <not set>")
        else:
            print(f"{key} = {value!r}")
        return 0

    settings = cfg.items()
    if not settings:
        print("No config settings are set.")
        print(f"Config: {cfg.path}")
        return 0

    print(f"Config: {cfg.path}")
    print()
    for setting_key in sorted(settings):
        print(f"  {setting_key} = {settings[setting_key]!r}")
    return 0


def handleListConfigs() -> int:
    """Handle ``--list-configs``.

    Prints all known configurable settings with their descriptions, then lists
    any named configs available from ``ENVOY_CFG_ROOTS``.

    Returns:
        Exit code (always 0).

    """
    cfg = UserConfig.load()

    print("Configurable settings:")
    print()
    for setting_key, meta in sorted(KNOWN_SETTINGS.items()):
        current = cfg.get(setting_key)
        status = f"(current: {current!r})" if current is not None else "(not set)"
        choices_str = f"  Choices: {', '.join(meta['choices'])}" if meta.get('choices') else ""
        print(f"  {setting_key}  {status}")
        print(f"    {meta['description']}")
        if choices_str:
            print(f"   {choices_str}")
        print()

    print(f"Config file: {cfg.path}")
    print()
    print("Usage:  envoy --set-config KEY=VALUE")
    print("        envoy --set-config KEY=       (clear a setting)")

    # Show available named configs when ENVOY_CFG_ROOTS is set.
    named = listNamedConfigs()
    if named:
        print()
        print(f"Available named configs ({CFG_ROOTS_VAR}):")
        print()
        for entry in named:
            print(f"  {entry.name:<20}  version: {entry.version}")
            print(f"    {entry.path}")
        print()
        print("Usage:  envoy --set-config bundles_config=<name>")
    elif os.environ.get(CFG_ROOTS_VAR):
        print()
        print(f"No named configs found in {CFG_ROOTS_VAR}.")

    return 0


def main(argv: list[str] | None = None) -> int:
    """Main CLI entry point.
    
    Args:
        argv: Command-line arguments (defaults to sys.argv[1:])
        
    Returns:
        Exit code
        
    """
    parser = argparse.ArgumentParser(
        prog='envoy',
        description='Envoy: Environment orchestration for applications'
    )

    parser.add_argument(
        '--version',
        action='version',
        version=f'%(prog)s {_VERSION}',
    )

    parser.add_argument(
        '--docs',
        action='store_true',
        help='Open the envoy documentation in the default browser.',
    )

    parser.add_argument(
        '--list',
        action='store_true',
        help='List all available commands'
    )
    
    parser.add_argument(
        '--info',
        metavar='COMMAND',
        help='Show detailed information about a command'
    )
    
    parser.add_argument(
        '--which',
        metavar='COMMAND',
        help='Show the resolved executable path for a command'
    )
    
    parser.add_argument(
        '--commands-file', '-c',
        type=Path,
        help='Path to commands.json file (auto-detected if not specified)'
    )
    
    parser.add_argument(
        '--bundles-config', '-b',
        type=Path,
        help='Path to bundles config file (auto-discovers from ENVOY_BNDL_ROOTS if not specified)'
    )

    parser.add_argument(
        '--set-config', '-sc',
        metavar='KEY=VALUE',
        help=(
            'Set a user config value and save it. '
            'Use KEY= (empty value) to clear a setting. '
            'Example: --set-config bundles_config=/path/to/bundles.json'
        ),
    )

    parser.add_argument(
        '--get-config', '-gc',
        metavar='KEY',
        nargs='?',
        const='',
        help=(
            'Print one or all user config values and exit. '
            'Omit KEY to print all settings.'
        ),
    )

    parser.add_argument(
        '--list-configs', '-lc',
        action='store_true',
        help='List all known configurable settings with their descriptions and exit.',
    )

    parser.add_argument(
        '--ignore-config', '-ic',
        action='store_true',
        help=(
            'Bypass the user config for this invocation. '
            'Auto-discovery via ENVOY_BNDL_ROOTS proceeds as if no user config is set.'
        ),
    )

    parser.add_argument(
        '--env', '-e',
        metavar='ENV_COMMAND',
        help=(
            'Run the target command inside a different command\'s environment. '
            'E.g. "envoy -e krita python" runs python using the krita environment.'
        )
    )

    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Enable verbose logging'
    )
    
    parser.add_argument(
        '--inherit-env', '-i',
        action='store_true',
        help='Inherit the full system environment (overrides default closed environment mode)'
    )

    parser.add_argument(
        '--trace',
        metavar='VAR',
        help=(
            'Show how VAR is mutated through env file processing for COMMAND. '
            'Prints each mutation step (allowlist seeding, per-file operators) '
            'and exits without running COMMAND. '
            'Example: envoy --trace UE_PYTHONPATH unreal'
        )
    )
    
    parser.add_argument(
        'command',
        nargs='?',
        help='Command to execute'
    )
    
    parser.add_argument(
        'args',
        nargs=argparse.REMAINDER,
        help='Arguments to pass to the command'
    )
    
    # Parse args - REMAINDER captures everything after the command verbatim,
    # including flags like -c or --version that belong to the child process.
    if argv is None:
        argv = sys.argv[1:]
    
    args = parser.parse_args(argv)

    # --list-configs / --set-config / --get-config: config-only operations, exit early.
    if args.list_configs:
        return handleListConfigs()

    if args.set_config is not None:
        return handleSetConfig(args.set_config)

    if args.get_config is not None:
        return handleGetConfig(args.get_config or None)

    # Setup logging
    setupLogging(args.verbose)

    # --docs: open the documentation site and exit immediately.
    if args.docs:
        import webbrowser
        _DOCS_URL = 'https://gtvfx-contrib.github.io/gt-envoy/'
        if getattr(sys, 'frozen', False):
            # Running as a PyInstaller exe: bin/envoy.exe → parent = bin/ → parent = bundle root
            _bundle_root = Path(sys.executable).parent.parent
        else:
            # Running from source or wheel: py/envoy/_cli.py → up 3 levels = bundle root
            _bundle_root = Path(__file__).parent.parent.parent
        _local_docs = _bundle_root / 'docs' / 'index.html'
        webbrowser.open(_local_docs.as_uri() if _local_docs.exists() else _DOCS_URL)
        return 0

    # Load user config (used unless --ignore-config is set).
    user_cfg = UserConfig.load()

    # Initialize command registry
    registry = CommandRegistry()
    bundles = None  # Track discovered bundles for env file resolution

    # Apply default verbosity from user config (CLI --verbose always wins).
    if not args.verbose and not args.ignore_config:
        cfg_verbosity = user_cfg.get('verbosity')
        if cfg_verbosity == 'verbose':
            setupLogging(verbose=True)

    # Determine command loading strategy.
    #
    # Priority:
    #   1. --bundles-config CLI flag  (may be a path or a named config)
    #   2. user config bundles_config (unless --ignore-config)
    #   3. auto-discovery via ENVOY_BNDL_ROOTS
    #   4. local commands.json fallback
    effective_bundles_config: Path | None = None

    # Resolve the raw bundles-config value (path or named config) to a Path.
    def _resolveConfigValue(raw: str | Path) -> Path | None:
        """Resolve a bundles-config value — path or named config — to a Path."""
        value = str(raw)
        if isConfigName(value):
            resolved = resolveNamedConfig(value)
            if resolved is None:
                print(
                    f"Error: Named config {value!r} not found in {CFG_ROOTS_VAR}. "
                    f"Run 'envoy --list-configs' to see available configs.",
                    file=sys.stderr,
                )
                return None
            log.debug("Resolved named config %r to: %s", value, resolved)
            return resolved
        return Path(value)

    if args.bundles_config is not None:
        raw_value = str(args.bundles_config)
        if isConfigName(raw_value):
            effective_bundles_config = _resolveConfigValue(raw_value)
            if effective_bundles_config is None:
                return 1
        else:
            effective_bundles_config = args.bundles_config

    if effective_bundles_config is None and not args.ignore_config:
        stored = user_cfg.get('bundles_config')
        if stored:
            effective_bundles_config = _resolveConfigValue(stored)
            if effective_bundles_config is None:
                return 1
            log.debug("Using bundles_config from user config: %s", effective_bundles_config)

    if effective_bundles_config:
        # Load from bundles config file
        try:
            discovered_bundles = getBundles(config_file=effective_bundles_config)
            if discovered_bundles:
                log.info(f"Discovered {len(discovered_bundles)} bundle(s) from config file")
                registry.loadFromBundles(discovered_bundles)
                bundles = discovered_bundles
            else:
                log.warning("No bundles found in config file")
        except WrapperError as e:
            print(f"Error loading bundles config: {e}", file=sys.stderr)
            return 1
    elif args.commands_file:
        # Load from specific commands file (legacy mode)
        if not args.commands_file.exists():
            print(f"Error: Commands file not found: {args.commands_file}", file=sys.stderr)
            return 1
        try:
            registry.loadFromFile(args.commands_file)
        except WrapperError as e:
            print(f"Error loading commands: {e}", file=sys.stderr)
            return 1
    else:
        # Try bundle auto-discovery first
        try:
            discovered_bundles = getBundles()
            if discovered_bundles:
                log.info(f"Auto-discovered {len(discovered_bundles)} bundle(s)")
                registry.loadFromBundles(discovered_bundles)
                bundles = discovered_bundles
        except WrapperError as e:
            log.debug(f"Bundle auto-discovery failed: {e}")

        # Fall back to local commands.json if no bundles found
        if len(registry) == 0:
            commands_file = findCommandsFile()
            if commands_file:
                try:
                    registry.loadFromFile(commands_file)
                except WrapperError as e:
                    print(f"Error loading commands: {e}", file=sys.stderr)
                    return 1
            else:
                print("Error: Could not find commands.json", file=sys.stderr)
                print("Searched for envoy_env/commands.json in current directory and parents", file=sys.stderr)
                print("Or set ENVOY_BNDL_ROOTS environment variable for auto-discovery", file=sys.stderr)
                return 1
    
    # Check if we have any commands
    if len(registry) == 0:
        print("Error: No commands loaded", file=sys.stderr)
        return 1
    
    # Handle list commands
    if args.list:
        return listCommands(registry)
    
    # Handle command info
    if args.info:
        return showCommandInfo(registry, args.info)
    
    # Parse allowlist and inherit-env — needed by both --which and run.
    allowlist_str = os.environ.get('ENVOY_ALLOWLIST', '')
    env_allowlist = (
        {v.strip() for v in allowlist_str.replace(',', ';').split(';') if v.strip()}
        if allowlist_str else None
    )
    if env_allowlist:
        log.debug(f"Allowlist: {sorted(env_allowlist)}")

    # Handle which
    if args.which:
        return showWhich(
            registry,
            args.which,
            bundles=bundles,
            inherit_env=args.inherit_env,
            env_allowlist=env_allowlist,
        )

    # Handle trace
    if args.trace:
        if not args.command:
            print("Error: --trace requires a COMMAND argument", file=sys.stderr)
            print("Example: envoy --trace UE_PYTHONPATH unreal", file=sys.stderr)
            return 1
        return traceCommand(
            registry=registry,
            command_name=args.command,
            trace_var=args.trace,
            bundles=bundles,
            inherit_env=args.inherit_env,
            env_allowlist=env_allowlist,
            env_override=args.env,
        )
    
    # Must have a command to execute
    if not args.command:
        parser.print_help()
        return 0
    
    # Execute command
    return runCommand(
        registry=registry,
        command_name=args.command,
        args=args.args,
        bundles=bundles,
        verbose=args.verbose,
        inherit_env=args.inherit_env,
        env_allowlist=env_allowlist,
        env_override=args.env,
    )


if __name__ == '__main__':
    sys.exit(main())
