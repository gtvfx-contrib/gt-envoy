"""Environment variable handling for ApplicationWrapper."""

import os
import json
import re
import logging
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ._exceptions import WrapperError


log = logging.getLogger(__name__)


@dataclass
class TraceAllowlistEvent:
    """Emitted when the traced variable appears in an ``environment_allowlist``."""
    file_path: Path
    var_name: str
    #: True → the variable was actually written into the merged env.
    seeded: bool
    #: Value from ``os.environ`` at seed time (empty string if absent).
    os_value: str
    #: True → var was already present in ``merged_env`` (base_env wins); seeded is False.
    already_set: bool


@dataclass
class TraceStepEvent:
    """Emitted for each env-file entry that touches the traced variable."""
    file_path: Path
    var_name: str
    #: Operator string: ``'='``, ``'+='``, ``'^='``, or ``'?='``.
    operator: str
    #: Raw JSON value from the file (as produced by ``json.dumps``).
    raw_value: str
    #: Value after ``${VAR}`` expansion.
    expanded_value: str
    #: ``merged_env[var_name]`` before this entry was processed (empty string if absent).
    value_before: str
    #: ``merged_env[var_name]`` after this entry was processed.
    value_after: str
    #: False when a ``?=`` entry was skipped because the variable was already set.
    was_applied: bool


# Variables always seeded into the subprocess environment in closed mode.
# These provide identity, paths, and OS services that most processes assume
# are present. They are never secret and refusing them typically breaks
# tools in unexpected ways. The user allowlist (ENVOY_ALLOWLIST / --inherit-env)
# is additive on top of these.
_CORE_ENV_VARS: frozenset[str] = frozenset({
    # --- User identity & home ---
    'USERNAME',
    'USERPROFILE',
    'USERDOMAIN',
    'USERDOMAIN_ROAMINGPROFILE',
    'HOMEDRIVE',
    'HOMEPATH',
    # --- User data directories ---
    'APPDATA',
    'LOCALAPPDATA',
    'PUBLIC',
    'PROGRAMDATA',
    'ALLUSERSPROFILE',  # legacy alias for PROGRAMDATA, still set on all Windows versions
    # --- Temp ---
    'TEMP',
    'TMP',
    'TMPDIR',           # macOS / Linux
    # --- System / Windows layout ---
    'SystemRoot',
    'SystemDrive',
    'windir',
    'ProgramFiles',
    'ProgramFiles(x86)',
    'ProgramW6432',
    'CommonProgramFiles',
    'CommonProgramFiles(x86)',
    'CommonProgramW6432',
    # --- Hardware / OS identity ---
    'COMPUTERNAME',
    'OS',
    'PROCESSOR_ARCHITECTURE',
    'PROCESSOR_IDENTIFIER',
    'PROCESSOR_LEVEL',
    'PROCESSOR_REVISION',
    'NUMBER_OF_PROCESSORS',
    # --- Shell / console ---
    'COMSPEC',
    'PATHEXT',          # Windows: executable file extensions (.COM;.EXE;.BAT;.CMD;...)
    'TERM',
    'TERM_PROGRAM',
    'COLORTERM',
    # --- Unix identity (macOS / Linux) ---
    'HOME',
    'USER',
    'LOGNAME',
    'SHELL',
    # --- Locale / encoding ---
    'LANG',
    'LC_ALL',
    'LC_CTYPE',
    'LC_MESSAGES',
    # --- XDG base dirs (Linux) ---
    'XDG_RUNTIME_DIR',
    'XDG_CONFIG_HOME',
    'XDG_DATA_HOME',
    'XDG_CACHE_HOME',
})

# Envoy's own environment variables — always carried through so that child
# processes launched by a command (e.g. a build script that calls envoy again)
# inherit the same discovery and configuration context.
_ENVOY_ENV_VARS: frozenset[str] = frozenset({
    'ENVOY_BNDL_ROOTS',
    'ENVOY_ALLOWLIST',
    'ENVOY_BUNDLES_CONFIG',
})


class EnvironmentManager:
    """Manages environment variable loading, expansion, and preparation.
    
    Handles:
    - Loading environment from JSON files
    - Variable expansion with ${VARNAME} syntax
    - Special wrapper variables like ${__BUNDLE__}
    - Path normalization (Unix → Windows)
    - List-based paths with automatic joining
    - Append (+=), prepend (^=), and default (?=) operators
    
    Environment modes:
    - Closed (default): child process receives variables defined in env files,
      plus the built-in core OS variables (_CORE_ENV_VARS) and any additional
      variables listed in the user allowlist (ENVOY_ALLOWLIST / --inherit-env).
    - Inherit-env: child process inherits the full system environment, with env
      file values layered on top.
    
    """
    
    def __init__(self, inherit_env: bool = False, allowlist: set[str] | None = None):
        """Initialize the environment manager.
        
        Args:
            inherit_env: If True, child process inherits the full system environment
                (inherit-env mode). If False, only env file variables and allowlisted
                system variables are passed through (closed mode).
            allowlist: Set of system environment variable names to inherit even in
                closed mode. Typically sourced from ENVOY_ALLOWLIST.
            
        """
        self.inherit_env = inherit_env
        self.allowlist = allowlist or set()
    
    @staticmethod
    def expand_env_value(
        value: str, 
        current_env: dict[str, str],
        special_vars: dict[str, str] | None = None
    ) -> str:
        """Expand environment variable references in a value string.
        
        Supports ``${VARNAME}`` syntax to reference existing environment
        variables.  The legacy ``{$VARNAME}`` form is also accepted for
        backward compatibility.
        
        Special variables:
            ${__BUNDLE__}      - Root directory of the bundle (parent of envoy_env/)
            ${__BUNDLE_ENV__}  - The envoy_env/ directory itself
            ${__BUNDLE_NAME__} - Name of the bundle (directory name)
            ${__FILE__}        - Current environment JSON file being processed
        
        Lookup priority:
        1. Special wrapper variables (if provided)
        2. Current environment being built
        
        Unresolved references expand to an empty string. In closed mode
        only allowlisted variables are seeded into current_env, so unknown
        references produce empty strings rather than leaking system values.
        
        Args:
            value: String potentially containing ${VARNAME} references
            current_env: Current environment dictionary being built
            special_vars: Special wrapper-internal variables (optional)
            
        Returns:
            Expanded string value
            
        """
        # Primary form: ${VARNAME}  (POSIX/shell standard)
        # Back-compat:  {$VARNAME}  (legacy envoy form)
        pattern = re.compile(r'\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\{\$([A-Za-z_][A-Za-z0-9_]*)\}')
        
        def replacer(match):
            # Group 1 = ${VAR} style (canonical), group 2 = {$VAR} style (back-compat)
            var_name = match.group(1) or match.group(2)
            
            # Check special variables first (highest priority)
            if special_vars and var_name in special_vars:
                return special_vars[var_name]
            
            # Check current_env second
            if var_name in current_env:
                return current_env[var_name]
            
            # Unresolved — return empty string (never read from os.environ here).
            return ''
        
        return pattern.sub(replacer, value)
    
    @staticmethod
    def normalize_path(path: str) -> str:
        """Normalize Unix-style paths to OS-specific format.
        
        Converts forward slashes to backslashes on Windows.
        Leaves paths unchanged on Unix systems.
        
        Args:
            path: Path string (can use Unix-style forward slashes)
            
        Returns:
            Normalized path for the current OS
            
        """
        if os.name == 'nt':
            # On Windows, convert forward slashes to backslashes
            return path.replace('/', '\\')
        return path
    
    def process_env_value(
        self, 
        value: Any, 
        merged_env: dict[str, str],
        special_vars: dict[str, str] | None = None
    ) -> str:
        """Process an environment variable value from JSON.
        
        Handles:
        - Lists: joined with OS path separator
        - Strings: used as-is
        - Other types: converted to string
        - ${VARNAME} expansion (including special variables)
        - Paths stored in UNIX style (forward slashes) for cross-platform compatibility
        
        Args:
            value: The value from JSON (string, list, or other)
            merged_env: Current environment dictionary for variable expansion
            special_vars: Special wrapper-internal variables (optional)
            
        Returns:
            Processed string value
            
        """
        # Determine path separator based on OS
        path_sep = ';' if os.name == 'nt' else ':'
        
        # Handle list values - join with path separator
        if isinstance(value, list):
            # Keep paths in UNIX style (forward slashes) for consistency
            str_value = path_sep.join(str(item) for item in value)
        else:
            # Convert to string, keep as-is
            str_value = str(value)
        
        # Expand any ${VARNAME} references (including special vars)
        expanded_value = self.expand_env_value(str_value, merged_env, special_vars)
        
        return expanded_value
    
    @staticmethod
    def get_special_variables(env_file_path: Path) -> dict[str, str]:
        """Calculate special wrapper-internal variables for an environment file.
        
        Special variables (available as ``${NAME}`` in env file values):
            __BUNDLE__      - Root directory of the bundle (parent of envoy_env/)
            __BUNDLE_ENV__  - The envoy_env/ directory itself
            __BUNDLE_NAME__ - Name of the bundle (directory name)
            __FILE__        - Path to the current environment JSON file
            
        Returns:
            Dictionary of special variable names and their values
            
        """
        env_file_abs = env_file_path.resolve()
        
        # Try to find the envoy_env/ directory by walking up the path
        current = env_file_abs.parent
        package_env_dir = None
        package_root = None
        
        # Look for 'envoy_env' directory in the path
        for parent in [current] + list(current.parents):
            if parent.name == 'envoy_env':
                package_env_dir = parent
                package_root = parent.parent
                break
        
        # If no envoy_env/ directory found, use file's parent as bundle root
        if package_root is None:
            package_root = env_file_abs.parent
            package_env_dir = package_root
        
        # Convert to cross-platform paths (keep forward slashes - will be normalized later)
        # The paths will use forward slashes internally and get normalized
        # to backslashes on Windows during normalize_path processing
        special_vars = {
            '__FILE__': str(env_file_abs).replace('\\', '/'),
            '__BUNDLE__': str(package_root).replace('\\', '/'),
            '__BUNDLE_ENV__': str(package_env_dir).replace('\\', '/'),
            '__BUNDLE_NAME__': package_root.name,
        }
        
        return special_vars
    
    def load_env_from_files(
        self, 
        env_files: str | Path | list[str | Path] | None,
        base_env: dict[str, str] | None = None,
        trace_var: str | None = None,
        trace_out: list | None = None,
    ) -> dict[str, str]:
        """Load environment variables from JSON file(s).
        
        Files are merged in order, with later files overriding earlier ones.
        Supports variable expansion, append/prepend operators, and path lists.
        
        Two top-level JSON formats are accepted:

        **Flat format** — every key is a variable name (with optional operator):

        .. code-block:: json

            {
                "PYTHONPATH": ["R:/path1", "R:/path2"],
                "+=PYTHONPATH": "R:/extra",
                "?=USER_ROOT": "${LOCALAPPDATA}/myapp",
                "PATH": "${__BUNDLE__}/bin"
            }

        **Structured format** — top-level dict has an ``"environment"`` key:

        .. code-block:: json

            {
                "environment": [
                    ["+=PYTHONPATH", "${__BUNDLE__}/python"],
                    ["?=MY_TOOL_HOME", "${__BUNDLE__}"]
                ],
                "environment_allowlist": ["PYTHONPATH", "MY_EXISTING_VAR"]
            }

        In structured format ``"environment"`` may be either:

        * A **list of ``[key, value]`` pairs** (shown above).
        * A **dict** — identical semantics to the flat format.

        The ``"environment_allowlist"`` key (optional) lists OS environment
        variables that should be seeded into scope before any file's entries
        are processed.  All ``"environment_allowlist"`` declarations across
        **all** files in the list are aggregated in a pre-pass and seeded from
        ``os.environ`` at once, before the main processing loop begins.  This
        means a var declared in a later file's allowlist is already visible to
        ``+=`` / ``^=`` operators in earlier files.  Variables not present in
        ``os.environ`` are silently skipped.
        
        Operator prefix summary:
            ``+=VAR`` — append (current + sep + new)
            ``^=VAR`` — prepend (new + sep + current)
            ``?=VAR`` — default (set only if VAR is not already defined)
            (none)   — replace unconditionally

        Special wrapper variables available in ``${...}`` expansion:
            ``${__BUNDLE__}``      — bundle root directory (parent of ``envoy_env/``)
            ``${__BUNDLE_ENV__}``  — the ``envoy_env/`` directory itself
            ``${__BUNDLE_NAME__}`` — bundle directory name
            ``${__FILE__}``        — current environment JSON file path
        
        Args:
            env_files: Single file path or list of file paths to load
            base_env: Variables already in scope before any file is processed.
                Used for ${VARNAME} expansion (with {$VARNAME} as a legacy alias) and as the starting point for +=
                and ^= operators.  Should be os.environ.copy() in inherit-env
                mode, or the allowlist-seeded dict in closed mode.  Never
                modified — a copy is taken before file processing begins.
            
        Returns:
            Dictionary of environment variables from files (base_env entries
            are included so callers can update result_env with this return value)
            
        Raises:
            WrapperError: If file cannot be read or parsed
            
        """
        if not env_files:
            return dict(base_env) if base_env else {}
        
        # Normalize to list
        if isinstance(env_files, (str, Path)):
            env_files = [env_files]
        
        # Determine path separator based on OS
        path_sep = ';' if os.name == 'nt' else ':'
        
        # Seed from base_env so ${VAR} references and += operators see whatever
        # variables are legitimately in scope (allowlist or full system env).
        # A copy is taken so the caller's dict is never modified.
        merged_env: dict[str, str] = dict(base_env) if base_env else {}

        # --- Pre-pass: parse all files and aggregate environment_allowlist ---
        # Collect allowlist entries from every file before any variable
        # expansion runs, then seed them all from os.environ at once.
        # This means a var declared in a later file's allowlist is already
        # visible to += / ^= operators in earlier files.
        parsed_files: list[tuple[Path, Any]] = []
        for file_path in env_files:
            path = Path(file_path)
            if not path.exists():
                raise WrapperError(f"Environment file not found: {path}")
            try:
                with open(path, 'r', encoding='utf-8') as f:
                    file_data = json.load(f)
            except json.JSONDecodeError as e:
                raise WrapperError(f"Invalid JSON in environment file {path}: {e}") from e
            parsed_files.append((path, file_data))

        # Aggregate and seed allowlist vars before the main processing loop.
        # Only seed vars that are not already in merged_env (base_env wins).
        for _path, _data in parsed_files:
            if isinstance(_data, dict):
                for var in _data.get('environment_allowlist', []):
                    already_set = var in merged_env
                    in_os = var in os.environ
                    if not already_set and in_os:
                        merged_env[var] = os.environ[var]
                    if trace_out is not None and trace_var == var:
                        trace_out.append(TraceAllowlistEvent(
                            file_path=_path,
                            var_name=var,
                            seeded=not already_set and in_os,
                            os_value=os.environ.get(var, ''),
                            already_set=already_set,
                        ))

        # --- Main pass: process each file's entries in order -----------------
        for path, file_data in parsed_files:
            # Calculate special variables for this file
            special_vars = self.get_special_variables(path)

            try:
                if isinstance(file_data, list):
                    # Bare list format: [[key, value], [key, value], ...]
                    items = []
                    for idx, entry in enumerate(file_data):
                        if not (isinstance(entry, (list, tuple)) and len(entry) == 2):
                            raise WrapperError(
                                f'list entry {idx} in {path} must be a '
                                f'[key, value] pair, got: {entry!r}'
                            )
                        items.append((entry[0], entry[1]))

                elif not isinstance(file_data, dict):
                    raise WrapperError(
                        f"Environment file must contain a JSON object or array: {path}"
                    )

                elif 'environment' in file_data:
                    # Structured dict format -----------------------------------
                    # (allowlist already seeded in the pre-pass above)

                    # Normalise "environment" to an iterable of (key, value).
                    env_entries = file_data['environment']
                    if isinstance(env_entries, dict):
                        items: list[tuple[str, Any]] = list(env_entries.items())
                    elif isinstance(env_entries, list):
                        items = []
                        for idx, entry in enumerate(env_entries):
                            if not (isinstance(entry, (list, tuple)) and len(entry) == 2):
                                raise WrapperError(
                                    f'environment entry {idx} in {path} must be a '
                                    f'[key, value] pair, got: {entry!r}'
                                )
                            items.append((entry[0], entry[1]))
                    else:
                        raise WrapperError(
                            f'"environment" in {path} must be a list or dict, '
                            f'got {type(env_entries).__name__}'
                        )

                    unknown_keys = set(file_data) - {'environment', 'environment_allowlist'}
                    if unknown_keys:
                        log.warning(
                            'Unknown keys in structured env file %s: %s',
                            path, ', '.join(sorted(unknown_keys)),
                        )

                else:
                    # Flat dict format -----------------------------------------
                    items = list(file_data.items())

                # ------------------------------------------------------------------
                # Process each (key, value) pair — identical logic for both formats.
                # ------------------------------------------------------------------
                for key, value in items:
                    append_mode = False
                    prepend_mode = False
                    default_mode = False
                    var_name = key
                    
                    if key.startswith('?='):
                        default_mode = True
                        var_name = key[2:]
                    elif key.startswith('+='):
                        append_mode = True
                        var_name = key[2:]
                    elif key.startswith('^='):
                        prepend_mode = True
                        var_name = key[2:]
                    
                    # Process the value (handles lists, normalization, expansion)
                    processed_value = self.process_env_value(value, merged_env, special_vars)

                    value_before = merged_env.get(var_name, '')
                    was_applied = True
                    if default_mode:
                        if var_name not in merged_env:
                            merged_env[var_name] = processed_value
                        else:
                            was_applied = False
                    elif append_mode or prepend_mode:
                        current_value = merged_env.get(var_name, '')
                        if append_mode:
                            if current_value:
                                merged_env[var_name] = f"{current_value}{path_sep}{processed_value}"
                            else:
                                merged_env[var_name] = processed_value
                        else:  # prepend_mode
                            if current_value:
                                merged_env[var_name] = f"{processed_value}{path_sep}{current_value}"
                            else:
                                merged_env[var_name] = processed_value
                    else:
                        merged_env[var_name] = processed_value

                    if trace_out is not None and trace_var == var_name:
                        operator = '?=' if default_mode else '+=' if append_mode else '^=' if prepend_mode else '='
                        trace_out.append(TraceStepEvent(
                            file_path=path,
                            var_name=var_name,
                            operator=operator,
                            raw_value=json.dumps(value),
                            expanded_value=processed_value,
                            value_before=value_before,
                            value_after=merged_env.get(var_name, ''),
                            was_applied=was_applied,
                        ))
                
                log.info('Loaded environment from %s (%d entries)', path, len(items))

            except WrapperError:
                raise
            except Exception as e:
                raise WrapperError(f"Error reading environment file {path}: {e}") from e
        
        return merged_env
    
    def prepare_environment(
        self,
        env_files: str | Path | list[str | Path] | None = None,
        env: dict[str, str] | None = None,
        trace_var: str | None = None,
        trace_out: list | None = None,
    ) -> dict[str, str]:
        """Prepare environment variables for subprocess.
        
        Priority (later overrides earlier):
        1. Allowlisted system variables (closed mode) or full system env (inherit-env mode)
        2. Environment from JSON files
        3. Explicit environment dict
        
        Args:
            env_files: JSON file(s) to load environment from
            env: Explicit environment variables to add/override
            
        Returns:
            Dictionary of environment variables
            
        """
        if self.inherit_env:
            # Inherit-env: start with the full system environment
            result_env = os.environ.copy()
        else:
            # Closed: always seed core OS variables first, then the user allowlist.
            # Core vars (identity, temp, system paths, locale, etc.) are safe to
            # carry through unconditionally and their absence tends to break tools
            # in unexpected ways.  Envoy's own vars are included so child processes
            # that invoke envoy again inherit the same discovery context.
            result_env = {}
            for var in _CORE_ENV_VARS | _ENVOY_ENV_VARS | self.allowlist:
                if var in os.environ:
                    result_env[var] = os.environ[var]
        
        # Load from files (overrides inherited/seeded env).
        # Pass result_env as base_env so ${VAR} expansion and += / ^= operators
        # inside env files see exactly the same variables that will be in scope —
        # no silent leakage of system variables that aren't in base_env.
        file_env = self.load_env_from_files(env_files, base_env=result_env, trace_var=trace_var, trace_out=trace_out)
        result_env.update(file_env)
        
        # Explicit env dict overrides everything
        if env:
            result_env.update(env)
        
        return result_env
