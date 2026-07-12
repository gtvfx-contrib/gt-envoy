"""envoy -- Environment orchestration for managed application execution.

This mixed-layout maturin package re-exports symbols from the compiled
``envoy._envoy`` extension module while preserving the existing
``import envoy`` and ``import envoy.proc`` entry points expected by
downstream consumers.
"""

from __future__ import annotations

import importlib
import sys as _sys

_native = importlib.import_module("._envoy", __name__)

_core_version = _native._core_version
proc = _native.proc
CalledProcessError = proc.CalledProcessError
CommandNotFoundError = proc.CommandNotFoundError
EnvironmentBuildError = proc.EnvironmentBuildError

_sys.modules[__name__ + ".proc"] = proc

__all__ = [
    "_core_version",
    "proc",
    "CalledProcessError",
    "CommandNotFoundError",
    "EnvironmentBuildError",
]
