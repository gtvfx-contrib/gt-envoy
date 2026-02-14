"""Environment variable handling for ApplicationWrapper."""

import os
import json
import re
import logging
from pathlib import Path
from typing import Any

from ._exceptions import WrapperError


log = logging.getLogger(__name__)


class EnvironmentManager:
    """Manages environment variable loading, expansion, and preparation.
    
    Handles:
    - Loading environment from JSON files
    - Variable expansion with {$VARNAME} syntax
    - Special wrapper variables like {$__PACKAGE__}
    - Path normalization (Unix → Windows)
    - List-based paths with automatic joining
    - Append (+=) and prepend (^=) operators
    
    """
    
    def __init__(self, inherit_env: bool = True):
        """Initialize the environment manager.
        
        Args:
            inherit_env: Whether to inherit system environment variables
            
        """
        self.inherit_env = inherit_env
    
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
            {$__PACKAGE__}      - Root directory of the package (parent of env/)
            {$__PACKAGE_ENV__}  - The env/ directory itself
            {$__PACKAGE_NAME__} - Name of the package (directory name)
            {$__FILE__}         - Current environment JSON file being processed
        
        Lookup priority:
        1. Special wrapper variables (if provided)
        2. Current environment being built
        3. System environment variables
        
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
            
            # Fall back to os.environ
            return os.environ.get(var_name, '')
        
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
        - Path normalization (Unix to Windows if needed)
        
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
            # Normalize each path in the list
            normalized_paths = [self.normalize_path(str(item)) for item in value]
            str_value = path_sep.join(normalized_paths)
        else:
            # Convert to string and normalize
            str_value = self.normalize_path(str(value))
        
        # Expand any {$VARNAME} references (including special vars)
        expanded_value = self.expand_env_value(str_value, merged_env, special_vars)
        
        return expanded_value
    
    @staticmethod
    def get_special_variables(env_file_path: Path) -> dict[str, str]:
        """Calculate special wrapper-internal variables for an environment file.
        
        Special variables:
            __PACKAGE__      - Root directory of the package (parent of env/)
            __PACKAGE_ENV__  - The env/ directory itself
            __PACKAGE_NAME__ - Name of the package (directory name)
            __FILE__         - Current environment JSON file being processed
        
        Args:
            env_file_path: Path to the environment JSON file
            
        Returns:
            Dictionary of special variable names and their values
            
        """
        env_file_abs = env_file_path.resolve()
        
        # Try to find the env/ directory by walking up the path
        current = env_file_abs.parent
        package_env_dir = None
        package_root = None
        
        # Look for 'env' directory in the path
        for parent in [current] + list(current.parents):
            if parent.name == 'env':
                package_env_dir = parent
                package_root = parent.parent
                break
        
        # If no env/ directory found, use file's parent as package root
        if package_root is None:
            package_root = env_file_abs.parent
            package_env_dir = package_root
        
        # Convert to cross-platform paths (keep forward slashes - will be normalized later)
        # The paths will use forward slashes internally and get normalized
        # to backslashes on Windows during normalize_path processing
        special_vars = {
            '__FILE__': str(env_file_abs).replace('\\', '/'),
            '__PACKAGE__': str(package_root).replace('\\', '/'),
            '__PACKAGE_ENV__': str(package_env_dir).replace('\\', '/'),
            '__PACKAGE_NAME__': package_root.name,
        }
        
        return special_vars
    
    def load_env_from_files(
        self, 
        env_files: str | Path | list[str | Path] | None
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
            Special:  "PATH": "{$__PACKAGE__}/bin"
        
        Special wrapper variables:
            {$__PACKAGE__}      - Root directory of the package (parent of env/)
            {$__PACKAGE_ENV__}  - The env/ directory itself
            {$__PACKAGE_NAME__} - Name of the package (directory name)
            {$__FILE__}         - Current environment JSON file path
        
        Paths can use Unix-style forward slashes, automatically converted on Windows.
        
        Args:
            env_files: Single file path or list of file paths to load
            
        Returns:
            Dictionary of environment variables from files
            
        Raises:
            WrapperError: If file cannot be read or parsed
            
        """
        if not env_files:
            return {}
        
        # Normalize to list
        if isinstance(env_files, (str, Path)):
            env_files = [env_files]
        
        # Determine path separator based on OS
        path_sep = ';' if os.name == 'nt' else ':'
        
        merged_env = {}
        
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
                        # Get current value from merged_env or os.environ
                        current_value = merged_env.get(var_name) or os.environ.get(var_name, '')
                        
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
        1. System environment (if inherit_env=True)
        2. Environment from JSON files (env_files)
        3. Explicit environment dict (env)
        
        Args:
            env_files: JSON file(s) to load environment from
            env: Explicit environment variables to add/override
            
        Returns:
            Dictionary of environment variables
            
        """
        if self.inherit_env:
            result_env = os.environ.copy()
        else:
            result_env = {}
        
        # Load from files (overrides inherited env)
        file_env = self.load_env_from_files(env_files)
        result_env.update(file_env)
        
        # Explicit env dict overrides everything
        if env:
            result_env.update(env)
        
        return result_env
