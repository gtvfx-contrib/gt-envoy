"""envoy -- Environment orchestration for managed application execution.

Provides environment isolation, bundle-based discovery, and process launch
facilities for DCC applications and pipeline tools.

Can be used as a Python library or as a CLI tool::

    python -m envoy [command] [args...]

Quickstart (Python API)::

    import envoy as envoy
    import envoy.proc as proc

    # Inspect the prepared environment for a command
    env_dict = envoy.getEnvironment('maya')

    # Launch once
    proc.call(['maya', 'myfile.ma'])

    # Bake env once, launch many
    env = proc.Environment('nuke')
    env.spawn(['comp.nk'])

    # Inspect an individual bundle
    bundle = envoy.Bundle('gt:pythoncore')           # resolve by bndlid via ENVOY_BNDL_ROOTS
    bundle = envoy.Bundle('/repo/gtvfx-contrib/gt/pythoncore')  # or by path
    print(bundle.bndlid)    # 'gt:pythoncore'  (namespace inferred from parent dir)
    print(bundle.name)      # 'pythoncore'
    print(bundle.namespace) # 'gt'
    print(bundle.commands)

    # Load a bundle config file
    cfg = envoy.BundleConfig('/studio/bundles.json')
    for b in cfg.bundles:
        print(b.name, b.commands)

Submodules:
    proc       -- process execution (Environment class and free functions)
    testing    -- test helpers (patchBundleRoots, patchCommandsFile)
    exceptions -- all envoy exception classes
"""

from __future__ import annotations

__all__ = [
    # ---- Public constants ----
    '__version__',
    'OPERATING_SYSTEM',
    'SUPPORTED_OPERATING_SYSTEMS',
    'BUNDLE_CHECKOUT',
    'BUNDLE_DEFAULT_NAMESPACE',
    # ---- Core classes ----
    'ApplicationWrapper',
    'WrapperConfig',
    'ExecutionResult',
    'Command',
    'CommandDefinition',
    'CommandRegistry',
    'Bundle',
    'BundleConfig',
    'BundleInfo',
    # ---- User config ----
    'UserConfig',
    'USER_CONFIG_PATH',
    'KNOWN_SETTINGS',
    # ---- Named config registry ----
    'NamedConfigEntry',
    'CFG_ROOTS_VAR',
    'isConfigName',
    'resolveNamedConfig',
    'listNamedConfigs',
    'listConfigVersions',
    'publishConfig',
    # ---- Exceptions ----
    'EnvoyError',
    'WrapperError',
    'PreRunError',
    'PostRunError',
    'ExecutionError',
    'EnvironmentBuildError',
    'CommandNotFoundError',
    'CalledProcessError',
    'ValidationError',
    # ---- Top-level API functions ----
    'getEnvironment',
    'getAllowlist',
    'setApiVerbosity',
    'traceEnvironment',
    'loadUserConfig',
    'getCurrentBundleConfig',
    # ---- Trace event types ----
    'TraceAllowlistEvent',
    'TraceStepEvent',
    # ---- Utility functions ----
    'createWrapper',
    'findCommandsFile',
    'cli_main',
    # ---- Bundle discovery ----
    'getBundles',
    'discoverBundlesAuto',
    'loadBundlesFromConfig',
    # ---- Submodules ----
    'proc',
    'testing',
    'exceptions',
]

from importlib.metadata import (
    PackageNotFoundError as _PackageNotFoundError,
)
from importlib.metadata import (
    version as _metadata_version,
)

try:
    #: The version of envoy, read from installed package metadata.
    __version__: str = _metadata_version('envoy')
except _PackageNotFoundError:
    # Raw sys.path checkout — use the file written by hatch-vcs at install time.
    try:
        from ._version import __version__  # type: ignore[no-redef]
    except ImportError:
        __version__ = '0.0.0+uninstalled'

# Convenience submodule imports — ``import envoy`` makes these available as
# ``envoy.proc``, ``envoy.testing``, and ``envoy.exceptions``.
# This module eagerly imports these submodules at import time.
from . import (
    exceptions,  # noqa: E402
    proc,  # noqa: E402
    testing,  # noqa: E402
)
from ._api import (
    OPERATING_SYSTEM,
    SUPPORTED_OPERATING_SYSTEMS,
    getAllowlist,
    getCurrentBundleConfig,
    getEnvironment,
    loadUserConfig,
    setApiVerbosity,
    traceEnvironment,
)
from ._cli import main as cli_main
from ._commands import CommandDefinition, CommandRegistry, findCommandsFile
from ._config_registry import (
    CFG_ROOTS_VAR,
    NamedConfigEntry,
    isConfigName,
    listConfigVersions,
    listNamedConfigs,
    publishConfig,
    resolveNamedConfig,
)
from ._discovery import (
    BUNDLE_CHECKOUT,
    BUNDLE_DEFAULT_NAMESPACE,
    Bundle,
    BundleConfig,
    BundleInfo,
    discoverBundlesAuto,
    getBundles,
    loadBundlesFromConfig,
)
from ._environment import TraceAllowlistEvent, TraceStepEvent
from ._exceptions import (
    CalledProcessError,
    CommandNotFoundError,
    EnvironmentBuildError,
    EnvoyError,
    ExecutionError,
    PostRunError,
    PreRunError,
    ValidationError,
    WrapperError,
)
from ._models import ExecutionResult, WrapperConfig
from ._user_config import KNOWN_SETTINGS, USER_CONFIG_PATH, UserConfig
from ._wrapper import ApplicationWrapper, createWrapper

#: Public alias for :class:`~._commands.CommandDefinition`.
#:
#: Exposes ``.name``, ``.alias``, ``.bundle``, ``.environment``,
#: ``.executable``, and ``.base_args``.
Command = CommandDefinition
