"""Core application wrapper implementation."""

import subprocess
import logging
import signal
import time
from pathlib import Path
from typing import Callable
from contextlib import contextmanager

from ._exceptions import PreRunError, PostRunError, ExecutionError
from ._models import ExecutionResult, WrapperConfig
from ._environment import EnvironmentManager
from ._executor import ProcessExecutor


log = logging.getLogger(__name__)


class ApplicationWrapper:
    """Sophisticated application wrapper with pre/post operations and process control.
    
    Example:
        >>> config = WrapperConfig(
        ...     executable="python",
        ...     args=["script.py", "--verbose"],
        ...     preRun=lambda: print("Starting..."),
        ...     postRun=lambda result: print(f"Done: {result}")
        ... )
        >>> wrapper = ApplicationWrapper(config)
        >>> result = wrapper.run()
        >>> print(result.return_code)
        
    """
    
    def __init__(self, config: WrapperConfig):
        """Initialize the application wrapper.
        
        Args:
            config: WrapperConfig instance with execution settings
            
        """
        self.config = config
        self._process: subprocess.Popen | None = None
        self._interrupted = False
        self._original_sigint_handler = None
        
        # Initialize managers
        self._env_manager = EnvironmentManager(
            inherit_env=config.inherit_env,
            allowlist=config.env_allowlist
        )
        self._executor = ProcessExecutor(
            stream_output=config.stream_output,
            onOutput=config.onOutput,
            onError=config.onError
        )
        
        # Setup logging
        if config.log_execution:
            self._setupLogging()
    
    def _setupLogging(self):
        """Configure logging for wrapper execution.
        
        """
        if not log.handlers:
            handler = logging.StreamHandler()
            formatter = logging.Formatter(
                '%(asctime)s - %(name)s - %(levelname)s - %(message)s'
            )
            handler.setFormatter(formatter)
            log.addHandler(handler)
        log.setLevel(self.config.log_level)
    
    def _handleSignal(self, signum, frame):
        """Handle interrupt signals."""
        log.warning(f"Received signal {signum}, terminating process...")
        self._interrupted = True
        if self._process:
            self._executor.terminateProcess(self._process)
    
    @contextmanager
    def _signalHandlerContext(self):
        """Context manager for signal handling."""
        self._original_sigint_handler = signal.signal(signal.SIGINT, self._handleSignal)
        try:
            yield
        finally:
            signal.signal(signal.SIGINT, self._original_sigint_handler)
    
    def _executePreRun(self):
        """Execute pre-run operations."""
        if not self.config.preRun:
            return
        
        try:
            log.info("Executing pre-run operations...")
            self.config.preRun()
            log.info("Pre-run operations completed")
        except Exception as e:
            log.error(f"Pre-run operation failed: {e}")
            if not self.config.continue_on_pre_run_error:
                raise PreRunError(f"Pre-run operation failed: {e}") from e
    
    def _executePostRun(self, result: ExecutionResult):
        """Execute post-run operations."""
        if not self.config.postRun:
            return
        
        try:
            log.info("Executing post-run operations...")
            self.config.postRun(result)
            log.info("Post-run operations completed")
        except Exception as e:
            log.error(f"Post-run operation failed: {e}")
            if not self.config.continue_on_post_run_error:
                raise PostRunError(f"Post-run operation failed: {e}") from e
    
    def run(self) -> ExecutionResult:
        """Execute the application with configured settings.
        
        Returns:
            ExecutionResult with execution details
            
        Raises:
            WrapperError: On execution failures (if raise_on_error=True)
            
        """
        start_time = time.time()
        result = ExecutionResult(
            return_code=-1,
            command=[]
        )
        
        try:
            # Pre-run operations
            self._executePreRun()
            
            # Build the subprocess environment first so that executable
            # resolution uses the subprocess PATH rather than the envoy
            # process PATH (critical in closed-environment mode).
            env = self._env_manager.prepareEnvironment(
                env_files=self.config.env_files,
                env=self.config.env
            )
            command = self._executor.prepareCommand(
                self.config.executable,
                self.config.args,
                search_path=env.get('PATH'),
            )
            cwd = str(self.config.cwd) if self.config.cwd else None
            
            result.command = command
            
            log.info(f"Executing: {' '.join(command)}")
            if cwd:
                log.info(f"Working directory: {cwd}")
            
            # Setup process arguments
            process_kwargs = {
                'env': env,
                'cwd': cwd,
                'shell': self.config.shell,
            }
            
            if self.config.capture_output or self.config.stream_output:
                process_kwargs['stdout'] = subprocess.PIPE
                process_kwargs['stderr'] = subprocess.PIPE
            
            # Execute with signal handling
            with self._signalHandlerContext():
                self._process = subprocess.Popen(command, **process_kwargs)
                result.pid = self._process.pid
                
                # Notify on start
                if self.config.onStart:
                    try:
                        self.config.onStart(result.pid)
                    except Exception as e:
                        log.warning(f"onStart callback error: {e}")
                
                log.info(f"Process started with PID: {result.pid}")
                
                # Handle output
                if self.config.capture_output or self.config.stream_output:
                    try:
                        stdout, stderr = self._executor.streamProcessOutput(self._process)
                        result.stdout = stdout if stdout else None
                        result.stderr = stderr if stderr else None
                        return_code = self._process.wait(timeout=self.config.timeout)
                    except subprocess.TimeoutExpired:
                        log.error(f"Process timed out after {self.config.timeout}s")
                        self._executor.terminateProcess(self._process)
                        result.timed_out = True
                        return_code = -1
                else:
                    try:
                        return_code = self._process.wait(timeout=self.config.timeout)
                    except subprocess.TimeoutExpired:
                        log.error(f"Process timed out after {self.config.timeout}s")
                        self._executor.terminateProcess(self._process)
                        result.timed_out = True
                        return_code = -1
                
                result.return_code = return_code
                result.execution_time = time.time() - start_time
                
                if self._interrupted:
                    log.warning("Process was interrupted")
                    result.return_code = -2
                
                log.info(f"Process finished: {result}")
                
        except (PreRunError, PostRunError):
            # Re-raise wrapper errors
            raise
        except Exception as e:
            result.execution_time = time.time() - start_time
            log.error(f"Execution failed: {e}")
            if self.config.raise_on_error:
                raise ExecutionError(f"Execution failed: {e}") from e
        finally:
            self._process = None
            
            # Post-run operations (always execute if configured)
            try:
                self._executePostRun(result)
            except PostRunError:
                if not self.config.continue_on_post_run_error:
                    raise
        
        # Check for errors
        if self.config.raise_on_error and not result.success:
            if result.timed_out:
                raise ExecutionError(f"Process timed out after {self.config.timeout}s")
            elif result.return_code != 0:
                raise ExecutionError(
                    f"Process exited with code {result.return_code}\n"
                    f"Command: {' '.join(result.command)}"
                )
        
        return result
    
    def __call__(self) -> ExecutionResult:
        """Allow wrapper to be called as a function."""
        return self.run()
    
    @contextmanager
    def __enter__(self):
        """Context manager entry."""
        yield self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - cleanup if process is still running.
        
        """
        if self._process:
            self._executor.terminateProcess(self._process)


def createWrapper(
    executable: str | Path,
    *args: str,
    env: dict[str, str] | None = None,
    env_files: str | Path | list[str | Path] | None = None,
    preRun: Callable[[], None] | None = None,
    postRun: Callable[[ExecutionResult], None] | None = None,
    **kwargs
) -> ApplicationWrapper:
    """Convenience function to create an ApplicationWrapper.
    
    Args:
        executable: Path to executable or command name
        *args: Command-line arguments
        env: Environment variables to add/update
        env_files: JSON file(s) containing environment variables
        preRun: Pre-run callback
        postRun: Post-run callback
        **kwargs: Additional WrapperConfig parameters
        
    Returns:
        Configured ApplicationWrapper instance
        
    Example:
        >>> wrapper = createWrapper(
        ...     "python", "script.py", "--verbose",
        ...     env_files="config/env.json",
        ...     timeout=60
        ... )
        >>> result = wrapper.run()
        
    """
    config = WrapperConfig(
        executable=executable,
        args=list(args),
        env=env,
        env_files=env_files,
        preRun=preRun,
        postRun=postRun,
        **kwargs
    )
    return ApplicationWrapper(config)
