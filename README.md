# Application Wrapper

A sophisticated Python module for wrapping application execution with pre/post operations, environment management, and comprehensive process control.

## Features

### Core Capabilities
- ✅ **Pre/Post Run Operations** - Execute setup and teardown code
- ✅ **Environment Management** - Full control over environment variables
- ✅ **Output Handling** - Capture, stream, or suppress stdout/stderr
- ✅ **Timeout Support** - Automatic process termination after timeout
- ✅ **Return Code Handling** - Capture and propagate exit codes
- ✅ **Signal Handling** - Graceful cleanup on interrupts (Ctrl+C)
- ✅ **Working Directory** - Set custom working directory for subprocess
- ✅ **Path Resolution** - Automatic executable lookup in PATH

### Advanced Features
- ✅ **Context Manager** - Clean resource management with `with` statement
- ✅ **Event Callbacks** - Multiple hooks (on_start, on_output, on_error)
- ✅ **Process Information** - Access PID, execution time, command details
- ✅ **Comprehensive Logging** - Built-in logging with configurable levels
- ✅ **Error Control** - Fine-grained error handling options
- ✅ **Platform Support** - Windows and Unix compatibility

## Installation

The module is part of the `gt.app.wrapper` namespace package.

```python
from gt.app.wrapper import ApplicationWrapper, WrapperConfig, create_wrapper
```

## Quick Start

### Basic Usage

```python
from gt.app.wrapper import WrapperConfig, ApplicationWrapper

config = WrapperConfig(
    executable="python",
    args=["script.py", "--verbose"],
    capture_output=True
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

print(f"Return code: {result.return_code}")
print(f"Output: {result.stdout}")
```

### Using the Convenience Function

```python
from gt.app.wrapper import create_wrapper

wrapper = create_wrapper(
    "python",
    "script.py",
    "--verbose",
    timeout=60,
    capture_output=True
)

result = wrapper.run()
```

### With Pre/Post Operations

```python
from gt.app.wrapper import WrapperConfig, ApplicationWrapper

def setup():
    print("Setting up environment...")
    # Perform pre-execution tasks

def cleanup(result):
    print(f"Cleaning up... Exit code: {result.return_code}")
    # Perform post-execution tasks

config = WrapperConfig(
    executable="myapp",
    args=["--input", "data.txt"],
    pre_run=setup,
    post_run=cleanup,
    env={"MY_VAR": "value"}
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()
```

## Configuration

### WrapperConfig Parameters

#### Core Settings
- **executable** (str | Path) - Path to executable or command name
- **args** (List[str]) - Command-line arguments

#### Environment
- **env** (Dict[str, str] | None) - Environment variables to add/update
- **inherit_env** (bool) - Inherit parent process environment (default: True)

#### Working Directory
- **cwd** (str | Path | None) - Working directory for subprocess

#### Output Handling
- **capture_output** (bool) - Capture stdout/stderr (default: False)
- **stream_output** (bool) - Stream output in real-time (default: True)

#### Execution Control
- **timeout** (float | None) - Timeout in seconds (None = no timeout)
- **shell** (bool) - Run through shell (default: False)

#### Callbacks
- **pre_run** (Callable[[], None]) - Called before execution
- **post_run** (Callable[[ExecutionResult], None]) - Called after execution
- **on_start** (Callable[[int], None]) - Called when process starts (receives PID)
- **on_output** (Callable[[str], None]) - Called for each stdout line
- **on_error** (Callable[[str], None]) - Called for each stderr line

#### Error Handling
- **raise_on_error** (bool) - Raise exception on non-zero exit (default: True)
- **continue_on_pre_run_error** (bool) - Continue if pre_run fails (default: False)
- **continue_on_post_run_error** (bool) - Continue if post_run fails (default: True)

#### Logging
- **log_execution** (bool) - Enable logging (default: True)
- **log_level** (int) - Logging level (default: logging.INFO)

## ExecutionResult

The `run()` method returns an `ExecutionResult` object with:

- **return_code** (int) - Process exit code
- **stdout** (str | None) - Captured stdout (if capture_output=True)
- **stderr** (str | None) - Captured stderr (if capture_output=True)
- **execution_time** (float) - Execution duration in seconds
- **pid** (int | None) - Process ID
- **command** (List[str]) - Full command that was executed
- **timed_out** (bool) - Whether process was terminated due to timeout
- **success** (bool) - Property: True if return_code==0 and not timed_out

## Examples

### Environment Variables

```python
config = WrapperConfig(
    executable="python",
    args=["script.py"],
    env={
        "API_KEY": "secret",
        "DEBUG": "1"
    }
)
```

### Timeout Handling

```python
config = WrapperConfig(
    executable="long_running_task",
    timeout=30.0,  # Kill after 30 seconds
    raise_on_error=False  # Don't raise exception on timeout
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

if result.timed_out:
    print("Task exceeded time limit")
```

### Event Callbacks

```python
def on_start(pid):
    print(f"Process {pid} started")

def on_output(line):
    if "ERROR" in line:
        # Handle error lines differently
        log_error(line)

config = WrapperConfig(
    executable="build.sh",
    on_start=on_start,
    on_output=on_output,
    stream_output=False  # Use callbacks instead
)
```

### Context Manager

```python
config = WrapperConfig(executable="server", args=["--port", "8080"])

with ApplicationWrapper(config) as wrapper:
    result = wrapper.run()
    # Automatic cleanup on exit
```

### Working Directory

```python
from pathlib import Path

config = WrapperConfig(
    executable="make",
    args=["build"],
    cwd=Path("/path/to/project")
)
```

### Custom Error Handling

```python
def safe_cleanup(result):
    try:
        # Cleanup operations that might fail
        cleanup_temp_files()
    except Exception as e:
        print(f"Cleanup warning: {e}")

config = WrapperConfig(
    executable="risky_app",
    post_run=safe_cleanup,
    continue_on_post_run_error=True,  # Don't fail if cleanup fails
    raise_on_error=False  # Handle errors manually
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()

if not result.success:
    # Custom error handling
    send_alert(f"App failed: {result.return_code}")
```

## Exception Handling

The module defines specific exceptions:

- **WrapperError** - Base exception
- **PreRunError** - Error during pre_run operation
- **PostRunError** - Error during post_run operation
- **ExecutionError** - Error during process execution

```python
from gt.app.wrapper import ExecutionError, PreRunError

try:
    result = wrapper.run()
except PreRunError as e:
    print(f"Setup failed: {e}")
except ExecutionError as e:
    print(f"Execution failed: {e}")
```

## Real-World Example: Build System

```python
import time
from gt.app.wrapper import WrapperConfig, ApplicationWrapper

def pre_build():
    print("🔨 Preparing build environment...")
    clean_artifacts()
    validate_dependencies()

def post_build(result):
    if result.success:
        print("✅ Build successful!")
        archive_artifacts()
        run_tests()
    else:
        print(f"❌ Build failed: {result.return_code}")
        notify_team(result)

def on_output(line):
    # Highlight errors and warnings
    if "error" in line.lower():
        print(f"❌ {line}")
    elif "warning" in line.lower():
        print(f"⚠️  {line}")

config = WrapperConfig(
    executable="cmake",
    args=["--build", ".", "--config", "Release"],
    cwd="/path/to/project/build",
    env={"CMAKE_BUILD_TYPE": "Release"},
    pre_run=pre_build,
    post_run=post_build,
    on_output=on_output,
    timeout=1800,  # 30 minutes
    raise_on_error=True
)

wrapper = ApplicationWrapper(config)
result = wrapper.run()
```

## Best Practices

1. **Use pre_run for validation** - Check prerequisites before execution
2. **Set appropriate timeouts** - Prevent infinite hangs
3. **Handle cleanup in post_run** - Always clean up resources
4. **Use callbacks for real-time monitoring** - Process output as it happens
5. **Enable logging for debugging** - Helps troubleshoot issues
6. **Set raise_on_error appropriately** - Decide error handling strategy
7. **Use context managers** - Ensure cleanup even on exceptions

## Testing

Run the included examples:

```bash
python examples.py
```

## License

Part of the GT Tools collection.
