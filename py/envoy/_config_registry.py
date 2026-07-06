"""Named config registry for envoy.

Manages versioned bundle-config files stored in one or more *config root*
directories.  Each named config lives in its own subdirectory and is versioned
by timestamp, with a ``latest`` text file that always points to the most
recently published version.

Directory layout under a config root::

    <cfg-root>/
    └── studio/
        ├── 2026-06-21T10-13-00.json    ← versioned config file
        ├── 2026-06-22T09-00-00.json    ← newer version
        └── latest                      ← plain text: "2026-06-22T09-00-00.json"

Usage example::

    # Publish a new version of the "studio" config
    publishConfig(
        cfg_root=Path('/studio/envoy/configs'),
        name='studio',
        source_path=Path('/tmp/my_bundles.json'),
    )

    # Resolve "studio" to the latest config file path
    path = resolveNamedConfig('studio')
    print(path)  # /studio/envoy/configs/studio/2026-06-22T09-00-00.json

    # List all available named configs
    for entry in listNamedConfigs():
        print(entry.name, entry.version, entry.path)

"""

from __future__ import annotations

import os
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

#: Environment variable containing the config root directories.
#: Semicolon-separated on Windows, colon-separated on Unix.
CFG_ROOTS_VAR: str = 'ENVOY_CFG_ROOTS'

#: Name of the file that stores the latest published config version filename.
_LATEST_FILE: str = 'latest'

#: Timestamp format used for versioned config filenames (filesystem-safe).
_TIMESTAMP_FMT: str = '%Y-%m-%dT%H-%M-%S'


# ---------------------------------------------------------------------------
# Data types
# ---------------------------------------------------------------------------


@dataclass
class NamedConfigEntry:
    """A single named config entry discovered from ``ENVOY_CFG_ROOTS``.

    Attributes:
        name: The config name (e.g. ``'studio'``).
        version: The version timestamp string (e.g. ``'2026-06-21T10-13-00'``).
        path: Absolute path to the resolved config JSON file.
        cfg_root: Absolute path to the config root directory that owns this entry.

    """

    name: str
    version: str
    path: Path
    cfg_root: Path


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _cfgRoots() -> list[Path]:
    """Return the list of config root directories from ``ENVOY_CFG_ROOTS``.

    Returns:
        List of absolute :class:`~pathlib.Path` objects (only existing directories).

    """
    roots_str = os.environ.get(CFG_ROOTS_VAR, '').strip()
    if not roots_str:
        return []
    separator = ';' if os.name == 'nt' else ':'
    return [Path(r.strip()).resolve() for r in roots_str.split(separator) if r.strip()]


def _readLatest(name_dir: Path) -> str | None:
    """Read the ``latest`` pointer for a named config directory.

    Args:
        name_dir: Directory of the named config (e.g. ``<cfg-root>/studio``).

    Returns:
        The filename stored in the ``latest`` file (e.g.
        ``'2026-06-21T10-13-00.json'``), or ``None`` if absent or unreadable.

    """
    latest_file = name_dir / _LATEST_FILE
    if not latest_file.is_file():
        return None
    try:
        return latest_file.read_text(encoding='utf-8').strip()
    except OSError:
        return None


def _writeLatest(name_dir: Path, filename: str) -> None:
    """Write *filename* to the ``latest`` pointer file.

    Args:
        name_dir: Directory of the named config.
        filename: Filename (not full path) of the new latest config file.

    """
    (name_dir / _LATEST_FILE).write_text(filename, encoding='utf-8')


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def isConfigName(value: str) -> bool:
    r"""Return ``True`` if *value* looks like a named config rather than a path.

    A value is treated as a *name* when it contains no path separator characters
    (``/``, ``\\``, ``:``) and does not start with a dot.  Everything else is
    treated as a filesystem path.

    Args:
        value: The raw string from ``bundles_config`` or ``--bundles-config``.

    Returns:
        ``True`` if *value* is a config name; ``False`` if it looks like a path.

    Examples::

        isConfigName('studio')          # True
        isConfigName('my-config')       # True
        isConfigName('/path/to/f.json') # False
        isConfigName('R:/configs.json') # False
        isConfigName('./relative.json') # False

    """
    if not value:
        return False
    return not any(c in value for c in ('/', '\\', ':')) and not value.startswith('.')


def resolveNamedConfig(name: str) -> Path | None:
    """Resolve a named config to the path of its latest version.

    Searches each directory in ``ENVOY_CFG_ROOTS`` for a subdirectory named
    *name* that contains a ``latest`` pointer file.  Returns the first match.

    Args:
        name: Config name to resolve (e.g. ``'studio'``).

    Returns:
        Absolute path to the latest config JSON file, or ``None`` if not found.

    """
    for root in _cfgRoots():
        name_dir = root / name
        if not name_dir.is_dir():
            continue
        latest_filename = _readLatest(name_dir)
        if not latest_filename:
            continue
        config_path = name_dir / latest_filename
        if config_path.is_file():
            return config_path
    return None


def listNamedConfigs() -> list[NamedConfigEntry]:
    """List all available named configs across all ``ENVOY_CFG_ROOTS`` roots.

    Scans each config root for named subdirectories that have a ``latest``
    pointer file.  Deduplicates by name — the first root that defines a given
    name wins (matching the resolution order used by :func:`resolveNamedConfig`).

    Returns:
        List of :class:`NamedConfigEntry` objects, sorted by name.

    """
    seen: set[str] = set()
    entries: list[NamedConfigEntry] = []

    for root in _cfgRoots():
        if not root.is_dir():
            continue
        for name_dir in sorted(root.iterdir()):
            if not name_dir.is_dir():
                continue
            name = name_dir.name
            if name in seen:
                continue
            latest_filename = _readLatest(name_dir)
            if not latest_filename:
                continue
            config_path = name_dir / latest_filename
            if not config_path.is_file():
                continue
            version = latest_filename.removesuffix('.json')
            entries.append(
                NamedConfigEntry(
                    name=name,
                    version=version,
                    path=config_path,
                    cfg_root=root,
                )
            )
            seen.add(name)

    return sorted(entries, key=lambda e: e.name)


def listConfigVersions(name: str) -> list[tuple[str, Path]]:
    """List all published versions of a named config in order (newest first).

    Args:
        name: Config name (e.g. ``'studio'``).

    Returns:
        List of ``(version_string, absolute_path)`` tuples, newest first.
        Returns an empty list if the name is not found in any root.

    """
    for root in _cfgRoots():
        name_dir = root / name
        if not name_dir.is_dir():
            continue
        versions = []
        for f in name_dir.iterdir():
            if f.name == _LATEST_FILE or not f.suffix == '.json':
                continue
            if f.is_file():
                versions.append((f.stem, f))
        versions.sort(key=lambda t: t[0], reverse=True)
        return [(v, p) for v, p in versions]
    return []


def publishConfig(
    cfg_root: Path,
    name: str,
    source_path: Path,
    *,
    dry_run: bool = False,
) -> Path:
    """Publish a new version of a named config.

    Copies *source_path* into ``<cfg_root>/<name>/<timestamp>.json`` and
    updates the ``<cfg_root>/<name>/latest`` pointer file.

    Args:
        cfg_root: Root directory for config storage.
        name: Config name (e.g. ``'studio'``).
        source_path: Path to the source bundles-config JSON file.
        dry_run: If ``True``, print what would happen and return without writing.

    Returns:
        The absolute path of the newly written config file.

    Raises:
        ValueError: If *source_path* does not exist or is not a file.
        OSError: If the destination directory cannot be created or written to.

    """
    if not source_path.is_file():
        raise ValueError(f"Source config file does not exist: {source_path}")

    timestamp = datetime.now(timezone.utc).strftime(_TIMESTAMP_FMT)
    filename = f"{timestamp}.json"
    name_dir = cfg_root / name
    dest_path = name_dir / filename

    if dry_run:
        print(f"Would publish: {source_path}")
        print(f"          to: {dest_path}")
        print(f"     (latest: {name_dir / _LATEST_FILE} → {filename})")
        return dest_path

    name_dir.mkdir(parents=True, exist_ok=True)
    dest_path.write_text(source_path.read_text(encoding='utf-8'), encoding='utf-8')
    _writeLatest(name_dir, filename)

    return dest_path
