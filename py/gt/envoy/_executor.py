"""Process execution handling for ApplicationWrapper."""

import os
import sys
import subprocess
import shutil
import logging
from pathlib import Path
from typing import Callable

from ._exceptions import WrapperError


log = logging.getLogger(__name__)


class ProcessExecutor:
    """Handles subprocess execution, output streaming, and process control.
    
    Manages:
    - Executable resolution
    - Command preparation
    - Process spawning and monitoring
    - Output streaming (stdout/stderr)
    - Process termination (graceful/forceful)
    
    """
    
    def __init__(
        self,
        stream_output: bool = True,
        on_output: Callable[[str], None] | None = None,
        on_error: Callable[[str], None] | None = None
    ):
        """Initialize the process executor.
        
        Args:
            stream_output: Whether to stream output to stdout/stderr
            on_output: Callback for stdout lines
            on_error: Callback for stderr lines
            
        """
        self.stream_output = stream_output
        self.on_output = on_output
        self.on_error = on_error
    
    @staticmethod
    def resolve_executable(executable: str | Path, search_path: str | None = None) -> str:
        """Resolve executable path, checking PATH if necessary.
        
        Args:
            executable: Executable name or path
            search_path: Value of PATH to search when resolving bare executable
                names.  Should be the subprocess PATH built from env files, not
                the envoy process PATH.  Falls back to the system PATH if None.
            
        Returns:
            Absolute path to executable
            
        Raises:
            WrapperError: If executable cannot be found
            
        """
        exe = str(executable)
        
        # If it's an absolute path or relative path with directory separators
        if os.path.isabs(exe) or os.path.dirname(exe):
            if not os.path.exists(exe):
                raise WrapperError(f"Executable not found: {exe}")
            return os.path.abspath(exe)
        
        # Search in the subprocess PATH (or system PATH if not provided)
        found = shutil.which(exe, path=search_path)
        if found:
            return found
        
        raise WrapperError(f"Executable '{exe}' not found in PATH")
    
    def prepare_command(
        self, 
        executable: str | Path, 
        args: list[str],
        search_path: str | None = None,
    ) -> list[str]:
        """Prepare the full command to execute.
        
        Args:
            executable: Executable name or path
            args: Command-line arguments
            search_path: PATH string to use for bare-name resolution.  Pass the
                subprocess env PATH so the correct executable is found even in
                closed-environment mode.
            
        Returns:
            List of command components
            
        """
        exe = self.resolve_executable(executable, search_path=search_path)
        return [exe] + list(args)
    
    def stream_process_output(self, process: subprocess.Popen) -> tuple[str, str]:
        """Stream output from process in real-time.
        
        Args:
            process: Running subprocess
            
        Returns:
            Tuple of (stdout, stderr) as strings
            
        """
        stdout_lines = []
        stderr_lines = []
        
        # Read stdout
        if process.stdout:
            for line in iter(process.stdout.readline, b''):
                if not line:
                    break
                decoded = line.decode('utf-8', errors='replace').rstrip()
                stdout_lines.append(decoded)
                
                if self.stream_output:
                    print(decoded, file=sys.stdout, flush=True)
                
                if self.on_output:
                    try:
                        self.on_output(decoded)
                    except Exception as e:
                        log.warning(f"on_output callback error: {e}")
        
        # Read stderr
        if process.stderr:
            for line in iter(process.stderr.readline, b''):
                if not line:
                    break
                decoded = line.decode('utf-8', errors='replace').rstrip()
                stderr_lines.append(decoded)
                
                if self.stream_output:
                    print(decoded, file=sys.stderr, flush=True)
                
                if self.on_error:
                    try:
                        self.on_error(decoded)
                    except Exception as e:
                        log.warning(f"on_error callback error: {e}")
        
        return '\n'.join(stdout_lines), '\n'.join(stderr_lines)
    
    @staticmethod
    def terminate_process(process: subprocess.Popen | None) -> None:
        """Terminate a running process gracefully.
        
        Attempts graceful termination first, then forces kill if needed.
        
        Args:
            process: Process to terminate (None is safe to pass)
            
        """
        if not process:
            return
        
        try:
            # Try graceful termination first
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                # Force kill if termination takes too long
                log.warning("Process did not terminate gracefully, forcing kill...")
                process.kill()
                process.wait()
        except Exception as e:
            log.error(f"Error terminating process: {e}")
