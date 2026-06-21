"""Persistent user configuration for envoy.

Stores per-user preferences in a platform-appropriate JSON file so that
flags and paths do not need to be repeated on every invocation.

Config file locations:

- **Windows**: ``%APPDATA%\\envoy\\user_config.json``
- **macOS/Linux**: ``~/.config/envoy/user_config.json``

Settings are stored as a flat JSON object.  Use :meth:`UserConfig.load`
to read the config and :meth:`UserConfig.save` to persist changes.

Example::

    cfg = UserConfig.load()
    cfg.set('bundles_config', '/studio/envoy/studio_bundles.json')
    cfg.save()
    print(cfg.get('bundles_config'))

"""

from __future__ import annotations

import json
import os
import platform
from pathlib import Path
from typing import Any


# ---------------------------------------------------------------------------
# Platform-appropriate default path
# ---------------------------------------------------------------------------

def _defaultConfigPath() -> Path:
    """Return the platform-appropriate user config file path.

    Returns:
        Path to the user config JSON file.

    """
    if platform.system() == 'Windows':
        base = Path(os.environ.get('APPDATA', Path.home()))
    else:
        xdg = os.environ.get('XDG_CONFIG_HOME', '')
        base = Path(xdg) if xdg else Path.home() / '.config'
    return base / 'envoy' / 'user_config.json'


#: Absolute path to the user config file.  Can be overridden for testing via
#: the ``ENVOY_USER_CONFIG`` environment variable.
USER_CONFIG_PATH: Path = Path(
    os.environ.get('ENVOY_USER_CONFIG', '')
) or _defaultConfigPath()


# ---------------------------------------------------------------------------
# Known settings registry
# ---------------------------------------------------------------------------

#: Registry of all settings that can be stored in the user config.
#: Each entry maps a setting key to a dict with ``description`` and optional
#: ``choices`` (list of valid string values, or ``None`` for free-form).
KNOWN_SETTINGS: dict[str, dict[str, Any]] = {
    'bundles_config': {
        'description': (
            'Path to the default bundles config JSON file.  '
            'Used when --bundles-config is not supplied on the command line.'
        ),
        'choices': None,
    },
    'verbosity': {
        'description': 'Default verbosity level for all envoy invocations.',
        'choices': ['quiet', 'normal', 'verbose'],
    },
}


# ---------------------------------------------------------------------------
# UserConfig class
# ---------------------------------------------------------------------------

class UserConfig:
    """Persistent user configuration for envoy.

    Loaded from and saved to :data:`USER_CONFIG_PATH` (or the path specified
    by the ``ENVOY_USER_CONFIG`` environment variable).

    Attributes:
        path: Absolute path to the config file this instance was loaded from.

    Example::

        cfg = UserConfig.load()
        cfg.set('bundles_config', '/studio/envoy/bundles.json')
        cfg.save()
        print(cfg.get('bundles_config'))

    """

    def __init__(self, data: dict[str, str], path: Path | None = None) -> None:
        """Initialise a UserConfig instance.

        Args:
            data: Flat dict of setting key → value.
            path: Path to the config file.  Defaults to
                :data:`USER_CONFIG_PATH`.

        """
        self._data: dict[str, str] = dict(data)
        self.path: Path = path if path is not None else USER_CONFIG_PATH

    # ------------------------------------------------------------------
    # Factory
    # ------------------------------------------------------------------

    @classmethod
    def load(cls, path: Path | None = None) -> 'UserConfig':
        """Load the user config from disk.

        Returns an empty (default) config if the file does not exist or
        cannot be parsed — never raises on a missing/corrupt file.

        Args:
            path: Override the config file path.  Defaults to
                :data:`USER_CONFIG_PATH`.

        Returns:
            Loaded :class:`UserConfig` instance.

        """
        config_path = path if path is not None else USER_CONFIG_PATH
        if config_path.is_file():
            try:
                raw = json.loads(config_path.read_text(encoding='utf-8'))
                if isinstance(raw, dict):
                    return cls({str(k): str(v) for k, v in raw.items()}, path=config_path)
            except (OSError, json.JSONDecodeError, ValueError):
                pass
        return cls({}, path=config_path)

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def save(self) -> None:
        """Save the current config to disk.

        Creates parent directories as needed.

        Raises:
            OSError: If the file cannot be written.

        """
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(
            json.dumps(self._data, indent=2, sort_keys=True),
            encoding='utf-8',
        )

    # ------------------------------------------------------------------
    # Settings access
    # ------------------------------------------------------------------

    def get(self, key: str) -> str | None:
        """Return the value of *key*, or ``None`` if not set.

        Args:
            key: Setting name.

        Returns:
            The stored string value, or ``None``.

        """
        return self._data.get(key)

    def set(self, key: str, value: str) -> None:
        """Set *key* to *value* in memory (call :meth:`save` to persist).

        Args:
            key: Setting name.  Must be a key in :data:`KNOWN_SETTINGS`.
            value: Value to store.

        Raises:
            ValueError: If *key* is not a known setting.
            ValueError: If *value* is not in the allowed choices for *key*.

        """
        if key not in KNOWN_SETTINGS:
            known = ', '.join(sorted(KNOWN_SETTINGS))
            raise ValueError(
                f"Unknown config setting {key!r}. Known settings: {known}"
            )
        choices = KNOWN_SETTINGS[key].get('choices')
        if choices is not None and value not in choices:
            raise ValueError(
                f"Invalid value {value!r} for {key!r}. "
                f"Valid choices: {', '.join(choices)}"
            )
        self._data[key] = value

    def unset(self, key: str) -> bool:
        """Remove *key* from the config.

        Args:
            key: Setting name to remove.

        Returns:
            ``True`` if the key existed and was removed, ``False`` otherwise.

        """
        if key in self._data:
            del self._data[key]
            return True
        return False

    def items(self) -> dict[str, str]:
        """Return all currently stored settings as a dict.

        Returns:
            Copy of the internal settings dict.

        """
        return dict(self._data)

    def __bool__(self) -> bool:
        return bool(self._data)

    def __repr__(self) -> str:
        return f"UserConfig(path={self.path}, settings={self._data})"

    def __str__(self) -> str:
        return repr(self)
