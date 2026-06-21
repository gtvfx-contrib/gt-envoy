"""Bundle discovery for wrapper environments.

Supports two methods of discovering bundles:
1. Auto-discovery: Search directories specified in ENVOY_BNDL_ROOTS for git repositories
2. Config file: Explicit list of bundle paths

"""

import os
import re
import logging
from pathlib import Path
import json

from ._exceptions import WrapperError


logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Version sentinel constants
# ---------------------------------------------------------------------------

#: Version sentinel for a bundle that lives directly in a git checkout.
#: All :class:`Bundle` objects constructed from a filesystem path use this
#: version until the versioned-build system is implemented.
BUNDLE_CHECKOUT: str = 'checkout'

#: Default namespace prefix for bundles.  Matches the ``gt`` team directory
#: convention where bundles live under ``<bundle_roots>/gt/<bundle_name>``.
#: Used when constructing :attr:`Bundle.bndlid` and no explicit namespace is
#: supplied.
BUNDLE_DEFAULT_NAMESPACE: str = 'gt'

#: Name of the marker file written by ``engit publish`` / ``bundle-publish.yml``
#: at the root of every production bundle.  Serves as a discovery anchor for
#: :func:`find_bundle_roots` so that deployed (non-git) bundles are found by
#: :func:`discover_bundles_from_roots` without requiring a ``.git/`` directory.
BUNDLE_MARKER_FILE: str = '.bundle'

_NAMESPACE_RE = re.compile(r'^[A-Za-z][A-Za-z0-9_]{1,19}$')

_BNDLID_RE = re.compile(r'^([A-Za-z][A-Za-z0-9_]{1,19}):([A-Za-z][A-Za-z0-9_-]*)$')

#: Matches ``${VARNAME}`` references in bundle config path strings.
_BUNDLE_PATH_VAR_RE = re.compile(r'\$\{([A-Za-z_][A-Za-z0-9_]*)\}')


def _expandBundlePath(raw: str, config_file: Path) -> str | None:
    """Expand ``${VARNAME}`` references in a bundle path string.

    Resolves each ``${VARNAME}`` token against :data:`os.environ`.  If any
    referenced variable is undefined, a warning is logged for each missing
    variable and ``None`` is returned so the caller can skip the entry.

    Args:
        raw: Raw path string from a bundle config file, potentially containing
            ``${VARNAME}`` tokens.
        config_file: Path to the config file being processed (used in warning
            messages so the user knows where the undefined reference came from).

    Returns:
        The fully expanded string, or ``None`` if any variable was undefined.

    Example::

        os.environ['STUDIO_ROOT'] = 'R:/studio'
        result = _expandBundlePath('${STUDIO_ROOT}/envoy/0.2.1', Path('bundles.json'))
        # 'R:/studio/envoy/0.2.1'

    """
    unresolved: list[str] = []

    def _replacer(match: re.Match) -> str:
        var_name = match.group(1)
        val = os.environ.get(var_name)
        if val is None:
            unresolved.append(var_name)
            return ''
        return val

    result = _BUNDLE_PATH_VAR_RE.sub(_replacer, raw)

    if unresolved:
        for var_name in unresolved:
            logger.warning(
                "Bundle config %s: path %r references undefined variable ${%s} — skipping",
                config_file,
                raw,
                var_name,
            )
        return None

    return result


def _is_bndlid(spec: str) -> bool:
    """Return ``True`` if *spec* looks like a bundle ID (``'<ns>:<name>'``).

    Requires the namespace to be at least 2 characters so that Windows drive
    letters (``C:``, ``R:`` etc.) are never mistaken for bundle IDs.

    """
    return bool(_BNDLID_RE.match(spec))


def _resolve_bndlid(bndlid: str) -> Path:
    """Resolve a bundle ID to a filesystem path via ``ENVOY_BNDL_ROOTS``.

    Resolution strategy:

    1. **Fast path** — for each root in ``ENVOY_BNDL_ROOTS`` check
       ``<root>/<namespace>/<name>`` directly.  This is O(roots) and covers
       the standard directory convention.
    2. **Scan fallback** — if the fast path finds nothing, run a full
       :func:`discover_bundles_from_roots` scan and match by
       :attr:`~BundleInfo.bndlid`.

    Args:
        bndlid: Bundle identifier in ``'<namespace>:<name>'`` format.

    Returns:
        Absolute path to the bundle root directory.

    Raises:
        WrapperError: If ``ENVOY_BNDL_ROOTS`` is not set.
        WrapperError: If no bundle matching *bndlid* is found.

    Example::

        path = _resolve_bndlid('gt:pythoncore')
        # → Path('R:/repo/gtvfx-contrib/gt/pythoncore')

    """
    m = _BNDLID_RE.match(bndlid)
    if not m:
        raise WrapperError(f"Invalid bundle ID: {bndlid!r}")
    namespace, name = m.group(1), m.group(2)

    roots_str = os.environ.get('ENVOY_BNDL_ROOTS', '')
    if not roots_str:
        raise WrapperError(
            f"Cannot resolve bndlid {bndlid!r}: ENVOY_BNDL_ROOTS is not set"
        )
    separator = ';' if os.name == 'nt' else ':'
    roots = [Path(r.strip()) for r in roots_str.split(separator) if r.strip()]

    # 1. Fast path: <root>/<namespace>/<name>
    for root in roots:
        candidate = (root / namespace / name).resolve()
        if candidate.is_dir() and (candidate / 'envoy_env').is_dir():
            logger.debug("Resolved %s via fast path: %s", bndlid, candidate)
            return candidate

    # 2. Scan fallback
    logger.debug("Fast path missed %s, falling back to full scan", bndlid)
    infos = discover_bundles_from_roots([str(r) for r in roots])
    for info in infos:
        if info.bndlid == bndlid:
            logger.debug("Resolved %s via scan: %s", bndlid, info.root)
            return info.root

    searched = ', '.join(str(r) for r in roots)
    raise WrapperError(
        f"Bundle {bndlid!r} not found in ENVOY_BNDL_ROOTS ({searched})"
    )


def _infer_namespace(bundle_root: Path) -> str:
    """Infer a bundle namespace from its parent directory name.

    Follows the convention ``<bundle_roots>/<namespace>/<bundle_name>`` —
    i.e. the parent directory of the bundle root is treated as the namespace
    token when it looks like a valid identifier (1–20 alphanumeric/underscore
    characters starting with a letter).  If the parent name does not match
    that pattern (e.g. the bundle sits directly inside a bundle root with no
    intermediate namespace directory), :data:`BUNDLE_DEFAULT_NAMESPACE` is
    returned instead.

    Args:
        bundle_root: Absolute path to the bundle's root directory.

    Returns:
        Namespace string to use for this bundle.

    Example::

        # R:/repo/gtvfx-contrib/gt/pythoncore  →  'gt'
        # R:/repo/something_weird/pythoncore   →  'gt'  (fallback)

    """
    parent_name = bundle_root.parent.name
    if _NAMESPACE_RE.match(parent_name):
        return parent_name
    return BUNDLE_DEFAULT_NAMESPACE


class BundleInfo:
    """Information about a discovered bundle."""
    
    def __init__(self, root: Path, name: str, namespace: str = BUNDLE_DEFAULT_NAMESPACE):
        """Initialize bundle information.
        
        Args:
            root: Root directory of the bundle
            name: Name of the bundle (directory name)
            namespace: Team/namespace prefix (default: ``'gt'``)
        
        """
        self.root = root
        self.name = name
        self.namespace = namespace
        self.envoy_env = root / "envoy_env"
        self.env_files: dict[str, Path] = self._index_env_files()

    @property
    def bndlid(self) -> str:
        """Namespaced package identifier: ``'<namespace>:<name>'``.

        Mirrors :attr:`Bundle.bndlid` so that internal code working with
        :class:`BundleInfo` objects (e.g. :class:`~envoy._commands.CommandRegistry`)
        can use the same identifier without round-tripping through a full
        :class:`Bundle` object.

        """
        return f"{self.namespace}:{self.name}"

    def _index_env_files(self) -> dict[str, Path]:
        """Scan envoy_env/ once and index all JSON files by filename.
        
        Returns:
            Dict mapping filename to absolute Path
        
        """
        if not self.envoy_env.is_dir():
            return {}
        return {f.name: f for f in self.envoy_env.glob('*.json')}
        
    def __repr__(self):
        return f"BundleInfo(bndlid={self.bndlid!r}, root={self.root})"
    
    def __str__(self):
        return f"{self.name} ({self.root})"


# ---------------------------------------------------------------------------
# Public API classes
# ---------------------------------------------------------------------------

class Bundle:
    """A discovered envoy bundle.

    A bundle is a directory (or, in the future,
    a versioned built directory) that contains an ``envoy_env/`` subdirectory
    with a ``commands.json`` and one or more environment JSON files.

    **Current behaviour (checkout mode)**

    All bundles are constructed from a filesystem path that points directly to
    a live git repository on disk.  :attr:`version` always returns
    :data:`BUNDLE_CHECKOUT` and :attr:`is_production` is always ``False``.

    **Planned versioned behaviour (future)**

    Bundles will also be constructable by *name* + *version* once the
    build/publish pipeline is in place::

        # By bundle ID — resolved from ENVOY_BNDL_ROOTS (current):
        bundle = Bundle('gt:pythoncore')
        assert bundle.bndlid == 'gt:pythoncore'
        assert bundle.version == 'checkout'
        assert bundle.is_checkout is True

        # By filesystem path (current):
        bundle = Bundle('/repo/gtvfx-contrib/gt/pythoncore')
        assert bundle.bndlid == 'gt:pythoncore'    # namespace inferred from parent dir

        # Explicit namespace override:
        bundle = Bundle('/repo/gtvfx-contrib/gt/pythoncore', namespace='vfx')
        assert bundle.bndlid == 'vfx:pythoncore'

        # Production — future, resolved from the built-bundle registry:
        bundle = Bundle('gt:pythoncore', version='1.2.3')   # not yet implemented
        bundle = Bundle('gt:pythoncore', version='latest')  # not yet implemented
        assert bundle.is_production is True

    A production bundle is the result of ``git tag`` → build-to-directory
    process; the :class:`BundleConfig` file will pin these versions (see its docstring
    for the planned config format).

    Args:
        spec: Either a filesystem path to the bundle root **or** a bundle ID
            string in ``'<namespace>:<name>'`` format (e.g.
            ``'gt:pythoncore'``).  When a bundle ID is supplied the path is
            resolved from ``ENVOY_BNDL_ROOTS`` automatically.
        namespace: Team/namespace prefix for :attr:`bndlid`.  Ignored when
            *spec* is already a bundle ID (the namespace is taken from the ID).
            If ``None`` (default) and *spec* is a path, the namespace is
            inferred from the bundle's parent directory name.  Falls back to
            :data:`BUNDLE_DEFAULT_NAMESPACE` (``'gt'``) when the parent name
            is not a valid identifier token.

    Raises:
        WrapperError: If *spec* is a bundle ID and cannot be resolved via
            ``ENVOY_BNDL_ROOTS``.
        ValueError: If the resolved or supplied path does not exist or lacks
            an ``envoy_env/`` subdirectory.

    Example::

        # Resolve by bundle ID (requires ENVOY_BNDL_ROOTS):
        bundle = Bundle('gt:pythoncore')
        print(bundle.bndlid)       # 'gt:pythoncore'
        print(bundle.is_checkout)  # True

        # Construct from a filesystem path:
        bundle = Bundle('/repo/gtvfx-contrib/gt/pythoncore')
        print(bundle.name)         # 'pythoncore'
        print(bundle.namespace)    # 'gt'
        print(bundle.bndlid)       # 'gt:pythoncore'
        print(bundle.version)      # 'checkout'
        print(bundle.is_checkout)  # True
        print(bundle.commands)     # ['python_dev', ...]

    """

    def __init__(self, spec: str | Path, namespace: str | None = None) -> None:
        # Detect bndlid form: 'gt:pythoncore' — has ':' and ≥2-char namespace
        # so that Windows drive letters ('C:', 'R:') are never matched.
        if isinstance(spec, str) and _is_bndlid(spec):
            m = _BNDLID_RE.match(spec)
            inferred_ns = m.group(1)  # type: ignore[union-attr]
            root = _resolve_bndlid(spec)
            ns = inferred_ns
        else:
            root = Path(spec).resolve()
            if not root.is_dir():
                raise ValueError(f"Bundle path does not exist: {root}")
            if not (root / 'envoy_env').is_dir():
                raise ValueError(f"Not a valid bundle (no envoy_env/): {root}")
            ns = namespace if namespace is not None else _infer_namespace(root)
        self._info = BundleInfo(root=root, name=root.name, namespace=ns)

    @classmethod
    def _from_info(cls, info: 'BundleInfo') -> 'Bundle':
        """Construct from an internal :class:`BundleInfo` (no re-validation)."""
        obj = object.__new__(cls)
        obj._info = info
        return obj

    @property
    def name(self) -> str:
        """The bundle directory name."""
        return self._info.name

    @property
    def namespace(self) -> str:
        """Team/namespace prefix for this bundle.

        Auto-inferred from the parent directory name at construction time
        (e.g. ``'gt'`` for a bundle under ``<roots>/gt/<name>``).
        Can be overridden by passing ``namespace=`` to :class:`Bundle`.
        Defaults to :data:`BUNDLE_DEFAULT_NAMESPACE` when the parent name
        is not a recognised identifier.

        """
        return self._info.namespace

    @property
    def bndlid(self) -> str:
        """Namespaced package identifier: ``'<namespace>:<name>'``.

        Uniquely identifies a bundle
        within a team's registry and allows bundles from multiple teams to
        coexist without name collisions.

        Currently used for display and logging; will become the primary lookup
        key once the versioned-build registry is implemented.

        Examples: ``'gt:pythoncore'``, ``'gt:globals'``, ``'vfx:maya'``.

        """
        return f"{self._info.namespace}:{self._info.name}"

    @property
    def version(self) -> str:
        """Version string for this bundle.

        Returns the ``version`` field from the ``.bundle`` marker file when
        the bundle is a published production release.  Otherwise returns
        :data:`BUNDLE_CHECKOUT` (``'checkout'``), indicating a live git checkout.

        """
        marker = self._info.root / BUNDLE_MARKER_FILE
        if marker.is_file():
            try:
                data = json.loads(marker.read_text(encoding='utf-8'))
                ver = data.get('version')
                if ver:
                    return str(ver)
            except (OSError, json.JSONDecodeError, ValueError):
                pass
        return BUNDLE_CHECKOUT

    @property
    def is_production(self) -> bool:
        """``True`` if this bundle is a built, versioned release directory.

        Returns ``True`` when a ``.bundle`` marker file is present at the
        bundle root — indicating the bundle was created by ``engit publish``
        or ``bundle-publish.yml`` rather than a live git checkout.

        """
        return (self._info.root / BUNDLE_MARKER_FILE).is_file()

    @property
    def is_checkout(self) -> bool:
        """``True`` if this bundle is a live git-repository checkout.

        This is the inverse of :attr:`is_production`.  ``True`` when no
        ``.bundle`` marker file is present at the bundle root.

        """
        return not self.is_production

    @property
    def path(self) -> Path:
        """Absolute path to the bundle root directory."""
        return self._info.root

    @property
    def envoy_env(self) -> Path:
        """Absolute path to the ``envoy_env/`` subdirectory."""
        return self._info.envoy_env

    @property
    def env_files(self) -> dict[str, Path]:
        """Mapping of JSON filename → absolute path for all env files."""
        return dict(self._info.env_files)

    @property
    def commands(self) -> list[str]:
        """Sorted list of command names defined in this bundle's ``commands.json``.

        Returns an empty list if the file is absent or cannot be parsed.

        """
        commands_file = self._info.envoy_env / 'commands.json'
        if not commands_file.exists():
            return []
        try:
            with commands_file.open() as fh:
                data = json.load(fh)
            return sorted(data.keys()) if isinstance(data, dict) else []
        except (json.JSONDecodeError, OSError):
            return []

    def __repr__(self) -> str:
        return f"Bundle(bndlid={self.bndlid!r}, path={self.path})"

    def __str__(self) -> str:
        return repr(self)


class BundleConfig:
    """An envoy bundle configuration file.

    A bundle config is a JSON file that declares which bundles envoy should use
    — the file passed to the CLI via ``--bundles-config``/``-bc``.

    **Current format** — flat list of filesystem paths (all checkout mode)::

        ["R:/repo/gtvfx-contrib/gt/pythoncore",
         "R:/repo/gtvfx-contrib/gt/globals"]

        or {"bundles": ["...", "..."]}

    **Planned versioned format** (future — once the build/publish pipeline
    exists)::

        {
            "bundles": {
                "pythoncore": "1.2.3",
                "globals": "latest",
                "my_local_tool": "checkout:/path/to/local"
            }
        }

    In the versioned model each bundle entry resolves to a built output
    directory tagged with the requested version.  The ``checkout:`` prefix
    will preserve the current path-based behaviour for in-development bundles.

    Instances are usually created via the constructor (from a path) or via one
    of the factory classmethods :meth:`from_name` and :meth:`current`.

    Args:
        path: Path to the bundle config JSON file.

    Raises:
        ValueError: If *path* does not exist.

    Examples::

        # Load from an explicit path
        cfg = BundleConfig('/studio/envoy_bundles.json')
        for bundle in cfg.bundles:
            print(bundle.name, bundle.version, bundle.is_checkout)
        print(cfg.commands)   # merged command list across all bundles

        # Load from a named config slot (resolved via ENVOY_CFG_ROOTS)
        cfg = BundleConfig.from_name('studio')
        print(cfg.name)         # 'studio'
        print(cfg.cfg_version)  # '2026-06-21T10-13-00'

        # Load whatever the user has configured
        cfg = BundleConfig.current()
        if cfg is not None:
            print(cfg.commands)

    """

    def __init__(self, path: str | Path) -> None:
        p = Path(path).resolve()
        if not p.is_file():
            raise ValueError(f"BundleConfig path does not exist: {p}")
        self._path = p
        self._bundles: list[Bundle] | None = None
        self._name: str | None = None
        self._cfg_version: str | None = None

    # ------------------------------------------------------------------
    # Internal factory (preserves name/version metadata)
    # ------------------------------------------------------------------

    @classmethod
    def _from_named(cls, path: Path, name: str, version: str) -> 'BundleConfig':
        """Construct a BundleConfig already resolved from a named config slot.

        Args:
            path: Absolute path to the resolved config JSON file.
            name: The config slot name (e.g. ``'studio'``).
            version: The version timestamp string.

        Returns:
            A fully initialised :class:`BundleConfig` with name/version set.

        """
        obj = cls.__new__(cls)
        obj._path = path
        obj._bundles = None
        obj._name = name
        obj._cfg_version = version
        return obj

    # ------------------------------------------------------------------
    # Factory classmethods
    # ------------------------------------------------------------------

    @classmethod
    def from_name(cls, name: str) -> 'BundleConfig':
        """Load a bundle config by named config slot.

        Resolves *name* to the latest published version via
        ``ENVOY_CFG_ROOTS``.  The first matching root wins (same precedence
        as the CLI ``--bundles-config`` flag).

        Args:
            name: Config slot name (e.g. ``'studio'``, ``'production'``).

        Returns:
            :class:`BundleConfig` instance with :attr:`name` and
            :attr:`cfg_version` populated.

        Raises:
            ValueError: If *name* cannot be resolved via ``ENVOY_CFG_ROOTS``
                (slot not found or ``ENVOY_CFG_ROOTS`` is not set).

        Example::

            cfg = BundleConfig.from_name('studio')
            print(cfg.name)         # 'studio'
            print(cfg.cfg_version)  # '2026-06-21T10-13-00'
            print(cfg.path)         # /studio/envoy/configs/studio/2026-...json
            print(cfg.commands)

        """
        from ._config_registry import resolveNamedConfig
        resolved = resolveNamedConfig(name)
        if resolved is None:
            raise ValueError(
                f"Named config {name!r} not found in ENVOY_CFG_ROOTS. "
                "Check that ENVOY_CFG_ROOTS is set and the config has been published."
            )
        version = resolved.stem
        return cls._from_named(resolved, name=name, version=version)

    @classmethod
    def current(
        cls,
        *,
        ignore_user_config: bool = False,
    ) -> 'BundleConfig | None':
        """Return the active bundle config as configured by the user.

        Reads the ``bundles_config`` setting from the persistent user config
        file and resolves it to a :class:`BundleConfig` instance.  Returns
        ``None`` when no ``bundles_config`` is set.

        Resolution logic:

        1. If *ignore_user_config* is ``True``, return ``None`` immediately.
        2. Load the user config from disk.
        3. Read the ``bundles_config`` setting.
        4. If the value looks like a named config slot (no path separators),
           resolve it via :func:`~._config_registry.resolveNamedConfig`.
        5. Otherwise, treat the value as a filesystem path.
        6. Return ``None`` if no setting is present.

        Args:
            ignore_user_config: When ``True``, bypass the user config entirely
                and return ``None``.  Mirrors the ``--ignore-config`` CLI flag.

        Returns:
            The resolved :class:`BundleConfig`, or ``None`` if no
            ``bundles_config`` is configured.

        Raises:
            ValueError: If the configured value resolves to a file that does
                not exist, or a named config slot that cannot be found.

        Example::

            cfg = BundleConfig.current()
            if cfg is not None:
                for bundle in cfg.bundles:
                    print(bundle.bndlid)
            else:
                print("No bundle config set — using auto-discovery")

        """
        if ignore_user_config:
            return None

        from ._user_config import UserConfig
        from ._config_registry import isConfigName, resolveNamedConfig

        user_cfg = UserConfig.load()
        raw = user_cfg.get('bundles_config')
        if not raw:
            return None

        if isConfigName(raw):
            resolved = resolveNamedConfig(raw)
            if resolved is None:
                raise ValueError(
                    f"Named config {raw!r} (from user config) not found in "
                    "ENVOY_CFG_ROOTS."
                )
            version = resolved.stem
            return cls._from_named(resolved, name=raw, version=version)

        return cls(raw)

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def path(self) -> Path:
        """Absolute path to the config file."""
        return self._path

    @property
    def name(self) -> str | None:
        """Named config slot this config was loaded from, or ``None``.

        Populated when the instance was created via :meth:`from_name` or
        :meth:`current` (when the user config holds a slot name rather than a
        path).  ``None`` for instances created directly from a path.

        """
        return self._name

    @property
    def cfg_version(self) -> str | None:
        """Version timestamp string if loaded from a named config slot, else ``None``.

        Format matches the filenames written by ``engit publish-config``:
        ``'2026-06-21T10-13-00'``.  ``None`` for instances created directly
        from a path.

        """
        return self._cfg_version

    @property
    def bundles(self) -> list[Bundle]:
        """List of :class:`Bundle` objects declared in this config.

        Loaded and cached on first access.

        """
        if self._bundles is None:
            infos = load_bundles_from_config(self._path)
            self._bundles = [Bundle._from_info(info) for info in infos]
        return self._bundles

    @property
    def commands(self) -> list[str]:
        """Sorted list of all command names across all bundles (deduplicated)."""
        seen: set[str] = set()
        for bundle in self.bundles:
            seen.update(bundle.commands)
        return sorted(seen)

    def __repr__(self) -> str:
        if self._name:
            return f"BundleConfig(name={self._name!r}, path={self._path})"
        return f"BundleConfig(path={self._path})"

    def __str__(self) -> str:
        return repr(self)


def is_git_repo(path: Path) -> bool:
    """Check if a directory is a git repository.

    Args:
        path: Path to check.

    Returns:
        True if path contains a .git directory.

    """
    return (path / ".git").is_dir()


def is_published_bundle(path: Path) -> bool:
    """Check if a directory is a published bundle (has a ``.bundle`` marker).

    Args:
        path: Path to check.

    Returns:
        True if path contains a ``.bundle`` marker file.

    """
    return (path / BUNDLE_MARKER_FILE).is_file()


def has_envoy_env(path: Path) -> bool:
    """Check if a directory has an envoy_env subdirectory.

    Args:
        path: Path to check.

    Returns:
        True if path contains an envoy_env directory.

    """
    return (path / "envoy_env").is_dir()


def validate_bundle(path: Path) -> bool:
    """Validate that a path is a valid envoy bundle.

    A valid bundle must be a directory with an ``envoy_env/`` subdirectory.

    Args:
        path: Path to validate.

    Returns:
        True if path is a valid bundle.

    """
    return path.is_dir() and has_envoy_env(path)


def find_bundle_roots(root_dir: Path, max_depth: int = 5) -> list[Path]:
    """Recursively find bundle roots under a root directory.

    A bundle root is any directory that contains either a ``.git/`` directory
    (live checkout) or a ``.bundle`` marker file (published production bundle).

    Args:
        root_dir: Root directory to search.
        max_depth: Maximum directory depth to search.

    Returns:
        List of paths to bundle root directories.

    """
    bundle_roots: list[Path] = []

    if not root_dir.is_dir():
        logger.warning("Root directory does not exist: %s", root_dir)
        return bundle_roots

    def search_dir(path: Path, depth: int = 0) -> None:
        if depth > max_depth:
            return
        try:
            if is_git_repo(path) or is_published_bundle(path):
                bundle_roots.append(path)
                return
            for item in path.iterdir():
                if item.is_dir() and not item.name.startswith('.'):
                    search_dir(item, depth + 1)
        except PermissionError:
            logger.debug("Permission denied: %s", path)
        except Exception as exc:
            logger.debug("Error searching %s: %s", path, exc)

    search_dir(root_dir)
    return bundle_roots


def find_git_repos(root_dir: Path, max_depth: int = 5) -> list[Path]:
    """Recursively find git repositories under a root directory.

    .. deprecated::
        Use :func:`find_bundle_roots` instead — it also detects published
        bundles that have a ``.bundle`` marker but no ``.git/`` directory.

    Args:
        root_dir: Root directory to search.
        max_depth: Maximum depth to search.

    Returns:
        List of paths to git repository roots.

    """
    repos: list[Path] = []

    if not root_dir.is_dir():
        logger.warning("Root directory does not exist: %s", root_dir)
        return repos

    def search_dir(path: Path, depth: int = 0) -> None:
        if depth > max_depth:
            return
        try:
            if is_git_repo(path):
                repos.append(path)
                return
            for item in path.iterdir():
                if item.is_dir() and not item.name.startswith('.'):
                    search_dir(item, depth + 1)
        except PermissionError:
            logger.debug("Permission denied: %s", path)
        except Exception as exc:
            logger.debug("Error searching %s: %s", path, exc)

    search_dir(root_dir)
    return repos


def discover_bundles_from_roots(root_dirs: list[str]) -> list[BundleInfo]:
    """Discover bundles in specified root directories.

    Searches for git repositories and published bundles (with a ``.bundle``
    marker file) and validates them as envoy bundles.

    Args:
        root_dirs: List of root directory paths.

    Returns:
        List of discovered bundles.

    """
    bundles = []

    for root_str in root_dirs:
        root = Path(root_str).resolve()
        logger.debug("Searching for bundles in: %s", root)

        candidates = find_bundle_roots(root)
        logger.debug("Found %d bundle candidate(s) in %s", len(candidates), root)

        for candidate_path in candidates:
            if validate_bundle(candidate_path):
                name, namespace = _nameAndNamespace(candidate_path)
                bundle = BundleInfo(
                    root=candidate_path,
                    name=name,
                    namespace=namespace,
                )
                bundles.append(bundle)
                logger.info("Discovered bundle: %s", bundle)
            else:
                logger.debug("Candidate is not an envoy bundle: %s", candidate_path)

    return bundles


def _nameAndNamespace(bundle_root: Path) -> tuple[str, str]:
    """Return ``(name, namespace)`` for a bundle root directory.

    For published bundles (have a ``.bundle`` marker), reads ``bndlid`` from the
    marker file so that the correct name and namespace are used even when the
    directory name is a version string (e.g. ``v1.0.0``).

    For checkout bundles (have ``.git/`` but no ``.bundle``), falls back to the
    directory name and :func:`_infer_namespace`.

    Args:
        bundle_root: Absolute path to the bundle root directory.

    Returns:
        Tuple of ``(name, namespace)`` strings.

    """
    marker = bundle_root / BUNDLE_MARKER_FILE
    if marker.is_file():
        try:
            data = json.loads(marker.read_text(encoding='utf-8'))
            bndlid = data.get('bndlid', '')
            if bndlid and ':' in bndlid:
                namespace, name = bndlid.split(':', 1)
                return name, namespace
            name = data.get('name') or bundle_root.name
            return name, _infer_namespace(bundle_root)
        except (OSError, json.JSONDecodeError, ValueError):
            pass
    return bundle_root.name, _infer_namespace(bundle_root)


def discover_bundles_auto() -> list[BundleInfo]:
    """Auto-discover bundles using ENVOY_BNDL_ROOTS environment variable.

    When ``ENVOY_BUNDLES_CONFIG`` is set (and the file exists), that pre-built
    bundles-config file is loaded directly, skipping the git-repo scan
    entirely.  This is the fast path used when VS Code is launched via the
    ``gt.vscode.wrapper``, which writes the file before spawning ``code``.

    Falls back to ``ENVOY_BNDL_ROOTS`` discovery when ``ENVOY_BUNDLES_CONFIG``
    is absent or points to a missing file.

    ENVOY_BNDL_ROOTS should contain a list of root directories separated by
    the OS path separator (';' on Windows, ':' on Unix).

    Returns:
        List of discovered bundles

    """
    # Fast path — use a pre-built bundles-config file when available.
    bundles_config_str = os.environ.get('ENVOY_BUNDLES_CONFIG', '').strip()
    if bundles_config_str:
        config_path = Path(bundles_config_str)
        if config_path.is_file():
            logger.debug(
                "Using pre-built bundle list from ENVOY_BUNDLES_CONFIG: %s",
                config_path,
            )
            return load_bundles_from_config(config_path)
        else:
            logger.warning(
                "ENVOY_BUNDLES_CONFIG is set but file not found: %s — "
                "falling back to ENVOY_BNDL_ROOTS discovery",
                config_path,
            )

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
    
    logger.info("Auto-discovering bundles from %d root(s)", len(root_dirs))
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
        if not isinstance(path_str, str):
            logger.warning("Bundle config %s: non-string entry %r — skipping", config_file, path_str)
            continue

        expanded = _expandBundlePath(path_str, config_file)
        if expanded is None:
            continue

        path = Path(expanded).resolve()

        if not validate_bundle(path):
            logger.warning(f"Invalid bundle in config: {path}")
            continue
        
        bundle = BundleInfo(
            root=path,
            name=path.name,
            namespace=_infer_namespace(path),
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
