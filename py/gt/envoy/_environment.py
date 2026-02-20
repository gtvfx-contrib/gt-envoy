"""Environment variable handling for ApplicationWrapper."""

import os
import json
import re
import logging
from pathlib import Path
from typing import Any

from ._exceptions import WrapperError


log = logging.getLogger(__name__)


# Variables always seeded into the subprocess environment in closed mode.
# These provide identity, paths, and OS services that most processes assume
# are present. They are never secret and refusing them typically breaks
# tools in unexpected ways. The user allowlist (ENVOY_ALLOWLIST / --passthrough)
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


class EnvironmentManager:
    """Manages environment variable loading, expansion, and preparation.
    
    Handles:
    - Loading environment from JSON files
    - Variable expansion with {$VARNAME} syntax
    - Special wrapper variables like {$__BUNDLE__}
    - Path normalization (Unix → Windows)
    - List-based paths with automatic joining
    - Append (+=) and prepend (^=) operators
    
    Environment modes:
    - Closed (default): child process receives variables defined in env files,
      plus the built-in core OS variables (_CORE_ENV_VARS) and any additional
      variables listed in the user allowlist (ENVOY_ALLOWLIST / --passthrough).
    - Passthrough: child process inherits the full system environment, with env
      file values layered on top.
    
    """
    
    def __init__(self, inherit_env: bool = False, allowlist: set[str] | None = None):
        """Initialize the environment manager.
        
        Args:
            inherit_env: If True, child process inherits the full system environment
                (passthrough mode). If False, only env file variables and allowlisted
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
        
        Supports {$VARNAME} syntax to reference existing environment variables.
        Also supports special wrapper-internal variables with __ prefix/suffix.
        
        Special variables:
            {$__BUNDLE__}      - Root directory of the bundle (parent of envoy_env/)
            {$__BUNDLE_ENV__}  - The envoy_env/ directory itself
            {$__BUNDLE_NAME__} - Name of the bundle (directory name)
            {$__FILE__}         - Current environment JSON file being processed
        
        Lookup priority:
        1. Special wrapper variables (if provided)
        2. Current environment being built
        
        Unresolved references expand to an empty string. In closed mode
        only allowlisted variables are seeded into current_env, so unknown
        references produce empty strings rather than leaking system values.
        
        Args:
            value: String potentially containing {$VARNAME} references
            current_env: Current environment dictionary being built
            special_vars: Special wrapper-internal variables (optional)
            
        Returns:
            Expanded string value
            
        """
        # Pattern to match {$VARNAME}
        pattern = re.compile(r'\{\$([A-Za-z_][A-Za-z0-9_]*)\}')
        
        def replacer(match):
            var_name = match.group(1)
            
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
        - {$VARNAME} expansion (including special variables)
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
        
        # Expand any {$VARNAME} references (including special vars)
        expanded_value = self.expand_env_value(str_value, merged_env, special_vars)
        
        return expanded_value
    
    @staticmethod
    def get_special_variables(env_file_path: Path) -> dict[str, str]:
        """Calculate special wrapper-internal variables for an environment file.
        
        Special variables:
            __BUNDLE__      - Root directory of the bundle (parent of envoy_env/)
            __BUNDLE_ENV__  - The envoy_env/ directory itself
            __BUNDLE_NAME__ - Name of the bundle (directory name)
            env_file_path: Path to the environment JSON file
            
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
    ) -> dict[str, str]:
        """Load environment variables from JSON file(s).
        
        Files are merged in order, with later files overriding earlier ones.
        Supports variable expansion, append/prepend operators, and path lists.
        
        Examples in JSON:
            List:     "PYTHONPATH": ["R:/path1", "R:/path2", "R:/path3"]
            Append:   "+=PYTHONPATH": ["R:/new/path"]
            Prepend:  "^=PYTHONPATH": "R:/new/path"
            Replace:  "PYTHONPATH": "R:/new/path"
            Variable: "PYTHONPATH": "{$PYTHONPATH};R:/new/path"
            Special:  "PATH": "{$__BUNDLE__}/bin"
        
        Special wrapper variables:
            {$__BUNDLE__}      - Root directory of the bundle (parent of envoy_env/)
            {$__BUNDLE_ENV__}  - The envoy_env/ directory itself
            {$__BUNDLE_NAME__} - Name of the bundle (directory name)
            {$__FILE__}         - Current environment JSON file path
        
        Paths can use Unix-style forward slashes, automatically converted on Windows.
        
        Args:
            env_files: Single file path or list of file paths to load
            base_env: Variables already in scope before any file is processed.
                Used for {$VARNAME} expansion and as the starting point for +=
                and ^= operators.  Should be os.environ.copy() in passthrough
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
        
        # Seed from base_env so {$VAR} references and += operators see whatever
        # variables are legitimately in scope (allowlist or full system env).
        # A copy is taken so the caller's dict is never modified.
        merged_env: dict[str, str] = dict(base_env) if base_env else {}
        
        for file_path in env_files:
            path = Path(file_path)
            
            if not path.exists():
                raise WrapperError(f"Environment file not found: {path}")
            
            # Calculate special variables for this file
            special_vars = self.get_special_variables(path)
            
            try:
                with open(path, 'r', encoding='utf-8') as f:
                    file_env = json.load(f)
                
                if not isinstance(file_env, dict):
                    raise WrapperError(
                        f"Environment file must contain a JSON object: {path}"
                    )
                
                # Process each key-value pair
                for key, value in file_env.items():
                    # Check for operators
                    append_mode = False
                    prepend_mode = False
                    var_name = key
                    
                    if key.startswith('+='):
                        append_mode = True
                        var_name = key[2:]  # Remove += prefix
                    elif key.startswith('^='):
                        prepend_mode = True
                        var_name = key[2:]  # Remove ^= prefix
                    
                    # Process the value (handles lists, normalization, expansion)
                    processed_value = self.process_env_value(value, merged_env, special_vars)
                    
                    # Handle append/prepend operations
                    if append_mode or prepend_mode:
                        # Only look in merged_env — never fall back to os.environ.
                        # If the variable isn't defined yet it's treated as empty,
                        # making += and ^= equivalent to a plain assignment on first use.
                        current_value = merged_env.get(var_name, '')
                        
                        if append_mode:
                            # Append: current + separator + new
                            if current_value:
                                merged_env[var_name] = f"{current_value}{path_sep}{processed_value}"
                            else:
                                merged_env[var_name] = processed_value
                        else:  # prepend_mode
                            # Prepend: new + separator + current
                            if current_value:
                                merged_env[var_name] = f"{processed_value}{path_sep}{current_value}"
                            else:
                                merged_env[var_name] = processed_value
                    else:
                        # Normal assignment - just set the value
                        merged_env[var_name] = processed_value
                
                log.info(f"Loaded {len(file_env)} environment variables from {path}")
                
            except json.JSONDecodeError as e:
                raise WrapperError(f"Invalid JSON in environment file {path}: {e}") from e
            except Exception as e:
                raise WrapperError(f"Error reading environment file {path}: {e}") from e
        
        return merged_env
    
    def prepare_environment(
        self,
        env_files: str | Path | list[str | Path] | None = None,
        env: dict[str, str] | None = None
    ) -> dict[str, str]:
        """Prepare environment variables for subprocess.
        
        Priority (later overrides earlier):
        1. Allowlisted system variables (closed mode) or full system env (passthrough mode)
        2. Environment from JSON files
        3. Explicit environment dict
        
        Args:
            env_files: JSON file(s) to load environment from
            env: Explicit environment variables to add/override
            
        Returns:
            Dictionary of environment variables
            
        """
        if self.inherit_env:
            # Passthrough: start with the full system environment
            result_env = os.environ.copy()
        else:
            # Closed: always seed core OS variables first, then the user allowlist.
            # Core vars (identity, temp, system paths, locale, etc.) are safe to
            # carry through unconditionally and their absence tends to break tools
            # in unexpected ways.
            result_env = {}
            for var in _CORE_ENV_VARS | self.allowlist:
                if var in os.environ:
                    result_env[var] = os.environ[var]
        
        # Load from files (overrides inherited/seeded env).
        # Pass result_env as base_env so {$VAR} expansion and += / ^= operators
        # inside env files see exactly the same variables that will be in scope —
        # no silent leakage of system variables that aren't in base_env.
        file_env = self.load_env_from_files(env_files, base_env=result_env)
        result_env.update(file_env)
        
        # Explicit env dict overrides everything
        if env:
            result_env.update(env)
        
        return result_env
