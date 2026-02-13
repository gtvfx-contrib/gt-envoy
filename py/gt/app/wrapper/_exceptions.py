"""Exception classes for the application wrapper module."""



class WrapperError(Exception):
    """Base exception for application wrapper errors."""
    pass


class PreRunError(WrapperError):
    """Error occurred during pre-run operations."""
    pass


class PostRunError(WrapperError):
    """Error occurred during post-run operations."""
    pass


class ExecutionError(WrapperError):
    """Error occurred during application execution."""
    pass
