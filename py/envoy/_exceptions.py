"""Exception classes for envoy.

Hierarchy::

    EnvoyError
    ├── WrapperError          (back-compat alias for EnvoyError)
    │   ├── PreRunError
    │   ├── PostRunError
    │   └── ExecutionError
    ├── CalledProcessError    (also inherits subprocess.CalledProcessError)
    ├── EnvironmentBuildError
    ├── CommandNotFoundError
    └── ValidationError

This module is re-exported as the public ``envoy.exceptions`` submodule,
and ``CalledProcessError`` is also re-exported on ``envoy.proc`` so that
the following identity holds::

    envoy.exceptions.CalledProcessError is envoy.proc.CalledProcessError

"""

from __future__ import annotations

import subprocess
from typing import Union
from os import PathLike


# ---------------------------------------------------------------------------
# Root
# ---------------------------------------------------------------------------

class EnvoyError(Exception):
    """Root base class for all envoy exceptions.

    Analogous to ``bl.exceptions.BlError``.  Catching this class will catch
    any exception raised by the envoy package.

    """


# ---------------------------------------------------------------------------
# Back-compat wrapper hierarchy
# ---------------------------------------------------------------------------

class WrapperError(EnvoyError):
    """Back-compat base for application-wrapper errors.

    Equivalent to ``EnvoyError``; retained so existing code that catches
    ``WrapperError`` continues to work.

    """


class PreRunError(WrapperError):
    """Error occurred during pre-run operations."""


class PostRunError(WrapperError):
    """Error occurred during post-run operations."""


class ExecutionError(WrapperError):
    """Error occurred during application execution."""


# ---------------------------------------------------------------------------
# Process errors
# ---------------------------------------------------------------------------

class CalledProcessError(EnvoyError, subprocess.CalledProcessError):
    """Raised when a checked envoy subprocess call exits with a non-zero code.

    Inherits from both :class:`EnvoyError` and
    :class:`subprocess.CalledProcessError` so it can be used wherever either
    base class is expected.

    This class is exported from both ``envoy.proc`` and
    ``envoy.exceptions``; they are the **same object**::

        envoy.exceptions.CalledProcessError is envoy.proc.CalledProcessError

    """

    def __init__(self, returncode: int, cmd: Union[str, bytes, PathLike], output=None, stderr=None):
        # subprocess.CalledProcessError.__init__ requires positional args.
        subprocess.CalledProcessError.__init__(self, returncode, cmd, output, stderr)


# ---------------------------------------------------------------------------
# Environment / validation errors
# ---------------------------------------------------------------------------

class EnvironmentBuildError(EnvoyError):
    """Failed to construct the subprocess environment for a command.

    Raised when environment files are missing, contain invalid JSON, or when
    variable expansion or path resolution fails during env preparation.

    """


class CommandNotFoundError(EnvoyError):
    """Named command does not exist in the loaded registry.

    Raised by the Python API (``envoy.proc``) when a caller references a
    command that is not registered in any discovered bundle or commands file.

    """


class ValidationError(EnvoyError):
    """A value provided to an envoy API failed validation.

    Analogous to ``bl.exceptions.ValidationError``.
    
    """
