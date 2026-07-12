"""envoy -- Environment orchestration for managed application execution.

This mixed-layout maturin package re-exports symbols from the compiled
``envoy._envoy`` extension module while preserving the existing
``import envoy`` and ``import envoy.proc`` entry points expected by
downstream consumers.
"""

from __future__ import annotations

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
BundleConfig = _native.BundleConfig
getEnvironment = _native.getEnvironment
getAllowlist = _native.getAllowlist
traceEnvironment = _native.traceEnvironment
setApiVerbosity = _native.setApiVerbosity
loadUserConfig = _native.loadUserConfig
getCurrentBundleConfig = _native.getCurrentBundleConfig
proc = _native.proc
exceptions = _native.exceptions
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

__all__ = [
    "_core_version",
    "__version__",
    "OPERATING_SYSTEM",
    "SUPPORTED_OPERATING_SYSTEMS",
    "TraceAllowlistEvent",
    "TraceStepEvent",
    "UserConfig",
    "BundleConfig",
    "getEnvironment",
    "getAllowlist",
    "traceEnvironment",
    "setApiVerbosity",
    "loadUserConfig",
    "getCurrentBundleConfig",
    "proc",
    "testing",
    "exceptions",
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
