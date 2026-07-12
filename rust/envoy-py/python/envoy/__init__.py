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
CalledProcessError = proc.CalledProcessError
CommandNotFoundError = proc.CommandNotFoundError
EnvironmentBuildError = proc.EnvironmentBuildError

_sys.modules[__name__ + ".proc"] = proc
_sys.modules[__name__ + ".testing"] = testing

__all__ = [
    "_core_version",
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
    "CalledProcessError",
    "CommandNotFoundError",
    "EnvironmentBuildError",
]
