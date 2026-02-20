"""Command-line interface for envoy."""

import os
import sys
import argparse
import logging
from pathlib import Path

from ._commands import CommandRegistry, find_commands_file
from ._discovery import get_bundles, BundleInfo
from ._wrapper import ApplicationWrapper
from ._environment import EnvironmentManager
from ._executor import ProcessExecutor
from ._models import WrapperConfig
from ._exceptions import WrapperError


log = logging.getLogger(__name__)


def setup_logging(verbose: bool = False) -> None:
    """Setup logging configuration.
    
    Args:
        verbose: Enable verbose logging
        
    """
    level = logging.DEBUG if verbose else logging.WARNING
    logging.basicConfig(
        level=level,
        format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
    )


def list_commands(registry: CommandRegistry) -> int:
    """List all available commands.
    
    Args:
        registry: Command registry
        
    Returns:
        Exit code (0 for success)
        
    """
    commands = registry.list_commands()
    
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


def show_command_info(registry: CommandRegistry, command_name: str) -> int:
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
    
    print(f"Environment files:")
    for env_file in cmd.environment:
        print(f"  - {env_file}")
    
    if cmd.envoy_env_dir:
        print(f"Environment directory: {cmd.envoy_env_dir}")
    
    if cmd.alias:
        print(f"Alias: {' '.join(cmd.alias)}")
    
    return 0


def show_which(
    registry: CommandRegistry,
    command_name: str,
    bundles: list[BundleInfo] | None = None,
    passthrough: bool = False,
    env_allowlist: set[str] | None = None,
) -> int:
    """Show the resolved executable path for a command.
    
    Builds the subprocess environment from the command's env files so that
    PATH resolution matches what the child process would actually see.
    
    Args:
        registry: Command registry
        command_name: Name of command to find
        bundles: Discovered bundles (for multi-bundle env file search)
        passthrough: Whether to inherit the full system environment
        env_allowlist: System variable names to seed in closed mode
        
    Returns:
        Exit code (0 for success)
        
    """
    cmd = registry.get(command_name)
    
    if not cmd:
        print(f"Error: Command '{command_name}' not found", file=sys.stderr)
        return 1
    
    executable = cmd.executable
    
    if cmd.alias:
        alias_str = " ".join(cmd.alias)
        print(f"command {command_name} aliased to: {alias_str}")
        return 0
    
    # Build env files the same way run_command does so PATH is correct.
    env_files = []
    if bundles:
        for bundle in bundles:
            if 'global_env.json' in bundle.env_files:
                env_files.append(str(bundle.env_files['global_env.json']))
        for env_file_name in cmd.environment:
            for bundle in bundles:
                if env_file_name in bundle.env_files:
                    env_files.append(str(bundle.env_files[env_file_name]))
    elif cmd.envoy_env_dir:
        global_env = cmd.envoy_env_dir / 'global_env.json'
        if global_env.exists():
            env_files.append(str(global_env))
        env_files.extend(str(cmd.envoy_env_dir / f) for f in cmd.environment)
    
    env_mgr = EnvironmentManager(inherit_env=passthrough, allowlist=env_allowlist)
    try:
        env = env_mgr.prepare_environment(env_files=[Path(f) for f in env_files])
    except WrapperError as e:
        print(f"Warning: Could not build environment: {e}", file=sys.stderr)
        env = {}
    
    # Resolve using the subprocess PATH.
    try:
        resolved = ProcessExecutor.resolve_executable(executable, search_path=env.get('PATH'))
        print(f"command {command_name} resolved to: {resolved}")
    except WrapperError:
        print(f"command {command_name} executable: {executable} (not found on PATH)")
    
    return 0


def run_command(
    registry: CommandRegistry,
    command_name: str,
    args: list[str],
    bundles: list[BundleInfo] | None = None,
    verbose: bool = False,
    passthrough: bool = False,
    env_allowlist: set[str] | None = None
) -> int:
    """Run a command from the registry.
    
    Args:
        registry: Command registry
        command_name: Name of command to run
        args: Arguments to pass to the command
        bundles: List of discovered bundles (for multi-bundle env file search)
        verbose: Enable verbose output
        passthrough: If True, child process inherits the full system environment
        env_allowlist: System variable names to inherit in closed mode
        
    Returns:
        Exit code from the executed command
        
    """
    cmd = registry.get(command_name)
    
    if not cmd:
        print(f"Error: Command '{command_name}' not found", file=sys.stderr)
        print(f"Run 'envoy --list' to see available commands", file=sys.stderr)
        return 1
    
    # Collect environment files
    env_files = []
    
    if bundles:
        # Multi-bundle mode: use pre-indexed env_files dict — no filesystem calls at run time
        for bundle in bundles:
            if 'global_env.json' in bundle.env_files:
                env_files.append(str(bundle.env_files['global_env.json']))
                log.debug(f"Found global environment file: {bundle.env_files['global_env.json']}")
        
        for env_file_name in cmd.environment:
            for bundle in bundles:
                if env_file_name in bundle.env_files:
                    env_files.append(str(bundle.env_files[env_file_name]))
                    log.debug(f"Found environment file: {bundle.env_files[env_file_name]}")
    else:
        # Legacy mode: use command's envoy_env_dir
        if cmd.envoy_env_dir:
            wrapper_env_dir = cmd.envoy_env_dir
        else:
            # Fall back to finding commands.json
            commands_file = find_commands_file()
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
        
        # Build full environment file paths
        cmd_env_files = [str(wrapper_env_dir / env_file) for env_file in cmd.environment]
        
        # Verify all environment files exist (only in legacy mode)
        for env_file in cmd_env_files:
            if not Path(env_file).exists():
                print(f"Error: Environment file not found: {env_file}", file=sys.stderr)
                return 1
        
        env_files.extend(cmd_env_files)
    
    # Combine base args with user args
    full_args = cmd.base_args + args
    
    # Create wrapper config
    config = WrapperConfig(
        executable=cmd.executable,
        args=full_args,
        env_files=[Path(f) for f in env_files],
        inherit_env=passthrough,
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
        '--commands-file', '-cf',
        type=Path,
        help='Path to commands.json file (auto-detected if not specified)'
    )
    
    parser.add_argument(
        '--bundles-config', '-bc',
        type=Path,
        help='Path to bundles config file (auto-discovers from ENVOY_BNDL_ROOTS if not specified)'
    )
    
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Enable verbose logging'
    )
    
    parser.add_argument(
        '--passthrough', '-pt',
        action='store_true',
        help='Inherit the full system environment (overrides default closed environment mode)'
    )
    
    parser.add_argument(
        'command',
        nargs='?',
        help='Command to execute'
    )
    
    parser.add_argument(
        'args',
        nargs='*',
        help='Arguments to pass to the command'
    )
    
    # Parse args - use parse_known_args to allow passthrough to commands
    if argv is None:
        argv = sys.argv[1:]
    
    args, unknown_args = parser.parse_known_args(argv)
    
    # Combine args with any unknown args (these should be passed to the command)
    if unknown_args:
        args.args = list(args.args) + unknown_args
    
    # Setup logging
    setup_logging(args.verbose)
    
    # Initialize command registry
    registry = CommandRegistry()
    bundles = None  # Track discovered bundles for env file resolution
    
    # Determine command loading strategy
    if args.bundles_config:
        # Load from bundles config file
        try:
            discovered_bundles = get_bundles(config_file=args.bundles_config)
            if discovered_bundles:
                log.info(f"Discovered {len(discovered_bundles)} bundle(s) from config file")
                registry.load_from_bundles(discovered_bundles)
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
            registry.load_from_file(args.commands_file)
        except WrapperError as e:
            print(f"Error loading commands: {e}", file=sys.stderr)
            return 1
    else:
        # Try bundle auto-discovery first
        try:
            discovered_bundles = get_bundles()
            if discovered_bundles:
                log.info(f"Auto-discovered {len(discovered_bundles)} bundle(s)")
                registry.load_from_bundles(discovered_bundles)
                bundles = discovered_bundles
        except WrapperError as e:
            log.debug(f"Bundle auto-discovery failed: {e}")
        
        # Fall back to local commands.json if no bundles found
        if len(registry) == 0:
            commands_file = find_commands_file()
            if commands_file:
                try:
                    registry.load_from_file(commands_file)
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
        return list_commands(registry)
    
    # Handle command info
    if args.info:
        return show_command_info(registry, args.info)
    
    # Parse allowlist and passthrough — needed by both --which and run.
    allowlist_str = os.environ.get('ENVOY_ALLOWLIST', '')
    env_allowlist = (
        {v.strip() for v in allowlist_str.replace(',', ';').split(';') if v.strip()}
        if allowlist_str else None
    )
    if env_allowlist:
        log.debug(f"Allowlist: {sorted(env_allowlist)}")

    # Handle which
    if args.which:
        return show_which(
            registry,
            args.which,
            bundles=bundles,
            passthrough=args.passthrough,
            env_allowlist=env_allowlist,
        )
    
    # Must have a command to execute
    if not args.command:
        parser.print_help()
        return 0
    
    # Execute command
    return run_command(
        registry=registry,
        command_name=args.command,
        args=args.args,
        bundles=bundles,
        verbose=args.verbose,
        passthrough=args.passthrough,
        env_allowlist=env_allowlist
    )


if __name__ == '__main__':
    sys.exit(main())
