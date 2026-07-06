"""envoy.exceptions -- Public exception module.

All envoy exceptions are accessible here.  The most commonly needed class,
:class:`CalledProcessError`, is also available on :mod:`envoy.proc` and is
the **same object** in both places::

    import envoy.exceptions as exceptions
    import envoy.proc as proc

    assert exceptions.CalledProcessError is proc.CalledProcessError  # True

Usage::

    from envoy.exceptions import EnvoyError, CalledProcessError

    try:
        proc.checkCall(['maya', 'bad_scene.ma'])
    except CalledProcessError as e:
        print(f'Process exited {e.returncode}')
    except EnvoyError as e:
        print(f'Envoy error: {e}')

"""

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

__all__ = [
    "EnvoyError",
    "WrapperError",
    "PreRunError",
    "PostRunError",
    "ExecutionError",
    "CalledProcessError",
    "EnvironmentBuildError",
    "CommandNotFoundError",
    "ValidationError",
]
