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
# Wrapper bindings.
ExecutionResult = _native.ExecutionResult
WrapperConfig = _native.WrapperConfig
ApplicationWrapper = _native.ApplicationWrapper
createWrapper = _native.createWrapper
# Bundle discovery bindings.
BUNDLE_CHECKOUT = _native.BUNDLE_CHECKOUT
BUNDLE_DEFAULT_NAMESPACE = _native.BUNDLE_DEFAULT_NAMESPACE
Bundle = _native.Bundle
BundleConfig = _native.BundleConfig
BundleInfo = _native.BundleInfo
discoverBundlesAuto = _native.discoverBundlesAuto
getBundles = _native.getBundles
loadBundlesFromConfig = _native.loadBundlesFromConfig
# Command bindings.
CommandDefinition = _native.CommandDefinition
CommandRegistry = _native.CommandRegistry
Command = _native.Command
findCommandsFile = _native.findCommandsFile

# Named-config registry and user-config metadata bindings.
NamedConfigEntry = _native.NamedConfigEntry
CFG_ROOTS_VAR = _native.CFG_ROOTS_VAR
USER_CONFIG_PATH = _native.USER_CONFIG_PATH
KNOWN_SETTINGS = _native.KNOWN_SETTINGS
isConfigName = _native.isConfigName
resolveNamedConfig = _native.resolveNamedConfig
listNamedConfigs = _native.listNamedConfigs
listConfigVersions = _native.listConfigVersions
publishConfig = _native.publishConfig

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
    "BundleConfig",
    "BundleInfo",
    "NamedConfigEntry",
    "CFG_ROOTS_VAR",
    "USER_CONFIG_PATH",
    "KNOWN_SETTINGS",
    "isConfigName",
    "resolveNamedConfig",
    "listNamedConfigs",
    "listConfigVersions",
    "publishConfig",
    "findCommandsFile",
    "getEnvironment",
    "getAllowlist",
    "traceEnvironment",
    "setApiVerbosity",
    "loadUserConfig",
    "getCurrentBundleConfig",
    "discoverBundlesAuto",
    "getBundles",
    "loadBundlesFromConfig",
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
