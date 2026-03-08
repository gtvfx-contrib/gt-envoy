"""Command loading and management for CLI wrapper."""

import json
import logging
import os
from pathlib import Path

from ._exceptions import WrapperError


log = logging.getLogger(__name__)


class CommandDefinition:
    """Represents a command definition from commands.json.
    
    Attributes:
        name: Command name (the JSON key)
        environment: List of environment JSON files to load
        alias: Optional list of command parts to execute instead of command name
        bundle: Optional bundle bndlid this command comes from (``'<ns>:<name>'``,
            e.g. ``'gt:pythoncore'``).  ``None`` for commands loaded from a
            bare ``commands.json`` without bundle context.
        envoy_env_dir: Directory containing environment files
        
    """
    
    def __init__(
        self,
        name: str,
        environment: list[str],
        alias: list[str] | None = None,
        bundle: str | None = None,
        envoy_env_dir: Path | None = None
    ):
        """Initialize command definition.
        
        Args:
            name: Command name
            environment: List of environment file names
            alias: Optional alias command parts (e.g., ["python", "-m", "module"])
            bundle: Optional bundle name this command belongs to
            envoy_env_dir: Optional directory containing environment files
            
        """
        self.name = name
        self.environment = environment
        self.alias = alias
        self.bundle = bundle  # bndlid string, e.g. 'gt:pythoncore'
        self.envoy_env_dir = envoy_env_dir
    
    @property
    def executable(self) -> str:
        """Get the executable for this command.
        
        Returns:
            The executable path/name - either from alias or command name
            
        """
        if self.alias:
            return self.alias[0]
        return self.name
    
    @property
    def base_args(self) -> list[str]:
        """Get the base arguments for this command.
        
        Returns:
            List of arguments that come before user-supplied args
            
        """
        if self.alias and len(self.alias) > 1:
            return self.alias[1:]
        return []
    
    def __repr__(self) -> str:
        """String representation."""
        alias_str = f" (alias: {' '.join(self.alias)})" if self.alias else ""
        bundle_str = f" [{self.bundle}]" if self.bundle else ""
        return f"CommandDefinition({self.name}{bundle_str}{alias_str}, env={self.environment})"


class CommandRegistry:
    """Manages loading and access to command definitions."""
    
    def __init__(self, commands_file: Path | None = None):
        """Initialize the command registry.
        
        Args:
            commands_file: Optional path to commands.json file
            
        """
        self._commands: dict[str, CommandDefinition] = {}
        self._bundle_sources: dict[str, str] = {}  # cmd_name -> bundle bndlid
        if commands_file:
            self.load_from_file(commands_file)
    
    def load_from_file(self, commands_file: Path, bundle_name: str | None = None) -> None:
        """Load commands from a JSON file.
        
        Args:
            commands_file: Path to commands.json
            bundle_name: Optional bundle bndlid (e.g. ``'gt:pythoncore'``) used
                to track command origin and populate
                :attr:`CommandDefinition.bundle`.  Pass ``None`` for commands
                loaded directly from a bare ``commands.json`` without a
                discovered bundle.
            
        Raises:
            WrapperError: If file cannot be read or parsed
            
        """
        if not commands_file.exists():
            raise WrapperError(f"Commands file not found: {commands_file}")
        
        wrapper_env_dir = commands_file.parent
        
        try:
            with open(commands_file, 'r', encoding='utf-8') as f:
                commands_data = json.load(f)
            
            if not isinstance(commands_data, dict):
                raise WrapperError(
                    f"Commands file must contain a JSON object: {commands_file}"
                )
            
            for cmd_name, cmd_config in commands_data.items():
                if not isinstance(cmd_config, dict):
                    log.warning(f"Skipping invalid command definition: {cmd_name}")
                    continue
                
                # Validate required fields
                if 'environment' not in cmd_config:
                    log.warning(f"Command '{cmd_name}' missing 'environment' field, skipping")
                    continue
                
                environment = cmd_config.get('environment', [])
                if not isinstance(environment, list):
                    log.warning(f"Command '{cmd_name}' has invalid 'environment' field, skipping")
                    continue
                
                alias = cmd_config.get('alias')
                if alias is not None and not isinstance(alias, list):
                    log.warning(f"Command '{cmd_name}' has invalid 'alias' field, skipping")
                    continue
                
                cmd_def = CommandDefinition(
                    name=cmd_name,
                    environment=environment,
                    alias=alias,
                    bundle=bundle_name,
                    envoy_env_dir=wrapper_env_dir
                )
                
                # Track conflicts
                if cmd_name in self._commands:
                    existing_bundle = self._bundle_sources.get(cmd_name, 'unknown')
                    log.warning(
                        f"Command '{cmd_name}' from {bundle_name or 'local'} "
                        f"overrides existing command from {existing_bundle}"
                    )
                
                self._commands[cmd_name] = cmd_def
                self._bundle_sources[cmd_name] = bundle_name or 'local'
            
            bundle_label = f" from bundle '{bundle_name}'" if bundle_name else ""
            log.info(f"Loaded {len(commands_data)} command(s) from {commands_file}{bundle_label}")
            
        except json.JSONDecodeError as e:
            raise WrapperError(f"Invalid JSON in commands file {commands_file}: {e}") from e
        except Exception as e:
            raise WrapperError(f"Error reading commands file {commands_file}: {e}") from e
    
    def resolve_environment(
        self,
        command_name: str,
        _seen: frozenset[str] | None = None,
    ) -> list[tuple[str, 'Path | None']]:
        """Resolve the full environment file list for a command.

        Entries in a command's ``environment`` list that contain no dot are
        treated as references to another command; their resolved environment
        lists are spliced in at that position, recursively.  Entries that
        contain a dot (e.g. ``"python_env.json"``) are treated as plain file
        names.

        This allows commands to inherit another command's environment::

            "unreal57": {
                "environment": ["unreal", "unreal57_env.json"],
                "alias": ["unreal57.bat"]
            }

        Args:
            command_name: Name of the command to resolve.
            _seen: Internal frozenset of visited command names used for cycle
                detection — callers should not pass this.

        Returns:
            Flat list of ``(filename, envoy_env_dir)`` pairs in load order.
            ``envoy_env_dir`` is the directory that owns the file and should be
            used to construct the full path in legacy (non-bundle) mode.

        Raises:
            WrapperError: If a referenced command is not found or a cycle is
                detected.

        """
        if _seen is None:
            _seen = frozenset()

        if command_name in _seen:
            raise WrapperError(
                f"Circular environment reference detected at command '{command_name}'"
            )

        cmd = self.get(command_name)
        if cmd is None:
            raise WrapperError(
                f"Environment reference '{command_name}' does not match any known command"
            )

        _seen = _seen | {command_name}
        result: list[tuple[str, Path | None]] = []

        for entry in cmd.environment:
            # An entry with no dot is treated as a command name reference;
            # anything with a dot (e.g. "python_env.json") is a plain file.
            if '.' not in Path(entry).name:
                result.extend(self.resolve_environment(entry, _seen=_seen))
            else:
                result.append((entry, cmd.envoy_env_dir))

        return result

    def load_from_bundles(self, bundles: list) -> None:
        """Load commands from multiple bundles.
        
        Args:
            bundles: List of BundleInfo (or Bundle) objects from discovery.
                Each entry must expose ``.bndlid``, ``.envoy_env``, and
                ``.name`` attributes.
            
        """
        for bundle in bundles:
            commands_file = bundle.envoy_env / "commands.json"
            if commands_file.exists():
                try:
                    self.load_from_file(commands_file, bundle_name=bundle.bndlid)
                except WrapperError as e:
                    log.warning(f"Failed to load commands from bundle {bundle.bndlid}: {e}")
    
    def get(self, command_name: str) -> CommandDefinition | None:
        """Get a command definition by name.
        
        Args:
            command_name: Name of the command
            
        Returns:
            CommandDefinition if found, None otherwise
            
        """
        return self._commands.get(command_name)
    
    def list_commands(self) -> list[str]:
        """Get list of all available command names.
        
        Returns:
            Sorted list of command names
            
        """
        return sorted(self._commands.keys())
    
    def __contains__(self, command_name: str) -> bool:
        """Check if a command exists.
        
        Args:
            command_name: Name to check
            
        Returns:
            True if command exists
            
        """
        return command_name in self._commands
    
    def __len__(self) -> int:
        """Get number of registered commands."""
        return len(self._commands)


def find_commands_file(start_path: Path | None = None) -> Path | None:
    """Find commands.json by searching up the directory tree for envoy_env/.

    Resolution order:

    1. ``ENVOY_COMMANDS_FILE`` environment variable — if set, that path is
       returned directly (used by :func:`~envoy.testing.patch_commands_file`
       in tests).
    2. Upward directory walk from *start_path* (or cwd) looking for
       ``envoy_env/commands.json``.

    Args:
        start_path: Starting directory (defaults to cwd).

    Returns:
        Path to commands.json if found, None otherwise.
        
    """
    # 1. Honour the override env var set by envoy_testing.patch_commands_file.
    env_override = os.environ.get('ENVOY_COMMANDS_FILE')
    if env_override:
        p = Path(env_override).resolve()
        if p.is_file():
            return p
        if p.exists():
            raise WrapperError(
                f"ENVOY_COMMANDS_FILE does not point to a file: {env_override!r}"
            )

    # 2. Walk up from start_path.
    if start_path is None:
        start_path = Path.cwd()

    current = start_path.resolve()

    for parent in [current] + list(current.parents):
        wrapper_env_dir = parent / "envoy_env"
        if wrapper_env_dir.is_dir():
            commands_file = wrapper_env_dir / "commands.json"
            if commands_file.exists():
                return commands_file

    return None
