"""envoy._api -- Top-level public API convenience functions and constants.

This module houses logic that is exposed at the ``envoy`` package level
but is kept out of ``__init__`` to separate interface aggregation from
implementation.
"""

from __future__ import annotations

import logging
import platform
from pathlib import Path
from typing import TYPE_CHECKING

from ._environment import _CORE_ENV_VARS, _ENVOY_ENV_VARS
from ._user_config import UserConfig


if TYPE_CHECKING:
    from ._discovery import BundleConfig

# ---------------------------------------------------------------------------
# Public constants
# ---------------------------------------------------------------------------

#: The current operating system name as returned by :func:`platform.system`
#: (e.g. ``'Windows'``, ``'Linux'``, ``'Darwin'``).
OPERATING_SYSTEM: str = platform.system()

#: Operating systems that envoy officially supports.
SUPPORTED_OPERATING_SYSTEMS: tuple[str, ...] = ('Windows', 'Linux', 'Darwin')


# ---------------------------------------------------------------------------
# Top-level API functions
# ---------------------------------------------------------------------------

def getEnvironment(
    command: str,
    *,
    inherit_env: bool = False,
    allowlist: list[str] | None = None,
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
) -> dict[str, str]:
    """Build and return the subprocess environment dict for *command*.

    This is a convenience wrapper around :class:`~.proc.Environment` that
    constructs and returns the environment dictionary without launching any
    process.  Useful for debugging, inspection, or passing to other tools.

    Args:
        command: The envoy command name (e.g. ``'maya'``).
        inherit_env: When ``True`` the returned dict is based on the full
            current process environment.  When ``False`` (default) only env
            file variables and the built-in OS seed vars are included.
        allowlist: Additional system variable names to include in closed mode.
        bundle_roots: Override bundle discovery roots.
        commands_file: Explicit ``commands.json`` path.

    Returns:
        The fully expanded subprocess environment dictionary.

    Raises:
        ~.CommandNotFoundError: If *command* is not registered.
        ~.EnvironmentBuildError: If environment preparation fails.

    Example::

        env = envoy.getEnvironment('maya')
        print(env.get('MAYA_VERSION'))

    """
    from .proc import Environment
    return Environment(
        command,
        inherit_env=inherit_env,
        allowlist=allowlist,
        bundle_roots=bundle_roots,
        commands_file=commands_file,
    ).build()


def getAllowlist(extra: list[str] | None = None) -> frozenset[str]:
    """Return the default set of system variable names that envoy seeds in
    closed mode.

    This is the union of :data:`~._environment._CORE_ENV_VARS` (identity,
    temp, system paths, locale) and :data:`~._environment._ENVOY_ENV_VARS`
    (``ENVOY_BNDL_ROOTS``, ``ENVOY_ALLOWLIST``).

    Args:
        extra: Additional variable names to include in the returned set.

    Returns:
        The combined allowlist as a :class:`frozenset`.

    Example::

        # See what's always seeded on this platform
        for var in sorted(envoy.getAllowlist()):
            print(var)

    """
    base = _CORE_ENV_VARS | _ENVOY_ENV_VARS
    if extra:
        return base | frozenset(extra)
    return base


def traceEnvironment(
    command: str,
    var: str,
    *,
    inherit_env: bool = False,
    allowlist: list[str] | None = None,
    bundle_roots: list[str] | None = None,
    commands_file: Path | None = None,
) -> tuple[dict[str, str], list]:
    """Build the environment for *command* and return a trace of how *var* mutated.

    This is the programmatic equivalent of ``envoy --trace VAR command``.  It
    performs all the same environment-file processing without launching a
    process, returning both the final environment dict and a list of trace
    events that describe each mutation step.

    Args:
        command: The envoy command name (e.g. ``'unreal'``).
        var: Name of the environment variable to trace (e.g. ``'UE_PYTHONPATH'``).
        inherit_env: When ``True`` the dict is based on the full current process
            environment.  When ``False`` (default) only env file variables and
            the built-in OS seed vars are included.
        allowlist: Additional system variable names to include in closed mode.
        bundle_roots: Override bundle discovery roots.
        commands_file: Explicit ``commands.json`` path.

    Returns:
        A ``(env_dict, trace_events)`` tuple.  *trace_events* is a list of
        :class:`~._environment.TraceAllowlistEvent` and
        :class:`~._environment.TraceStepEvent` instances in processing order.

    Raises:
        ~.CommandNotFoundError: If *command* is not registered.
        ~.EnvironmentBuildError: If environment preparation fails.

    Example::

        env, events = envoy.traceEnvironment('unreal', 'UE_PYTHONPATH')
        for ev in events:
            print(ev)

    """
    from .proc import _loadRegistry, _collectEnvFiles
    from ._environment import EnvironmentManager

    registry, bundles = _loadRegistry(
        bundle_roots=bundle_roots,
        commands_file=commands_file,
    )
    env_files = _collectEnvFiles(command, registry, bundles)

    trace_events: list = []
    env_mgr = EnvironmentManager(
        inherit_env=inherit_env,
        allowlist=set(allowlist) if allowlist else None,
    )
    final_env = env_mgr.prepareEnvironment(
        env_files=env_files,
        trace_var=var,
        trace_out=trace_events,
    )
    return final_env, trace_events


def setApiVerbosity(level: int | str) -> None:
    """Set the logging verbosity for the ``envoy`` logger.

    Pass a :mod:`logging` level
    constant (``logging.DEBUG``, ``logging.INFO``, etc.) or its string
    equivalent (``'DEBUG'``, ``'INFO'``, etc.).

    Args:
        level: New log level for the ``envoy`` logger tree.

    Example::

        import logging
        import envoy as envoy

        envoy.setApiVerbosity(logging.DEBUG)

    """
    logging.getLogger('envoy').setLevel(level)


def loadUserConfig(path: Path | None = None) -> UserConfig:
    """Load the persistent user config from disk.

    Convenience wrapper around :meth:`~._user_config.UserConfig.load`.
    Returns an empty (default) config when the file does not exist or cannot
    be parsed — never raises on a missing or corrupt file.

    Args:
        path: Override the config file path.  Defaults to
            :data:`~._user_config.USER_CONFIG_PATH`.

    Returns:
        Loaded :class:`~._user_config.UserConfig` instance.

    Example::

        cfg = envoy.loadUserConfig()
        print(cfg.get('bundles_config'))  # None if not set

        cfg.set('bundles_config', 'studio')
        cfg.save()

    """
    return UserConfig.load(path=path)


def getCurrentBundleConfig(
    *,
    ignore_user_config: bool = False,
) -> BundleConfig | None:
    """Return the active bundle config as configured by the user.

    Convenience wrapper around :meth:`~._discovery.BundleConfig.current`.
    Reads the ``bundles_config`` setting from the persistent user config and
    resolves it to a :class:`~._discovery.BundleConfig` instance.  Returns
    ``None`` when no ``bundles_config`` is set.

    Args:
        ignore_user_config: When ``True``, bypass the user config entirely and
            return ``None``.  Mirrors the ``--ignore-config`` CLI flag.

    Returns:
        The resolved :class:`~._discovery.BundleConfig`, or ``None`` if no
        ``bundles_config`` is configured.

    Raises:
        ValueError: If the configured value resolves to a file that does not
            exist, or a named config slot that cannot be found.

    Example::

        cfg = envoy.getCurrentBundleConfig()
        if cfg is not None:
            print(cfg.commands)
        else:
            print("No bundle config set — using auto-discovery")

    """
    from ._discovery import BundleConfig
    return BundleConfig.current(ignore_user_config=ignore_user_config)
