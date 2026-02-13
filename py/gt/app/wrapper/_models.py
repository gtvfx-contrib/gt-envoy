"""Data models for the application wrapper module."""

import logging
from pathlib import Path
from typing import Callable
from dataclasses import dataclass, field


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
    inherit_env: bool = True
    
    # Working directory
    cwd: str | Path | None = None
    
    # Output handling
    capture_output: bool = False
    stream_output: bool = True
    
    # Execution control
    timeout: float | None = None
    shell: bool = False
    
    # Callbacks
    pre_run: Callable[[], None] | None = None
    post_run: Callable[['ExecutionResult'], None] | None = None
    on_start: Callable[[int], None] | None = None  # Receives PID
    on_output: Callable[[str], None] | None = None  # Receives output line
    on_error: Callable[[str], None] | None = None  # Receives error line
    
    # Error handling
    raise_on_error: bool = True
    continue_on_pre_run_error: bool = False
    continue_on_post_run_error: bool = True
    
    # Logging
    log_execution: bool = True
    log_level: int = logging.INFO
