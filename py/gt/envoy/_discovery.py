"""Bundle discovery for wrapper environments.

Supports two methods of discovering bundles:
1. Auto-discovery: Search directories specified in ENVOY_BNDL_ROOTS for git repositories
2. Config file: Explicit list of bundle paths

"""

import os
import logging
from pathlib import Path
import json

from ._exceptions import WrapperError


logger = logging.getLogger(__name__)


class BundleInfo:
    """Information about a discovered bundle."""
    
    def __init__(self, root: Path, name: str):
        """Initialize bundle information.
        
        Args:
            root: Root directory of the bundle
            name: Name of the bundle (directory name)
        
        """
        self.root = root
        self.name = name
        self.envoy_env = root / "envoy_env"
        self.env_files: dict[str, Path] = self._index_env_files()

    def _index_env_files(self) -> dict[str, Path]:
        """Scan envoy_env/ once and index all JSON files by filename.
        
        Returns:
            Dict mapping filename to absolute Path
        
        """
        if not self.envoy_env.is_dir():
            return {}
        return {f.name: f for f in self.envoy_env.glob('*.json')}
        
    def __repr__(self):
        return f"BundleInfo(name={self.name}, root={self.root})"
    
    def __str__(self):
        return f"{self.name} ({self.root})"


def is_git_repo(path: Path) -> bool:
    """Check if a directory is a git repository.
    
    Args:
        path: Path to check
        
    Returns:
        True if path contains a .git directory
    
    """
    return (path / ".git").is_dir()


def has_envoy_env(path: Path) -> bool:
    """Check if a directory has an envoy_env subdirectory.
    
    Args:
        path: Path to check
        
    Returns:
        True if path contains an envoy_env directory
    
    """
    return (path / "envoy_env").is_dir()


def validate_bundle(path: Path) -> bool:
    """Validate that a path is a valid envoy bundle.
    
    A valid bundle must:
    - Be a directory
    - Have an envoy_env subdirectory
    
    Args:
        path: Path to validate
        
    Returns:
        True if path is a valid bundle
    
    """
    if not path.is_dir():
        return False
    
    if not has_envoy_env(path):
        return False
    
    return True


def find_git_repos(root_dir: Path, max_depth: int = 5) -> list[Path]:
    """Recursively find git repositories under a root directory.
    
    Args:
        root_dir: Root directory to search
        max_depth: Maximum depth to search
        
    Returns:
        List of paths to git repository roots
    
    """
    repos = []
    
    if not root_dir.is_dir():
        logger.warning(f"Root directory does not exist: {root_dir}")
        return repos
    
    def search_dir(path: Path, depth: int = 0):
        """Recursively search for git repos.
        
        """
        if depth > max_depth:
            return
        
        try:
            # Check if this directory is a git repo
            if is_git_repo(path):
                repos.append(path)
                # Don't search inside git repos
                return
            
            # Search subdirectories
            for item in path.iterdir():
                if item.is_dir() and not item.name.startswith('.'):
                    search_dir(item, depth + 1)
        except PermissionError:
            logger.debug(f"Permission denied: {path}")
        except Exception as e:
            logger.debug(f"Error searching {path}: {e}")
    
    search_dir(root_dir)
    return repos


def discover_bundles_from_roots(root_dirs: list[str]) -> list[BundleInfo]:
    """Discover bundles in specified root directories.
    
    Searches for git repositories and validates them as envoy bundles.
    
    Args:
        root_dirs: List of root directory paths
        
    Returns:
        List of discovered bundles
    
    """
    bundles = []
    
    for root_str in root_dirs:
        root = Path(root_str).resolve()
        logger.debug(f"Searching for bundles in: {root}")
        
        # Find all git repos under this root
        git_repos = find_git_repos(root)
        logger.debug(f"Found {len(git_repos)} git repositories in {root}")
        
        # Validate each repo as a bundle
        for repo_path in git_repos:
            if validate_bundle(repo_path):
                bundle = BundleInfo(
                    root=repo_path,
                    name=repo_path.name
                )
                bundles.append(bundle)
                logger.info(f"Discovered bundle: {bundle}")
            else:
                logger.debug(f"Git repo is not an envoy bundle: {repo_path}")
    
    return bundles


def discover_bundles_auto() -> list[BundleInfo]:
    """Auto-discover bundles using ENVOY_BNDL_ROOTS environment variable.
    
    ENVOY_BNDL_ROOTS should contain a list of root directories separated by
    the OS path separator (';' on Windows, ':' on Unix).
    
    Returns:
        List of discovered bundles
    
    """
    roots_str = os.environ.get('ENVOY_BNDL_ROOTS', '')
    
    if not roots_str:
        logger.debug("ENVOY_BNDL_ROOTS not set, no auto-discovery")
        return []
    
    # Split by OS path separator
    separator = ';' if os.name == 'nt' else ':'
    root_dirs = [r.strip() for r in roots_str.split(separator) if r.strip()]
    
    if not root_dirs:
        logger.debug("ENVOY_BNDL_ROOTS is empty")
        return []
    
    logger.info(f"Auto-discovering bundles from {len(root_dirs)} root(s)")
    return discover_bundles_from_roots(root_dirs)


def load_bundles_from_config(config_file: Path) -> list[BundleInfo]:
    """Load bundle paths from a configuration file.
    
    Config file format (JSON):
    {
        "bundles": [
            "/path/to/package1",
            "/path/to/package2"
        ]
    }
    
    or (JSON array):
    [
        "/path/to/package1",
        "/path/to/package2"
    ]
    
    Args:
        config_file: Path to configuration file
        
    Returns:
        List of bundles from config file
        
    Raises:
        WrapperError: If config file is invalid
    
    """
    if not config_file.is_file():
        raise WrapperError(f"Config file not found: {config_file}")
    
    try:
        with open(config_file, 'r') as f:
            data = json.load(f)
    except json.JSONDecodeError as e:
        raise WrapperError(f"Invalid JSON in config file: {e}")
    except Exception as e:
        raise WrapperError(f"Error reading config file: {e}")
    
    # Support both {"bundles": [...]} and direct array [...]
    if isinstance(data, dict):
        bundle_paths = data.get('bundles', [])
    elif isinstance(data, list):
        bundle_paths = data
    else:
        raise WrapperError("Config file must be a JSON object or array")
    
    bundles = []
    for path_str in bundle_paths:
        path = Path(path_str).resolve()
        
        if not validate_bundle(path):
            logger.warning(f"Invalid bundle in config: {path}")
            continue
        
        bundle = BundleInfo(
            root=path,
            name=path.name
        )
        bundles.append(bundle)
        logger.info(f"Loaded bundle from config: {bundle}")
    
    return bundles


def get_bundles(config_file: Path | None = None) -> list[BundleInfo]:
    """Get all bundles using config file or auto-discovery.
    
    If config_file is provided, only bundles from the config are used.
    Otherwise, auto-discovery is attempted using ENVOY_BNDL_ROOTS.
    
    Args:
        config_file: Optional path to config file
        
    Returns:
        List of discovered bundles
    
    """
    if config_file:
        logger.info(f"Using bundle config file: {config_file}")
        return load_bundles_from_config(config_file)
    else:
        logger.debug("No config file, attempting auto-discovery")
        return discover_bundles_auto()


def get_bundle_env_files(bundles: list[BundleInfo]) -> dict[str, list[Path]]:
    """Get all environment files from discovered bundles.
    
    Returns a mapping of bundle names to their environment JSON files.
    
    Args:
        bundles: List of bundles to scan
    
    Returns:
        Dict mapping bundle name to list of environment file paths
    
    """
    env_files = {}
    
    for bundle in bundles:
        files = []
        wrapper_env = bundle.envoy_env
        
        if wrapper_env.is_dir():
            # Find all .json files in envoy_env
            for json_file in wrapper_env.glob("*.json"):
                # Skip commands.json as it's handled separately
                if json_file.name != "commands.json":
                    files.append(json_file)
        
        if files:
            env_files[bundle.name] = files
            logger.debug(f"Bundle {bundle.name}: {len(files)} environment file(s)")
    
    return env_files


def get_bundle_commands_files(bundles: list[BundleInfo]) -> dict[str, Path]:
    """Get commands.json files from discovered bundles.
    
    Returns a mapping of bundle names to their commands.json files.
    
    Args:
        bundles: List of bundles to scan
    
    Returns:
        Dict mapping bundle name to commands.json path
    
    """
    commands_files = {}
    
    for bundle in bundles:
        commands_file = bundle.envoy_env / "commands.json"
        
        if commands_file.is_file():
            commands_files[bundle.name] = commands_file
            logger.debug(f"Bundle {bundle.name}: has commands.json")
    
    return commands_files
