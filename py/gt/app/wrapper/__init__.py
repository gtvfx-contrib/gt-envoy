"""
Application Wrapper Module

Provides sophisticated application execution with pre/post operations,
environment management, and comprehensive process control.
"""

from ._exceptions import (
    WrapperError,
    PreRunError,
    PostRunError,
    ExecutionError
)
from ._models import (
    ExecutionResult,
    WrapperConfig
)
from ._wrapper import (
    ApplicationWrapper,
    create_wrapper
)


__all__ = [
    # Core classes
    'ApplicationWrapper',
    'WrapperConfig',
    'ExecutionResult',
    
    # Exceptions
    'WrapperError',
    'PreRunError',
    'PostRunError',
    'ExecutionError',
    
    # Utility functions
    'create_wrapper',
]
