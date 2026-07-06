"""Data models for the application wrapper module."""

import logging
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class ExecutionResult:
    """Container for execution results."""

    return_code: int
    stdout: str | None = None
    stderr: str | None = None
    execution_time: float = 0.0
    pid: int | None = None
    command: list[str] = field(default_factory=list)
    timed_out: bool = False

    @property
    def success(self) -> bool:
        """Check if execution was successful."""
        return self.return_code == 0 and not self.timed_out

    def __repr__(self) -> str:
        status = "SUCCESS" if self.success else f"FAILED (code={self.return_code})"
        return f"ExecutionResult({status}, time={self.execution_time:.2f}s, pid={self.pid})"


@dataclass
class WrapperConfig:
    """Configuration for application wrapper."""

    # Core settings
    executable: str | Path
    args: list[str] = field(default_factory=list)

    # Environment
    env: dict[str, str] | None = None
    env_files: str | Path | list[str | Path] | None = (
        None  # JSON file(s) with environment variables
    )
    inherit_env: bool = False
    env_allowlist: set[str] | None = None  # System vars to inherit in closed mode

    # Working directory
    cwd: str | Path | None = None

    # Output handling
    capture_output: bool = False
    stream_output: bool = True

    # Execution control
    timeout: float | None = None
    shell: bool = False

    # Callbacks
    # These use an intentional onX/preX event-handler naming style,
    # consistent across the whole public callback API (see also
    # _executor.py, _wrapper.py) — not a case of unintentional mixedCase.
    preRun: Callable[[], None] | None = None  # noqa: N815
    postRun: Callable[['ExecutionResult'], None] | None = None  # noqa: N815
    onStart: Callable[[int], None] | None = None  # Receives PID  # noqa: N815
    onOutput: Callable[[str], None] | None = None  # Receives output line  # noqa: N815
    onError: Callable[[str], None] | None = None  # Receives error line  # noqa: N815

    # Error handling
    raise_on_error: bool = True
    continue_on_pre_run_error: bool = False
    continue_on_post_run_error: bool = True

    # Logging
    log_execution: bool = True
    log_level: int = logging.INFO
