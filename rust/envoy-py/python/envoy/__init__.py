"""envoy -- Environment orchestration for managed application execution.

This mixed-layout maturin package re-exports symbols from the compiled
``envoy._envoy`` extension module while preserving the existing
``import envoy`` and ``import envoy.proc`` entry points expected by
downstream consumers.
"""

from __future__ import annotations

import asyncio as _asyncio
import importlib
import sys as _sys

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
