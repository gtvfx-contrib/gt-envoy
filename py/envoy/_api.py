"""envoy._api -- Top-level public API convenience functions and constants.

This module houses logic that is exposed at the ``envoy`` package level
but is kept out of ``__init__`` to separate interface aggregation from
implementation.
"""

from __future__ import annotations

import logging
import platform
from pathlib import Path

from ._environment import _CORE_ENV_VARS, _ENVOY_ENV_VARS


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

def get_environment(
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

        env = envoy.get_environment('maya')
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


def get_allowlist(extra: list[str] | None = None) -> frozenset[str]:
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
        for var in sorted(envoy.get_allowlist()):
            print(var)

    """
    base = _CORE_ENV_VARS | _ENVOY_ENV_VARS
    if extra:
        return base | frozenset(extra)
    return base


def set_api_verbosity(level: int | str) -> None:
    """Set the logging verbosity for the ``envoy`` logger.

    Pass a :mod:`logging` level
    constant (``logging.DEBUG``, ``logging.INFO``, etc.) or its string
    equivalent (``'DEBUG'``, ``'INFO'``, etc.).

    Args:
        level: New log level for the ``envoy`` logger tree.

    Example::

        import logging
        import envoy as envoy

        envoy.set_api_verbosity(logging.DEBUG)

    """
    logging.getLogger('envoy').setLevel(level)
