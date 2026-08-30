"""envoy -- Environment orchestration for managed application execution.

This mixed-layout maturin package re-exports symbols from the compiled
``envoy._envoy`` extension module while preserving the existing
``import envoy`` and ``import envoy.proc`` entry points expected by
downstream consumers.
"""

from __future__ import annotations

import asyncio as _asyncio
import importlib
import os as _os
import runpy as _runpy
import sys as _sys
from pathlib import Path as _Path

from . import testing

_native = importlib.import_module("._envoy", __name__)

_core_version = _native._core_version
#: Git-tag-derived version string (e.g. ``'v1.2.3'`` or ``'v1.2.3-4-gabc123'``
#: for commits after the last tag), computed at build time via
#: ``rust/envoy-py/build.rs``. Mirrors ``py/envoy/__init__.py``'s
#: ``hatch-vcs``-derived ``__version__``.
__version__ = _native._git_version()
OPERATING_SYSTEM = _native.OPERATING_SYSTEM
SUPPORTED_OPERATING_SYSTEMS = _native.SUPPORTED_OPERATING_SYSTEMS
TraceAllowlistEvent = _native.TraceAllowlistEvent
TraceStepEvent = _native.TraceStepEvent
UserConfig = _native.UserConfig
# Wrapper bindings.
ExecutionResult = _native.ExecutionResult
WrapperConfig = _native.WrapperConfig
ApplicationWrapper = _native.ApplicationWrapper
createWrapper = _native.createWrapper
# Bundle discovery bindings.
BUNDLE_CHECKOUT = _native.BUNDLE_CHECKOUT
BUNDLE_DEFAULT_NAMESPACE = _native.BUNDLE_DEFAULT_NAMESPACE
Bundle = _native.Bundle
BundleInfo = _native.BundleInfo
Stack = _native.Stack
discoverBundlesAuto = _native.discoverBundlesAuto
getBundles = _native.getBundles
loadBundlesFromStack = _native.loadBundlesFromStack

# Bundle cache bindings.
BundleCache = _native.BundleCache

# Team configuration bindings.
TeamConfig = _native.TeamConfig
UserHostConfig = _native.UserHostConfig
Vcs = _native.Vcs
VcsChange = _native.VcsChange
VcsStatus = _native.VcsStatus

# SemVer bindings.
SemVer = _native.SemVer
Constraint = _native.Constraint
VersionSpec = _native.VersionSpec
# Command bindings.
CommandDefinition = _native.CommandDefinition
CommandRegistry = _native.CommandRegistry
Command = _native.Command
findCommandsFile = _native.findCommandsFile

# Named-stack registry and user-config metadata bindings.
NamedStackEntry = _native.NamedStackEntry
STACK_ROOTS_VAR = _native.STACK_ROOTS_VAR
USER_CONFIG_PATH = _native.USER_CONFIG_PATH
KNOWN_SETTINGS = _native.KNOWN_SETTINGS
getConfigRoot = _native.getConfigRoot
isStackName = _native.isStackName
resolveNamedStack = _native.resolveNamedStack
listNamedStacks = _native.listNamedStacks
listStackVersions = _native.listStackVersions

getEnvironment = _native.getEnvironment
getAllowlist = _native.getAllowlist
traceEnvironment = _native.traceEnvironment
diagnoseEnvironment = _native.diagnoseEnvironment
setApiVerbosity = _native.setApiVerbosity
loadUserConfig = _native.loadUserConfig
getCurrentStack = _native.getCurrentStack
getCurrentTeamConfig = _native.getCurrentTeamConfig
enable_telemetry = _native.enable_telemetry
disable_telemetry = _native.disable_telemetry
is_telemetry_enabled = _native.is_telemetry_enabled
cli_main = _native.cli_main
proc = _native.proc
exceptions = _native.exceptions
telemetry = _native.telemetry
# New top-level Environment class (dict-like, auto-initializing).
Environment = _native.Environment


async def async_new_environment(command: str, **kwargs: object) -> Environment:
    """Construct an :class:`Environment` without blocking the event loop.

    ``Environment`` construction is lazy (the subprocess environment is
    only built on first attribute access -- see ``Environment``'s own
    docstring), but resolving a command name still involves bundle
    discovery and file I/O the first time it happens. This wraps that
    construction in :func:`asyncio.to_thread` so `async` callers don't
    block the event loop on it.

    Args:
        command: Envoy command name or raw executable path, forwarded to
            :class:`Environment`.
        **kwargs: Additional keyword arguments forwarded to
            :class:`Environment` (``inherit_env``, ``allowlist``,
            ``bundle_roots``, ``commands_file``).

    Returns:
        The constructed ``Environment``.

    Note:
        ``Environment`` itself only exposes synchronous dict-like access
        (``env["VAR"]``, ``env.get(...)``, ``env.items()``); it does not
        currently expose process-execution methods (those live on
        ``envoy.proc``), so there is no async equivalent of
        ``check_output`` to offer here yet. Revisit if/when ``Environment``
        grows execution methods of its own.

    """
    return await _asyncio.to_thread(Environment, command, **kwargs)


EnvoyError = exceptions.EnvoyError
WrapperError = exceptions.WrapperError
PreRunError = exceptions.PreRunError
PostRunError = exceptions.PostRunError
ExecutionError = exceptions.ExecutionError
CalledProcessError = exceptions.CalledProcessError
EnvironmentBuildError = exceptions.EnvironmentBuildError
CommandNotFoundError = exceptions.CommandNotFoundError
ValidationError = exceptions.ValidationError

_sys.modules[__name__ + ".proc"] = proc
_sys.modules[__name__ + ".testing"] = testing
_sys.modules[__name__ + ".exceptions"] = exceptions
_sys.modules[__name__ + ".telemetry"] = telemetry

__all__ = [
    "_core_version",
    "__version__",
    "OPERATING_SYSTEM",
    "SUPPORTED_OPERATING_SYSTEMS",
    "BUNDLE_CHECKOUT",
    "BUNDLE_DEFAULT_NAMESPACE",
    "TraceAllowlistEvent",
    "TraceStepEvent",
    "UserConfig",
    "ExecutionResult",
    "WrapperConfig",
    "ApplicationWrapper",
    "createWrapper",
    "Command",
    "CommandDefinition",
    "CommandRegistry",
    "Bundle",
    "BundleInfo",
    "Stack",
    "NamedStackEntry",
    "STACK_ROOTS_VAR",
    "USER_CONFIG_PATH",
    "KNOWN_SETTINGS",
    "getConfigRoot",
    "isStackName",
    "resolveNamedStack",
    "listNamedStacks",
    "listStackVersions",
    "findCommandsFile",
    "getEnvironment",
    "getAllowlist",
    "traceEnvironment",
    "diagnoseEnvironment",
    "setApiVerbosity",
    "loadUserConfig",
    "getCurrentStack",
    "getCurrentTeamConfig",
    "enable_telemetry",
    "disable_telemetry",
    "is_telemetry_enabled",
    "cli_main",
    "discoverBundlesAuto",
    "getBundles",
    "loadBundlesFromStack",
    "BundleCache",
    "TeamConfig",
    "UserHostConfig",
    "Vcs",
    "VcsChange",
    "VcsStatus",
    "SemVer",
    "Constraint",
    "VersionSpec",
    "proc",
    "testing",
    "exceptions",
    "telemetry",
    "Environment",
    "async_new_environment",
    "EnvoyError",
    "WrapperError",
    "PreRunError",
    "PostRunError",
    "ExecutionError",
    "CalledProcessError",
    "CommandNotFoundError",
    "EnvironmentBuildError",
    "ValidationError",
]


def _run_pyinit_scripts() -> None:
    """Run Python files from every directory listed in ``ENVOY_PYINIT``.

    Opt-in extension point, off by default: when ``ENVOY_PYINIT`` is unset
    or empty, this is a no-op. Otherwise, each directory in the
    platform-separated (``;`` on Windows, ``:`` elsewhere, matching
    ``ENVOY_BNDL_ROOTS``/``ENVOY_STACK_ROOTS``) environment variable is
    scanned non-recursively for ``*.py`` files, which are run in sorted
    order via :func:`runpy.run_path`. This happens at the very end of
    module initialization so scripts can freely ``import envoy`` and use
    the full public API.

    A script that raises is treated as best-effort, matching the graceful-
    degradation pattern used elsewhere in envoy (e.g. bundle cache open
    failures, see ``envoy_core::bundle_cache::open_default_bundle_cache``):
    the exception is reported to stderr and the remaining scripts still
    run, so one broken script cannot block every subsequent ``import envoy``
    for everyone.

    """
    raw = _os.environ.get("ENVOY_PYINIT", "")
    if not raw.strip():
        return

    separator = ";" if _sys.platform == "win32" else ":"
    for directory in raw.split(separator):
        directory = directory.strip()
        if not directory:
            continue

        directory_path = _Path(directory)
        if not directory_path.is_dir():
            continue

        for script in sorted(directory_path.glob("*.py")):
            try:
                _runpy.run_path(str(script), run_name="__main__")
            except Exception as error:  # intentionally broad: best-effort, see docstring
                print(
                    f"Warning: ENVOY_PYINIT script {script} failed: {error}",
                    file=_sys.stderr,
                )


_run_pyinit_scripts()
